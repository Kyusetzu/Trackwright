//! Kommandozeile von Trackwright.
//!
//! ```text
//! trackwright plan  --preset anime.toml /media/Serien/X     # zeigt den Plan, ändert nichts
//! trackwright apply --preset anime.toml --mode replace /media/Serien/X
//! ```

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use trackwright_core::engine::{PlanIssue, PlanOptions, language_suggestions, plan_file};
use trackwright_core::env::check_tools;
use trackwright_core::fsutil::{FoundFile, collect_mkv, state_dir};
use trackwright_core::job::{
    JobEvent, JobOptions, OriginalLocation, OutputMode, Snapshot, planned_commands, run_job,
};
use trackwright_core::model::MediaInfo;
use trackwright_core::plan::{FilePlan, Strategy};
use trackwright_core::preset::{BUILTIN, Preset, list_presets, presets_dir};
use trackwright_core::probe::{ProbeCache, probe_many};
use trackwright_core::rules::RuleSet;
use trackwright_core::tools::Tools;

#[derive(Parser)]
#[command(
    name = "trackwright",
    version,
    about = "MKV-Spuren im Batch entfernen und Flags setzen (mit MKVToolNix)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Spuren von Dateien anzeigen.
    Probe {
        /// Dateien oder Ordner (rekursiv, *.mkv).
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Ausgabe als JSON.
        #[arg(long)]
        json: bool,
    },
    /// Plan anzeigen (Trockenlauf mit exakten Befehlen). Ändert nichts.
    Plan {
        #[command(flatten)]
        rules: RuleArgs,
        #[command(flatten)]
        output: OutputArgs,
        /// Plan als JSON ausgeben.
        #[arg(long)]
        json: bool,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Plan ausführen.
    Apply {
        #[command(flatten)]
        rules: RuleArgs,
        #[command(flatten)]
        output: OutputArgs,
        /// Gleichzeitige Dateien (Remux ist I/O-gebunden; auf SMB lieber 1).
        #[arg(long, short = 'j', default_value_t = 1)]
        jobs: usize,
        /// Keinen Header-Snapshot vor In-place-Änderungen anlegen.
        #[arg(long)]
        no_snapshot: bool,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// In-place-Änderung über einen Snapshot rückgängig machen.
    Undo {
        snapshot: PathBuf,
        /// Nur den Befehl anzeigen.
        #[arg(long)]
        dry_run: bool,
    },
    /// Verfügbare Presets auflisten.
    Presets,
}

#[derive(Args)]
struct RuleArgs {
    /// Preset: Pfad zu einer TOML-Datei, Name im Preset-Ordner oder mitgeliefert (anime, nur-deutsch).
    #[arg(long, short = 'p')]
    preset: Option<String>,
    /// Audio behalten, z. B. "Nr. 1 ODER Sprache in {ger, jpn}".
    #[arg(long, value_name = "AUSDRUCK")]
    audio_keep: Option<String>,
    /// Audio Standard-Flag: Ausdruck, "keine" oder "unverändert".
    #[arg(long, value_name = "REGEL")]
    audio_default: Option<String>,
    /// Audio Forced-Flag.
    #[arg(long, value_name = "REGEL")]
    audio_forced: Option<String>,
    /// Untertitel behalten.
    #[arg(long, value_name = "AUSDRUCK")]
    subs_keep: Option<String>,
    /// Untertitel Standard-Flag.
    #[arg(long, value_name = "REGEL")]
    subs_default: Option<String>,
    /// Untertitel Forced-Flag.
    #[arg(long, value_name = "REGEL")]
    subs_forced: Option<String>,
    /// Nur Dateien mit gleicher Spursignatur wie diese Referenzdatei bearbeiten.
    #[arg(long, value_name = "DATEI")]
    reference: Option<PathBuf>,
    /// Auch Dateien bearbeiten, bei denen keine Audiospur übrig bliebe (sonst übersprungen).
    #[arg(long)]
    allow_no_audio: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum ModeArg {
    /// Neue Datei neben dem Original (Suffix).
    Beside,
    /// In einen Ausgabeordner (Struktur gespiegelt).
    Dir,
    /// Original ersetzen (Original in den Papierkorb).
    Replace,
}

#[derive(Args)]
struct OutputArgs {
    /// Ausgabemodus.
    #[arg(long, short = 'm', value_enum, default_value = "beside")]
    mode: ModeArg,
    /// Suffix für --mode beside.
    #[arg(long, default_value = ".trackwright")]
    suffix: String,
    /// Ausgabeordner für --mode dir.
    #[arg(long, value_name = "ORDNER")]
    out: Option<PathBuf>,
    /// Für --mode replace: Original nach _trackwright-original/ verschieben, wenn kein Papierkorb
    /// verfügbar ist (z. B. SMB).
    #[arg(long)]
    original_folder: bool,
}

impl OutputArgs {
    fn mode(&self) -> Result<OutputMode> {
        Ok(match self.mode {
            ModeArg::Beside => OutputMode::Beside {
                suffix: self.suffix.clone(),
            },
            ModeArg::Dir => OutputMode::Directory {
                dir: self
                    .out
                    .clone()
                    .ok_or_else(|| anyhow!("--mode dir braucht --out <ORDNER>"))?,
            },
            ModeArg::Replace => OutputMode::Replace {
                fallback_dir: self.original_folder,
            },
        })
    }
}

fn load_preset(spec: &str) -> Result<Preset> {
    let as_path = Path::new(spec);
    if as_path.exists() {
        return Ok(Preset::load(as_path)?);
    }
    if let Some(dir) = presets_dir() {
        let candidate = dir.join(format!("{spec}.toml"));
        if candidate.exists() {
            return Ok(Preset::load(&candidate)?);
        }
    }
    if let Some((name, src)) = BUILTIN.iter().find(|(n, _)| *n == spec) {
        return Ok(Preset::parse(src, Path::new(name))?);
    }
    bail!("Preset „{spec}“ nicht gefunden (Pfad, Name im Preset-Ordner oder: anime, nur-deutsch)")
}

impl RuleArgs {
    fn rules(&self) -> Result<RuleSet> {
        let mut rules = match &self.preset {
            Some(p) => load_preset(p)?.rules(),
            None => RuleSet::default(),
        };
        let parse_keep = |s: &Option<String>, target: &mut Option<_>| -> Result<()> {
            if let Some(s) = s {
                *target = Some(s.parse().with_context(|| format!("Ausdruck „{s}“"))?);
            }
            Ok(())
        };
        parse_keep(&self.audio_keep, &mut rules.audio.keep)?;
        parse_keep(&self.subs_keep, &mut rules.subtitles.keep)?;
        for (src, target) in [
            (&self.audio_default, &mut rules.audio.default),
            (&self.audio_forced, &mut rules.audio.forced),
            (&self.subs_default, &mut rules.subtitles.default),
            (&self.subs_forced, &mut rules.subtitles.forced),
        ] {
            if let Some(s) = src {
                *target = s.parse().with_context(|| format!("Regel „{s}“"))?;
            }
        }
        Ok(rules)
    }
}

/// Eingelesene Datei samt Plan oder Grund fürs Überspringen.
struct Item {
    found: FoundFile,
    media: Option<MediaInfo>,
    outcome: std::result::Result<FilePlan, String>,
}

fn probe_all(
    tools: &Tools,
    paths: &[PathBuf],
) -> Result<Vec<(FoundFile, std::result::Result<MediaInfo, String>)>> {
    let (found, errors) = collect_mkv(paths);
    for e in &errors {
        eprintln!("Warnung: {e}");
    }
    if found.is_empty() {
        bail!("keine .mkv-Dateien gefunden");
    }
    let cache = ProbeCache::new();
    let file_paths: Vec<PathBuf> = found.iter().map(|f| f.path.clone()).collect();
    let results: Mutex<Vec<Option<std::result::Result<MediaInfo, String>>>> =
        Mutex::new(vec![None; found.len()]);
    let done = AtomicUsize::new(0);
    let total = found.len();
    probe_many(tools, &cache, &file_paths, 4, |i, r| {
        if let Ok(mut v) = results.lock() {
            v[i] = Some(r.map_err(|e| e.to_string()));
        }
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        if total > 10 {
            eprint!("\rEinlesen: {n}/{total}");
        }
    });
    if total > 10 {
        eprintln!();
    }
    let results = results
        .into_inner()
        .map_err(|_| anyhow!("interner Fehler"))?;
    Ok(found
        .into_iter()
        .zip(results)
        .map(|(f, r)| (f, r.unwrap_or_else(|| Err("nicht eingelesen".into()))))
        .collect())
}

fn plan_all(tools: &Tools, args: &RuleArgs, paths: &[PathBuf]) -> Result<Vec<Item>> {
    let rules = args.rules()?;
    let opts = PlanOptions {
        reference: match &args.reference {
            Some(r) => Some(
                trackwright_core::probe::probe(tools, r)
                    .with_context(|| format!("Referenzdatei {}", r.display()))?
                    .signature(),
            ),
            None => None,
        },
        allow_no_audio: args.allow_no_audio,
    };
    Ok(probe_all(tools, paths)?
        .into_iter()
        .map(|(found, media)| match media {
            Ok(m) => {
                let outcome =
                    plan_file(&m, &rules, None, &opts).map_err(|e: PlanIssue| e.to_string());
                Item {
                    found,
                    media: Some(m),
                    outcome,
                }
            }
            Err(e) => Item {
                found,
                media: None,
                outcome: Err(e),
            },
        })
        .collect())
}

fn gb(bytes: u64) -> String {
    format!("{:.2} GB", bytes as f64 / 1e9)
}

fn print_plan(items: &[Item], mode: &OutputMode) {
    let (mut remux, mut propedit, mut nothing, mut skipped, mut bytes) = (0, 0, 0, 0, 0u64);
    for item in items {
        match &item.outcome {
            Ok(plan) => {
                println!("▶ {}  [{}]", plan.path.display(), plan.strategy.label());
                print!("{}", plan.render_diff());
                for w in &plan.warnings {
                    println!("  ! {w}");
                }
                if let Some(m) = &item.media {
                    for s in language_suggestions(m) {
                        println!(
                            "  ? {} {}: „{}“ → Vorschlag {}",
                            s.kind, s.type_index, s.name, s.lang
                        );
                    }
                }
                match planned_commands(plan, Some(&item.found.root), mode) {
                    Ok(cmds) => {
                        for c in cmds {
                            println!("  $ {}", c.to_shell());
                        }
                    }
                    Err(e) => println!("  ! {e}"),
                }
                match plan.strategy {
                    Strategy::Remux => remux += 1,
                    Strategy::Propedit => propedit += 1,
                    Strategy::Nothing => nothing += 1,
                }
                bytes += plan.write_bytes();
            }
            Err(e) => {
                skipped += 1;
                println!("✗ {}  [übersprungen]\n  ! {e}", item.found.path.display());
            }
        }
        println!();
    }
    println!(
        "Summe: {} Dateien – {remux} Remux ({} zu schreiben), {propedit} nur Header, \
         {nothing} unverändert, {skipped} übersprungen",
        items.len(),
        gb(bytes)
    );
}

fn cmd_apply(
    tools: &Tools,
    items: Vec<Item>,
    mode: OutputMode,
    jobs: usize,
    snapshot: bool,
    cancel: &AtomicBool,
) -> Result<bool> {
    let opts = JobOptions {
        mode,
        snapshot_dir: if snapshot {
            state_dir().map(|d| d.join("snapshots"))
        } else {
            None
        },
    };
    let mut all_ok = true;
    let work: Vec<(FoundFile, MediaInfo, FilePlan)> = items
        .into_iter()
        .filter_map(|it| match (it.outcome, it.media) {
            (Ok(plan), Some(media)) if plan.strategy != Strategy::Nothing => {
                Some((it.found, media, plan))
            }
            (Ok(_), _) => None,
            (Err(e), _) => {
                all_ok = false;
                eprintln!("✗ {}: übersprungen – {e}", it.found.path.display());
                None
            }
        })
        .collect();
    if work.is_empty() {
        println!("Nichts zu tun.");
        return Ok(all_ok);
    }
    let total = work.len();
    let next = AtomicUsize::new(0);
    let failures = AtomicUsize::new(0);
    let print = Mutex::new(());
    std::thread::scope(|scope| {
        for _ in 0..jobs.clamp(1, 8).min(total) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((found, media, plan)) = work.get(i) else {
                        break;
                    };
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let name = media.file_name();
                    let mut last = 0u8;
                    let result = run_job(
                        plan,
                        media,
                        Some(&found.root),
                        tools,
                        &opts,
                        cancel,
                        &mut |ev| match ev {
                            JobEvent::Progress(p) if p >= last.saturating_add(10) || p == 100 => {
                                last = p;
                                if jobs == 1 {
                                    eprint!("\r[{}/{total}] {name}: {p:>3}%", i + 1);
                                }
                            }
                            JobEvent::Log(l) => tracing::debug!("{name}: {l}"),
                            _ => {}
                        },
                    );
                    let _guard = print.lock();
                    if jobs == 1 {
                        eprint!("\r\x1b[2K");
                    }
                    match result {
                        Ok(r) => {
                            let out = r
                                .output
                                .as_deref()
                                .unwrap_or(&r.source)
                                .display()
                                .to_string();
                            let moved = match &r.original_moved_to {
                                Some(OriginalLocation::Trash) => " (Original im Papierkorb)".into(),
                                Some(OriginalLocation::Folder(p)) => {
                                    format!(" (Original: {})", p.display())
                                }
                                None => String::new(),
                            };
                            println!("✓ [{}/{total}] {out}{moved}", i + 1);
                            for w in r.warnings {
                                println!("  ! {w}");
                            }
                        }
                        Err(e) => {
                            failures.fetch_add(1, Ordering::Relaxed);
                            println!("✗ [{}/{total}] {}: {e}", i + 1, plan.path.display());
                        }
                    }
                }
            });
        }
    });
    if cancel.load(Ordering::Relaxed) {
        println!("Abgebrochen.");
        return Ok(false);
    }
    Ok(all_ok && failures.load(Ordering::Relaxed) == 0)
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(if std::env::var_os("TRACKWRIGHT_DEBUG").is_some() {
            tracing::Level::DEBUG
        } else {
            tracing::Level::WARN
        })
        .init();
    let tools = Tools::default();
    let needs_tools = !matches!(cli.command, Command::Presets);
    if needs_tools {
        check_tools(&tools)?;
    }
    match cli.command {
        Command::Probe { paths, json } => {
            let results = probe_all(&tools, &paths)?;
            let mut ok = true;
            if json {
                let v: Vec<_> = results
                    .iter()
                    .filter_map(|(_, r)| r.as_ref().ok())
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
            for (found, r) in &results {
                match r {
                    Ok(m) if !json => {
                        println!("▶ {}  ({} Spuren)", found.path.display(), m.tracks.len());
                        for t in &m.tracks {
                            let mut flags = Vec::new();
                            if t.flags.default {
                                flags.push("Standard");
                            }
                            if t.flags.forced {
                                flags.push("Forced");
                            }
                            if t.flags.hearing_impaired {
                                flags.push("HI");
                            }
                            if t.flags.commentary {
                                flags.push("Kommentar");
                            }
                            println!("  ID {:>2}  {}  [{}]", t.id, t.describe(), flags.join(", "));
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        ok = false;
                        eprintln!("✗ {}: {e}", found.path.display());
                    }
                }
            }
            Ok(ok)
        }
        Command::Plan {
            rules,
            output,
            json,
            paths,
        } => {
            let mode = output.mode()?;
            let items = plan_all(&tools, &rules, &paths)?;
            if json {
                let v: Vec<_> = items
                    .iter()
                    .filter_map(|i| i.outcome.as_ref().ok())
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                print_plan(&items, &mode);
            }
            Ok(items.iter().all(|i| i.outcome.is_ok()))
        }
        Command::Apply {
            rules,
            output,
            jobs,
            no_snapshot,
            paths,
        } => {
            let mode = output.mode()?;
            let items = plan_all(&tools, &rules, &paths)?;
            let cancel: &'static AtomicBool = Box::leak(Box::new(AtomicBool::new(false)));
            ctrlc::set_handler(move || {
                eprintln!("\nAbbruch angefordert – laufender Prozess wird beendet …");
                cancel.store(true, Ordering::Relaxed);
            })
            .context("Strg+C-Handler")?;
            cmd_apply(&tools, items, mode, jobs, !no_snapshot, cancel)
        }
        Command::Undo { snapshot, dry_run } => {
            let snap = Snapshot::load(&snapshot)?;
            let cmd = snap.undo_command();
            println!("$ {}", cmd.to_shell());
            if !dry_run {
                let cancel = AtomicBool::new(false);
                trackwright_core::exec::run(&cmd, &tools, &cancel, &mut |_| {})?;
                println!("✓ wiederhergestellt: {}", snap.path.display());
            }
            Ok(true)
        }
        Command::Presets => {
            println!("Mitgeliefert:");
            for (name, src) in BUILTIN {
                let p = Preset::parse(src, Path::new(name))?;
                println!("  {name:<14} {}", p.description.unwrap_or_default());
            }
            if let Some(dir) = presets_dir() {
                println!("Eigene ({}):", dir.display());
                for path in list_presets(&dir) {
                    match Preset::load(&path) {
                        Ok(p) => println!("  {}", p.display_name(Some(&path))),
                        Err(e) => println!("  ! {e}"),
                    }
                }
            }
            Ok(true)
        }
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::from(1),
        Err(e) => {
            eprintln!("Fehler: {e:#}");
            std::process::ExitCode::from(2)
        }
    }
}
