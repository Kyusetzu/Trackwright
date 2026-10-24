//! GUI-Zustand ohne Darstellung: Dateiliste, Regel-Editor, Overrides, Pläne. Gut testbar.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use trackwright_core::engine::{
    Overrides, PlanOptions, TrackOverride, language_suggestions, plan_file,
};
use trackwright_core::fsutil::FoundFile;
use trackwright_core::job::{JobOptions, OutputMode};
use trackwright_core::lang::Lang;
use trackwright_core::model::{MediaInfo, Signature, TrackType};
use trackwright_core::plan::{FilePlan, Strategy};
use trackwright_core::preset::Preset;
use trackwright_core::rules::{Expr, FlagRule, LangAssign, NameAssign, RuleSet, TypeRules};

/// Einlesestatus einer Datei.
#[derive(Debug, Clone)]
pub enum ProbeState {
    Pending,
    Ready(Box<MediaInfo>),
    Failed(String),
}

/// Ausführungsstatus einer Datei.
#[derive(Debug, Clone, PartialEq)]
pub enum JobState {
    Idle,
    Queued,
    Running(u8),
    Done(String),
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub found: FoundFile,
    pub probe: ProbeState,
    pub job: JobState,
    pub selected: bool,
}

impl FileEntry {
    pub fn media(&self) -> Option<&MediaInfo> {
        match &self.probe {
            ProbeState::Ready(m) => Some(m),
            _ => None,
        }
    }

    pub fn name(&self) -> String {
        self.found
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Eine Zeile „Sprache setzen“ / „Name setzen“.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssignRow {
    pub when: String,
    pub value: String,
}

/// Editor für die Regeln eines Spurtyps (Textfelder in Kurzsyntax).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TypeRuleEditor {
    pub keep: String,
    pub default: String,
    pub forced: String,
    pub set_language: Vec<AssignRow>,
    pub set_name: Vec<AssignRow>,
}

/// Prüft einen Ausdruck; leer ist erlaubt.
pub fn expr_error(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    text.parse::<Expr>().err().map(|e| e.to_string())
}

/// Prüft eine Flag-Regel (leer, „keine“ oder Ausdruck).
pub fn flag_error(text: &str) -> Option<String> {
    text.parse::<FlagRule>().err().map(|e| e.to_string())
}

impl TypeRuleEditor {
    pub fn from_rules(r: &TypeRules) -> Self {
        let flag = |f: &FlagRule| match f {
            FlagRule::Unchanged => String::new(),
            other => other.to_string(),
        };
        TypeRuleEditor {
            keep: r.keep.as_ref().map(|e| e.to_string()).unwrap_or_default(),
            default: flag(&r.default),
            forced: flag(&r.forced),
            set_language: r
                .set_language
                .iter()
                .map(|a| AssignRow {
                    when: a.when.to_string(),
                    value: a.lang.clone(),
                })
                .collect(),
            set_name: r
                .set_name
                .iter()
                .map(|a| AssignRow {
                    when: a.when.to_string(),
                    value: a.name.clone(),
                })
                .collect(),
        }
    }

    pub fn to_rules(&self, label: &str) -> Result<TypeRules, Vec<String>> {
        let mut errors = Vec::new();
        let mut out = TypeRules::default();
        if !self.keep.trim().is_empty() {
            match self.keep.parse() {
                Ok(e) => out.keep = Some(e),
                Err(e) => errors.push(format!("{label} behalten: {e}")),
            }
        }
        match self.default.parse() {
            Ok(f) => out.default = f,
            Err(e) => errors.push(format!("{label} Standard: {e}")),
        }
        match self.forced.parse() {
            Ok(f) => out.forced = f,
            Err(e) => errors.push(format!("{label} Forced: {e}")),
        }
        for row in &self.set_language {
            if row.when.trim().is_empty() && row.value.trim().is_empty() {
                continue;
            }
            if row.value.trim().is_empty() {
                errors.push(format!("{label} Sprache setzen: Sprachcode fehlt"));
                continue;
            }
            match row.when.parse() {
                Ok(when) => out.set_language.push(LangAssign {
                    when,
                    lang: row.value.trim().to_string(),
                }),
                Err(e) => errors.push(format!("{label} Sprache setzen: {e}")),
            }
        }
        for row in &self.set_name {
            if row.when.trim().is_empty() {
                continue;
            }
            match row.when.parse() {
                Ok(when) => out.set_name.push(NameAssign {
                    when,
                    name: row.value.clone(),
                }),
                Err(e) => errors.push(format!("{label} Name setzen: {e}")),
            }
        }
        if errors.is_empty() {
            Ok(out)
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RuleEditor {
    pub audio: TypeRuleEditor,
    pub subtitles: TypeRuleEditor,
}

impl RuleEditor {
    pub fn from_preset(p: &Preset) -> Self {
        RuleEditor {
            audio: TypeRuleEditor::from_rules(&p.audio),
            subtitles: TypeRuleEditor::from_rules(&p.subtitles),
        }
    }

    pub fn to_rules(&self) -> Result<RuleSet, Vec<String>> {
        let audio = self.audio.to_rules("Audio");
        let subtitles = self.subtitles.to_rules("Untertitel");
        match (audio, subtitles) {
            (Ok(audio), Ok(subtitles)) => Ok(RuleSet { audio, subtitles }),
            (a, s) => Err(a
                .err()
                .unwrap_or_default()
                .into_iter()
                .chain(s.err().unwrap_or_default())
                .collect()),
        }
    }

    pub fn for_type_mut(&mut self, kind: TrackType) -> &mut TypeRuleEditor {
        match kind {
            TrackType::Subtitles => &mut self.subtitles,
            _ => &mut self.audio,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeKind {
    Beside,
    Directory,
    Replace,
}

#[derive(Debug, Clone)]
pub struct OutputSettings {
    pub kind: ModeKind,
    pub suffix: String,
    pub out_dir: Option<PathBuf>,
    pub fallback_dir: bool,
    pub snapshot: bool,
    pub jobs: usize,
}

impl Default for OutputSettings {
    fn default() -> Self {
        OutputSettings {
            kind: ModeKind::Beside,
            suffix: ".trackwright".into(),
            out_dir: None,
            fallback_dir: false,
            snapshot: true,
            jobs: 1,
        }
    }
}

impl OutputSettings {
    pub fn mode(&self) -> Result<OutputMode, String> {
        Ok(match self.kind {
            ModeKind::Beside => {
                if self.suffix.trim().is_empty() || self.suffix.contains('/') {
                    return Err("Suffix darf nicht leer sein und keinen „/“ enthalten".into());
                }
                OutputMode::Beside {
                    suffix: self.suffix.clone(),
                }
            }
            ModeKind::Directory => OutputMode::Directory {
                dir: self
                    .out_dir
                    .clone()
                    .ok_or_else(|| "Bitte einen Ausgabeordner wählen".to_string())?,
            },
            ModeKind::Replace => OutputMode::Replace {
                fallback_dir: self.fallback_dir,
            },
        })
    }

    pub fn job_options(&self) -> Result<JobOptions, String> {
        Ok(JobOptions {
            mode: self.mode()?,
            snapshot_dir: if self.snapshot {
                trackwright_core::fsutil::state_dir().map(|d| d.join("snapshots"))
            } else {
                None
            },
        })
    }
}

/// Gruppe gleich aufgebauter Dateien (Indizes in `Model::files`).
#[derive(Debug, Clone)]
pub struct GroupInfo {
    pub signature: Signature,
    pub files: Vec<usize>,
}

/// Zusammengefasster Sprachvorschlag über alle Dateien.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggSuggestion {
    pub kind: TrackType,
    pub name: String,
    pub lang: Lang,
    pub count: usize,
}

/// Summen für Vorschau und Statusleiste.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub remux: usize,
    pub propedit: usize,
    pub nothing: usize,
    pub skipped: usize,
    pub write_bytes: u64,
}

/// Gesamter Zustand.
#[derive(Default)]
pub struct Model {
    pub files: Vec<FileEntry>,
    pub editor: RuleEditor,
    pub allow_no_audio: bool,
    pub overrides: HashMap<Signature, Overrides>,
    pub reference: Option<Signature>,
    pub output: OutputSettings,
    /// Parallel zu `files`: Plan, Grund fürs Überspringen, oder `None` (noch nicht eingelesen).
    pub plans: Vec<Option<Result<FilePlan, String>>>,
    pub groups: Vec<GroupInfo>,
    pub rule_errors: Vec<String>,
    dirty: bool,
}

impl Model {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Fügt gefundene Dateien hinzu und liefert die neuen Pfade (ohne Duplikate).
    pub fn add_found(&mut self, found: Vec<FoundFile>) -> Vec<PathBuf> {
        let mut new = Vec::new();
        for f in found {
            if self.files.iter().any(|e| e.found.path == f.path) {
                continue;
            }
            new.push(f.path.clone());
            self.files.push(FileEntry {
                found: f,
                probe: ProbeState::Pending,
                job: JobState::Idle,
                selected: false,
            });
        }
        self.files.sort_by(|a, b| a.found.path.cmp(&b.found.path));
        self.dirty = true;
        new
    }

    pub fn set_probe(&mut self, path: &Path, result: Result<MediaInfo, String>) {
        if let Some(e) = self.files.iter_mut().find(|e| e.found.path == path) {
            e.probe = match result {
                Ok(m) => ProbeState::Ready(Box::new(m)),
                Err(err) => ProbeState::Failed(err),
            };
            self.dirty = true;
        }
    }

    pub fn remove_where(&mut self, pred: impl Fn(&FileEntry) -> bool) {
        self.files.retain(|e| !pred(e));
        self.dirty = true;
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.files.iter().position(|e| e.found.path == path)
    }

    /// Berechnet Pläne und Gruppen neu, wenn sich etwas geändert hat.
    pub fn refresh(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        let mut groups: Vec<GroupInfo> = Vec::new();
        for (i, e) in self.files.iter().enumerate() {
            if let Some(m) = e.media() {
                let sig = m.signature();
                match groups.iter_mut().find(|g| g.signature == sig) {
                    Some(g) => g.files.push(i),
                    None => groups.push(GroupInfo {
                        signature: sig,
                        files: vec![i],
                    }),
                }
            }
        }
        groups.sort_by_key(|g| std::cmp::Reverse(g.files.len()));
        self.groups = groups;

        let rules = self.editor.to_rules();
        self.rule_errors = rules.as_ref().err().cloned().unwrap_or_default();
        let opts = PlanOptions {
            reference: self.reference.clone(),
            allow_no_audio: self.allow_no_audio,
        };
        self.plans = self
            .files
            .iter()
            .map(|e| {
                let m = e.media()?;
                Some(match &rules {
                    Ok(r) => {
                        let ov = self.overrides.get(&m.signature());
                        plan_file(m, r, ov, &opts).map_err(|e| e.to_string())
                    }
                    Err(_) => Err("Regeln fehlerhaft".to_string()),
                })
            })
            .collect();
    }

    pub fn summary(&self) -> Summary {
        let mut s = Summary::default();
        for p in self.plans.iter().flatten() {
            match p {
                Ok(plan) => {
                    match plan.strategy {
                        Strategy::Remux => s.remux += 1,
                        Strategy::Propedit => s.propedit += 1,
                        Strategy::Nothing => s.nothing += 1,
                    }
                    s.write_bytes += plan.write_bytes();
                }
                Err(_) => s.skipped += 1,
            }
        }
        s
    }

    /// Sprachvorschläge aller Dateien, zusammengefasst nach Typ, Name und Sprache.
    pub fn suggestions(&self) -> Vec<AggSuggestion> {
        let mut out: Vec<AggSuggestion> = Vec::new();
        for m in self.files.iter().filter_map(FileEntry::media) {
            for s in language_suggestions(m) {
                match out
                    .iter_mut()
                    .find(|a| a.kind == s.kind && a.name == s.name && a.lang == s.lang)
                {
                    Some(a) => a.count += 1,
                    None => out.push(AggSuggestion {
                        kind: s.kind,
                        name: s.name,
                        lang: s.lang,
                        count: 1,
                    }),
                }
            }
        }
        out.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
        out
    }

    /// Übernimmt einen Vorschlag als Regel „Sprache setzen“ (nur für Spuren ohne Sprache).
    pub fn apply_suggestion(&mut self, s: &AggSuggestion) {
        let when = suggestion_expr(&s.name);
        let rows = &mut self.editor.for_type_mut(s.kind).set_language;
        if !rows.iter().any(|r| r.when == when) {
            rows.push(AssignRow {
                when,
                value: s.lang.iso639_2().to_string(),
            });
        }
        self.dirty = true;
    }

    /// Setzt oder entfernt einen Override.
    pub fn set_override(
        &mut self,
        sig: &Signature,
        position: usize,
        f: impl FnOnce(&mut TrackOverride),
    ) {
        let ov = self.overrides.entry(sig.clone()).or_default();
        let entry = ov.entry(position).or_default();
        f(entry);
        if entry.is_empty() {
            ov.remove(&position);
        }
        if ov.is_empty() {
            self.overrides.remove(sig);
        }
        self.dirty = true;
    }

    /// Arbeitsliste für die Ausführung: (Index, Plan) aller Dateien mit etwas zu tun.
    pub fn work(&self) -> Vec<(usize, FilePlan)> {
        self.plans
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p {
                Some(Ok(plan)) if plan.strategy != Strategy::Nothing => Some((i, plan.clone())),
                _ => None,
            })
            .collect()
    }
}

/// Ausdruck für „Spur ohne Sprache mit genau diesem Namen“.
pub fn suggestion_expr(name: &str) -> String {
    let re = format!("(?i)^{}$", regex::escape(name));
    format!(
        "Sprache = undefiniert UND Name ~ /{}/",
        re.replace('/', r"\/")
    )
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trackwright_core::preset::BUILTIN;
    use trackwright_core::probe::parse_identification;

    fn media(name: &str, path: &str) -> MediaInfo {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/json")
            .join(name);
        let json = std::fs::read_to_string(p).expect("fixture");
        let mut m = parse_identification(&json, Path::new(path)).expect("parse");
        m.size = 2_000_000_000;
        m
    }

    fn model_with(files: &[(&str, &str)]) -> Model {
        let mut m = Model::default();
        let found = files
            .iter()
            .map(|(_, p)| FoundFile {
                path: PathBuf::from(p),
                root: PathBuf::from("/media"),
            })
            .collect();
        let new = m.add_found(found);
        assert_eq!(new.len(), files.len());
        for (fixture, p) in files {
            m.set_probe(Path::new(p), Ok(media(fixture, p)));
        }
        m
    }

    #[test]
    fn editor_roundtrip_with_preset() {
        let preset = Preset::parse(BUILTIN[0].1, Path::new("anime")).expect("preset");
        let ed = RuleEditor::from_preset(&preset);
        assert_eq!(ed.to_rules().expect("rules"), preset.rules());
        assert_eq!(ed.subtitles.default, "keine");
        assert_eq!(ed.audio.forced, "");
    }

    #[test]
    fn editor_reports_errors() {
        let mut ed = RuleEditor::default();
        ed.audio.keep = "Nr.".into();
        ed.subtitles.set_language.push(AssignRow {
            when: "Nr. 1".into(),
            value: String::new(),
        });
        let errs = ed.to_rules().expect_err("fehler");
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert!(expr_error("Nr. 1").is_none());
        assert!(expr_error("").is_none());
        assert!(flag_error("keine").is_none());
        assert!(flag_error("Nr. x").is_some());
    }

    #[test]
    fn plans_groups_and_summary() {
        let mut m = model_with(&[
            ("full.json", "/media/a.mkv"),
            ("full.json", "/media/b.mkv"),
            ("und.json", "/media/c.mkv"),
        ]);
        m.editor = RuleEditor::from_preset(
            &Preset::parse(BUILTIN[0].1, Path::new("anime")).expect("preset"),
        );
        m.mark_dirty();
        m.refresh();
        assert_eq!(m.groups.len(), 2);
        assert_eq!(m.groups[0].files, vec![0, 1]);
        let s = m.summary();
        assert_eq!(
            (s.remux, s.skipped),
            (2, 1),
            "und.json: keine Audiospur bliebe"
        );
        assert_eq!(s.write_bytes, 4_000_000_000);
        assert_eq!(m.work().len(), 2);

        // Vorschlag übernehmen → c.mkv wird planbar.
        let sugg = m.suggestions();
        assert!(
            sugg.iter()
                .any(|s| s.name == "Deutsch" && s.kind == TrackType::Audio)
        );
        for s in sugg {
            m.apply_suggestion(&s);
        }
        m.refresh();
        let plan_c = m.plans[2].as_ref().expect("probed").as_ref().expect("plan");
        assert!(plan_c.kept().any(|t| t.kind == TrackType::Audio));
        assert!(m.rule_errors.is_empty(), "{:?}", m.rule_errors);
    }

    #[test]
    fn overrides_and_reference() {
        let mut m = model_with(&[("full.json", "/media/a.mkv"), ("und.json", "/media/c.mkv")]);
        m.refresh();
        let sig = m.files[0].media().expect("media").signature();
        m.set_override(&sig, 3, |o| o.keep = Some(false));
        m.refresh();
        let plan = m.plans[0].as_ref().expect("p").as_ref().expect("plan");
        assert!(!plan.tracks[3].keep);
        assert_eq!(plan.strategy, Strategy::Remux);
        // Override zurücknehmen entfernt den Eintrag.
        m.set_override(&sig, 3, |o| o.keep = None);
        assert!(m.overrides.is_empty());

        m.reference = Some(sig);
        m.mark_dirty();
        m.refresh();
        assert!(m.plans[1].as_ref().expect("p").is_err());
    }

    #[test]
    fn suggestion_expression_is_valid_and_exact() {
        for name in ["Deutsch", "a/b (GER) [x]", "日本語"] {
            let src = suggestion_expr(name);
            assert!(expr_error(&src).is_none(), "{src}");
        }
    }

    #[test]
    fn output_settings() {
        let mut o = OutputSettings::default();
        assert!(o.mode().is_ok());
        o.kind = ModeKind::Directory;
        assert!(o.mode().is_err());
        o.out_dir = Some("/out".into());
        assert!(matches!(o.mode(), Ok(OutputMode::Directory { .. })));
        o.kind = ModeKind::Beside;
        o.suffix = String::new();
        assert!(o.mode().is_err());
        assert_eq!(format_size(1_500_000_000), "1.5 GB");
        assert_eq!(format_size(12), "12 B");
    }
}
