//! Hauptfenster: Tabs Dateien / Regeln / Gruppen / Vorschau / Ausführen.

use crate::state::{
    AssignRow, JobState, ModeKind, Model, ProbeState, TypeRuleEditor, expr_error, flag_error,
    format_size,
};
use crate::strings as s;
use crate::worker::{JobMsg, Msg, Pick, WorkItem, Worker};
use egui::{Color32, RichText};
use egui_extras::{Column, TableBuilder};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use trackwright_core::env::check_tools;
use trackwright_core::fsutil::is_network_fs;
use trackwright_core::job::{OriginalLocation, planned_commands};
use trackwright_core::model::TrackType;
use trackwright_core::plan::{Strategy, TrackStatus};
use trackwright_core::preset::{BUILTIN, Preset, list_presets, presets_dir};
use trackwright_core::tools::Tools;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Files,
    Rules,
    Groups,
    Preview,
    Run,
}

#[derive(Debug, Clone)]
enum PresetSource {
    Builtin(&'static str),
    File(PathBuf),
}

struct Running {
    cancel: Arc<AtomicBool>,
    total: usize,
    done: usize,
}

const RED: Color32 = Color32::from_rgb(0xE0, 0x5A, 0x5A);
const YELLOW: Color32 = Color32::from_rgb(0xD8, 0xA6, 0x2B);
const GREEN: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50);

pub struct App {
    pub model: Model,
    worker: Worker,
    rx: Receiver<Msg>,
    pub tab: Tab,
    tools_status: Result<String, String>,
    presets: Vec<(String, PresetSource)>,
    preset_selected: Option<usize>,
    log: Vec<String>,
    running: Option<Running>,
    preview_only_changes: bool,
    filter_undefined: bool,
    rules_tab: TrackType,
    notice: Option<String>,
    /// Anzahl Dateien auf Netzwerkfreigaben (Cache für den Modus „ersetzen“).
    network_files: Option<(usize, usize)>,
}

impl App {
    pub fn new(ctx: egui::Context, tools: Tools) -> Self {
        let (tx, rx) = channel();
        let tools_status = check_tools(&tools)
            .map(|v| {
                format!(
                    "mkvmerge v{}.{} · mkvpropedit v{}.{}",
                    v.mkvmerge.0, v.mkvmerge.1, v.mkvpropedit.0, v.mkvpropedit.1
                )
            })
            .map_err(|e| e.to_string());
        let mut app = App {
            model: Model::default(),
            worker: Worker::new(tx, ctx, tools),
            rx,
            tab: Tab::Files,
            tools_status,
            presets: Vec::new(),
            preset_selected: None,
            log: Vec::new(),
            running: None,
            preview_only_changes: false,
            filter_undefined: false,
            rules_tab: TrackType::Audio,
            notice: None,
            network_files: None,
        };
        app.reload_presets();
        app
    }

    pub fn add_paths(&mut self, paths: Vec<PathBuf>) {
        if !paths.is_empty() {
            self.worker.find(paths);
        }
    }

    fn reload_presets(&mut self) {
        let mut v: Vec<(String, PresetSource)> = BUILTIN
            .iter()
            .map(|(name, src)| {
                let label = Preset::parse(src, Path::new(name))
                    .map(|p| p.display_name(None))
                    .unwrap_or_else(|_| (*name).to_string());
                (
                    format!("{label} (mitgeliefert)"),
                    PresetSource::Builtin(src),
                )
            })
            .collect();
        if let Some(dir) = presets_dir() {
            for path in list_presets(&dir) {
                let label = Preset::load(&path)
                    .map(|p| p.display_name(Some(&path)))
                    .unwrap_or_else(|_| path.display().to_string());
                v.push((label, PresetSource::File(path)));
            }
        }
        self.presets = v;
    }

    fn load_preset(&mut self, preset: &Preset) {
        self.model.editor = crate::state::RuleEditor::from_preset(preset);
        self.model.mark_dirty();
    }

    fn log(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        if self.log.len() > 2000 {
            self.log.drain(..500);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Nachrichten aus den Worker-Threads

    pub fn poll(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.handle(msg);
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Found { found, errors } => {
                for e in errors {
                    self.log(format!("Hinweis: {e}"));
                }
                if found.is_empty() {
                    self.notice = Some("Keine .mkv-Dateien gefunden.".into());
                }
                let new = self.model.add_found(found);
                self.network_files = None;
                self.worker.probe(new, false);
            }
            Msg::Probed { path, result } => {
                if let Err(e) = &result {
                    self.log(format!("✗ {}: {e}", path.display()));
                }
                self.model.set_probe(&path, result);
            }
            Msg::Picked(kind, paths) => match kind {
                Pick::AddFolders | Pick::AddFiles => self.add_paths(paths),
                Pick::OutDir => {
                    self.model.output.out_dir = paths.into_iter().next();
                    self.model.mark_dirty();
                }
                Pick::PresetLoad => {
                    if let Some(p) = paths.first() {
                        match Preset::load(p) {
                            Ok(preset) => {
                                self.load_preset(&preset);
                                self.log(format!("Preset geladen: {}", p.display()));
                            }
                            Err(e) => self.notice = Some(e.to_string()),
                        }
                    }
                }
                Pick::PresetSave => {
                    if let Some(p) = paths.first() {
                        self.save_preset(p);
                    }
                }
            },
            Msg::Job { path, msg } => self.handle_job(&path, msg),
            Msg::RunFinished { cancelled } => {
                for e in &mut self.model.files {
                    if e.job == JobState::Queued {
                        e.job = JobState::Cancelled;
                    }
                }
                self.log(if cancelled {
                    "■ Abgebrochen".to_string()
                } else {
                    "■ Fertig".to_string()
                });
                self.running = None;
            }
        }
    }

    fn handle_job(&mut self, path: &Path, msg: JobMsg) {
        let Some(i) = self.model.index_of(path) else {
            return;
        };
        let name = self.model.files[i].name();
        match msg {
            JobMsg::Started => {
                self.model.files[i].job = JobState::Running(0);
                self.log(format!("▶ {name}"));
            }
            JobMsg::Progress(p) => self.model.files[i].job = JobState::Running(p),
            JobMsg::Log(l) => self.log(format!("  {l}")),
            JobMsg::Done(result) => {
                if let Some(r) = &mut self.running {
                    r.done += 1;
                }
                let cancelled = self
                    .running
                    .as_ref()
                    .is_some_and(|r| r.cancel.load(Ordering::Relaxed));
                match result {
                    Ok(report) => {
                        let mut text = match &report.output {
                            Some(out) if out == path => "fertig (in place)".to_string(),
                            Some(out) => format!(
                                "fertig → {}",
                                out.file_name().unwrap_or_default().to_string_lossy()
                            ),
                            None => "unverändert".to_string(),
                        };
                        match &report.original_moved_to {
                            Some(OriginalLocation::Trash) => {
                                text.push_str(", Original im Papierkorb")
                            }
                            Some(OriginalLocation::Folder(p)) => {
                                text.push_str(&format!(", Original: {}", p.display()))
                            }
                            None => {}
                        }
                        for w in &report.warnings {
                            self.log(format!("  Warnung: {w}"));
                        }
                        self.log(format!("✓ {name}: {text}"));
                        self.model.files[i].job = JobState::Done(text);
                        if report.output.as_deref() == Some(path) {
                            // Datei hat sich geändert → neu einlesen.
                            self.worker.probe(vec![path.to_path_buf()], true);
                        }
                    }
                    Err(e) if cancelled && e == "abgebrochen" => {
                        self.log(format!("■ {name}: abgebrochen"));
                        self.model.files[i].job = JobState::Cancelled;
                    }
                    Err(e) => {
                        self.log(format!("✗ {name}: {e}"));
                        self.model.files[i].job = JobState::Failed(e);
                    }
                }
            }
        }
    }

    fn save_preset(&mut self, path: &Path) {
        match self.model.editor.to_rules() {
            Ok(rules) => {
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned());
                let preset = Preset::from_rules(name, &rules);
                match preset.save(path) {
                    Ok(()) => {
                        self.log(format!("Preset gespeichert: {}", path.display()));
                        self.reload_presets();
                    }
                    Err(e) => self.notice = Some(e.to_string()),
                }
            }
            Err(errs) => self.notice = Some(errs.join("\n")),
        }
    }

    fn start(&mut self) {
        let opts = match self.model.output.job_options() {
            Ok(o) => o,
            Err(e) => {
                self.notice = Some(e);
                return;
            }
        };
        let work = self.model.work();
        let items: Vec<WorkItem> = work
            .into_iter()
            .filter_map(|(i, plan)| {
                let e = &self.model.files[i];
                Some(WorkItem {
                    root: e.found.root.clone(),
                    media: e.media()?.clone(),
                    plan,
                })
            })
            .collect();
        if items.is_empty() {
            self.notice = Some(s::NOTHING_TO_DO.into());
            return;
        }
        for item in &items {
            if let Some(i) = self.model.index_of(&item.plan.path) {
                self.model.files[i].job = JobState::Queued;
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.log(format!("■ Start: {} Dateien", items.len()));
        self.running = Some(Running {
            cancel: cancel.clone(),
            total: items.len(),
            done: 0,
        });
        self.worker.run(items, opts, self.model.output.jobs, cancel);
    }

    // -----------------------------------------------------------------------------------------
    // Oberfläche

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_paths(dropped);
            self.tab = Tab::Files;
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll();
        let ctx = ui.ctx().clone();
        self.handle_drops(&ctx);
        self.model.refresh();

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Files => self.files_tab(ui),
            Tab::Rules => self.rules_tab(ui),
            Tab::Groups => self.groups_tab(ui),
            Tab::Preview => self.preview_tab(ui),
            Tab::Run => self.run_tab(ui),
        });

        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            egui::Area::new(egui::Id::unique("trackwright-drop-overlay"))
                .fixed_pos(egui::pos2(24.0, 80.0))
                .show(&ctx, |ui| {
                    ui.label(RichText::new(s::DROP_HINT).size(22.0).strong());
                });
        }

        if let Some(text) = self.notice.clone() {
            let mut open = true;
            egui::Window::new("Hinweis")
                .collapsible(false)
                .open(&mut open)
                .show(&ctx, |ui| {
                    ui.label(text);
                    if ui.button("OK").clicked() {
                        self.notice = None;
                    }
                });
            if !open {
                self.notice = None;
            }
        }
        if self.running.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.heading(s::APP_TITLE);
            ui.separator();
            for (tab, label) in [
                (Tab::Files, s::TAB_FILES),
                (Tab::Rules, s::TAB_RULES),
                (Tab::Groups, s::TAB_GROUPS),
                (Tab::Preview, s::TAB_PREVIEW),
                (Tab::Run, s::TAB_RUN),
            ] {
                ui.selectable_value(&mut self.tab, tab, label);
            }
            ui.separator();
            ui.label(s::PRESET);
            let current = self
                .preset_selected
                .and_then(|i| self.presets.get(i))
                .map(|(n, _)| n.clone())
                .unwrap_or_else(|| s::PRESET_NONE.to_string());
            let mut chosen = None;
            egui::ComboBox::from_id_salt("preset")
                .selected_text(current)
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (i, (name, _)) in self.presets.iter().enumerate() {
                        if ui
                            .selectable_label(self.preset_selected == Some(i), name)
                            .clicked()
                        {
                            chosen = Some(i);
                        }
                    }
                });
            if let Some(i) = chosen {
                self.preset_selected = Some(i);
                let loaded = match &self.presets[i].1 {
                    PresetSource::Builtin(src) => Preset::parse(src, Path::new("builtin")),
                    PresetSource::File(p) => Preset::load(p),
                };
                match loaded {
                    Ok(p) => self.load_preset(&p),
                    Err(e) => self.notice = Some(e.to_string()),
                }
            }
            if ui.button(s::PRESET_LOAD).clicked() {
                self.worker.pick(Pick::PresetLoad, presets_dir());
            }
            if ui.button(s::PRESET_SAVE).clicked() {
                if let Some(d) = presets_dir() {
                    let _ = std::fs::create_dir_all(&d);
                }
                self.worker.pick(Pick::PresetSave, presets_dir());
            }
        });
        ui.add_space(2.0);
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.tools_status {
                Ok(v) => ui.weak(v),
                Err(e) => ui.colored_label(RED, format!("{}: {e}", s::TOOLS_MISSING)),
            };
            ui.separator();
            let sum = self.model.summary();
            let pending = self
                .model
                .files
                .iter()
                .filter(|e| matches!(e.probe, ProbeState::Pending))
                .count();
            ui.label(files_label(self.model.files.len()));
            if pending > 0 {
                ui.spinner();
                ui.label(format!("{pending} werden eingelesen"));
            }
            ui.separator();
            ui.label(format!(
                "{} Remux ({}), {} nur Header, {} unverändert, {} übersprungen",
                sum.remux,
                format_size(sum.write_bytes),
                sum.propedit,
                sum.nothing,
                sum.skipped
            ));
            if !self.model.rule_errors.is_empty() {
                ui.separator();
                ui.colored_label(RED, "Regeln fehlerhaft");
            }
        });
    }

    // ----- Dateien ----------------------------------------------------------------------------

    fn files_tab(&mut self, ui: &mut egui::Ui) {
        let running = self.running.is_some();
        ui.horizontal_wrapped(|ui| {
            if ui.button(s::ADD_FOLDER).clicked() {
                self.worker.pick(Pick::AddFolders, None);
            }
            if ui.button(s::ADD_FILES).clicked() {
                self.worker.pick(Pick::AddFiles, None);
            }
            if ui.button(s::RELOAD).clicked() {
                let paths: Vec<PathBuf> = self
                    .model
                    .files
                    .iter()
                    .map(|e| e.found.path.clone())
                    .collect();
                for e in &mut self.model.files {
                    e.probe = ProbeState::Pending;
                }
                self.worker.probe(paths, true);
            }
            if ui
                .add_enabled(!running, egui::Button::new(s::REMOVE_SELECTED))
                .clicked()
            {
                self.model.remove_where(|e| e.selected);
            }
            if ui
                .add_enabled(!running, egui::Button::new(s::CLEAR_LIST))
                .clicked()
            {
                self.model.remove_where(|_| true);
            }
            ui.checkbox(&mut self.filter_undefined, s::ONLY_UNDEFINED);
        });
        ui.separator();

        if self.model.files.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.label(RichText::new(s::DROP_HINT).size(20.0).weak());
            });
            return;
        }

        let group_of = |i: usize| -> Option<usize> {
            self.model.groups.iter().position(|g| g.files.contains(&i))
        };
        let rows: Vec<usize> = (0..self.model.files.len())
            .filter(|&i| {
                !self.filter_undefined
                    || self.model.files[i]
                        .media()
                        .is_some_and(|m| m.has_undefined_language())
            })
            .collect();
        let groups: Vec<Option<usize>> = rows.iter().map(|&i| group_of(i)).collect();

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto())
            .column(Column::remainder().at_least(240.0).clip(true))
            .column(Column::auto().at_least(70.0))
            .column(Column::auto().at_least(110.0))
            .column(Column::auto().at_least(60.0))
            .column(Column::auto().at_least(160.0))
            .header(22.0, |mut h| {
                h.col(|_| {});
                for t in [
                    s::COL_NAME,
                    s::COL_SIZE,
                    s::COL_TRACKS,
                    s::COL_GROUP,
                    s::COL_STATUS,
                ] {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, rows.len(), |mut row| {
                    let i = rows[row.index()];
                    let group = groups[row.index()];
                    let entry = &mut self.model.files[i];
                    row.col(|ui| {
                        ui.checkbox(&mut entry.selected, "");
                    });
                    row.col(|ui| {
                        let r = ui.label(entry.name());
                        let path = entry.found.path.display().to_string();
                        let media = entry.media().cloned();
                        r.on_hover_ui(|ui| {
                            ui.label(path);
                            if let Some(m) = media {
                                for t in &m.tracks {
                                    ui.monospace(t.describe());
                                }
                            }
                        });
                    });
                    row.col(|ui| {
                        if let Some(m) = entry.media() {
                            ui.label(format_size(m.size));
                        }
                    });
                    row.col(|ui| {
                        if let Some(m) = entry.media() {
                            ui.label(format!(
                                "V{} A{} U{}",
                                m.count_of(TrackType::Video),
                                m.count_of(TrackType::Audio),
                                m.count_of(TrackType::Subtitles)
                            ));
                            if m.has_undefined_language() {
                                ui.colored_label(YELLOW, "⚠")
                                    .on_hover_text(s::UNDEFINED_BADGE);
                            }
                        }
                    });
                    row.col(|ui| {
                        if let Some(g) = group {
                            ui.label(format!("#{}", g + 1));
                        }
                    });
                    row.col(|ui| {
                        job_or_probe_status(ui, entry);
                    });
                });
            });
    }

    // ----- Regeln -----------------------------------------------------------------------------

    fn rules_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.rules_tab, TrackType::Audio, s::AUDIO);
            ui.selectable_value(&mut self.rules_tab, TrackType::Subtitles, s::SUBTITLES);
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let kind = self.rules_tab;
            let changed = type_rule_editor(ui, self.model.editor.for_type_mut(kind), kind);
            if changed {
                self.model.mark_dirty();
            }

            ui.add_space(8.0);
            if ui
                .checkbox(&mut self.model.allow_no_audio, s::ALLOW_NO_AUDIO)
                .changed()
            {
                self.model.mark_dirty();
            }

            let suggestions = self.model.suggestions();
            if !suggestions.is_empty() {
                ui.add_space(8.0);
                ui.strong(s::SUGGESTIONS);
                let mut apply = None;
                egui::Grid::new("suggestions").striped(true).show(ui, |ui| {
                    for (i, sg) in suggestions.iter().enumerate() {
                        ui.label(sg.kind.label());
                        ui.label(format!("„{}“", sg.name));
                        ui.label(format!("→ {}", sg.lang.display_name()));
                        ui.weak(files_label(sg.count));
                        if ui.button(s::SUGGESTION_APPLY).clicked() {
                            apply = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = apply {
                    self.model.apply_suggestion(&suggestions[i]);
                }
            }

            ui.add_space(8.0);
            egui::CollapsingHeader::new(s::SYNTAX_HELP_TITLE).show(ui, |ui| {
                ui.monospace(s::SYNTAX_HELP);
            });
            for e in &self.model.rule_errors {
                ui.colored_label(RED, e);
            }
        });
    }

    // ----- Gruppen ----------------------------------------------------------------------------

    fn groups_tab(&mut self, ui: &mut egui::Ui) {
        ui.weak(s::GROUP_HINT);
        ui.separator();
        let groups = self.model.groups.clone();
        let mut actions: Vec<GroupAction> = Vec::new();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (gi, g) in groups.iter().enumerate() {
                let Some(&first) = g.files.first() else {
                    continue;
                };
                let is_ref = self.model.reference.as_ref() == Some(&g.signature);
                let title = format!(
                    "Gruppe #{} – {}{}",
                    gi + 1,
                    files_label(g.files.len()),
                    if is_ref { " ★" } else { "" }
                );
                egui::CollapsingHeader::new(title)
                    .id_salt(("group", gi))
                    .default_open(gi < 3)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut r = is_ref;
                            if ui.checkbox(&mut r, s::GROUP_REFERENCE).changed() {
                                actions
                                    .push(GroupAction::Reference(r.then(|| g.signature.clone())));
                            }
                            if is_ref {
                                ui.colored_label(YELLOW, s::GROUP_REFERENCE_ACTIVE);
                            }
                            if self.model.overrides.contains_key(&g.signature)
                                && ui.button(s::GROUP_RESET).clicked()
                            {
                                actions.push(GroupAction::Reset(g.signature.clone()));
                            }
                        });
                        let overrides = self.model.overrides.get(&g.signature).cloned();
                        match self.model.plans.get(first).cloned().flatten() {
                            Some(Ok(plan)) => {
                                egui::Grid::new(("grid", gi)).striped(true).show(ui, |ui| {
                                    ui.strong("Spur");
                                    ui.strong(s::GROUP_KEEP);
                                    ui.strong(s::RULE_DEFAULT);
                                    ui.strong(s::RULE_FORCED);
                                    ui.label("");
                                    ui.end_row();
                                    for t in &plan.tracks {
                                        let editable = matches!(
                                            t.kind,
                                            TrackType::Audio | TrackType::Subtitles
                                        );
                                        let ov = overrides
                                            .as_ref()
                                            .and_then(|o| o.get(&t.position))
                                            .copied()
                                            .unwrap_or_default();
                                        let mut label = t.label();
                                        if t.keep && t.after.lang != t.before.lang {
                                            label.push_str(&format!(" → {}", t.after.lang));
                                        }
                                        ui.label(label);
                                        if !editable {
                                            ui.label("");
                                            ui.label("");
                                            ui.label("");
                                            ui.label("");
                                            ui.end_row();
                                            continue;
                                        }
                                        let mut keep = t.keep;
                                        if ui.checkbox(&mut keep, mark(ov.keep.is_some())).changed()
                                        {
                                            actions.push(GroupAction::Keep(
                                                g.signature.clone(),
                                                t.position,
                                                keep,
                                            ));
                                        }
                                        let mut def = t.after.default;
                                        if ui
                                            .add_enabled(
                                                t.keep,
                                                egui::Checkbox::new(
                                                    &mut def,
                                                    mark(ov.default.is_some()),
                                                ),
                                            )
                                            .changed()
                                        {
                                            actions.push(GroupAction::Default(
                                                g.signature.clone(),
                                                t.position,
                                                def,
                                            ));
                                        }
                                        let mut forced = t.after.forced;
                                        if ui
                                            .add_enabled(
                                                t.keep,
                                                egui::Checkbox::new(
                                                    &mut forced,
                                                    mark(ov.forced.is_some()),
                                                ),
                                            )
                                            .changed()
                                        {
                                            actions.push(GroupAction::Forced(
                                                g.signature.clone(),
                                                t.position,
                                                forced,
                                            ));
                                        }
                                        if !ov.is_empty() && ui.small_button("↺").clicked() {
                                            actions.push(GroupAction::ResetTrack(
                                                g.signature.clone(),
                                                t.position,
                                            ));
                                        } else if ov.is_empty() {
                                            ui.label("");
                                        }
                                        ui.end_row();
                                    }
                                });
                            }
                            Some(Err(e)) => {
                                ui.colored_label(RED, e);
                            }
                            None => {
                                ui.spinner();
                            }
                        }
                        egui::CollapsingHeader::new(files_label(g.files.len()))
                            .id_salt(("files", gi))
                            .show(ui, |ui| {
                                for &i in &g.files {
                                    ui.label(self.model.files[i].name());
                                }
                            });
                    });
            }
        });
        for a in actions {
            match a {
                GroupAction::Reference(r) => {
                    self.model.reference = r;
                    self.model.mark_dirty();
                }
                GroupAction::Reset(sig) => {
                    self.model.overrides.remove(&sig);
                    self.model.mark_dirty();
                }
                GroupAction::ResetTrack(sig, pos) => self
                    .model
                    .set_override(&sig, pos, |o| *o = Default::default()),
                GroupAction::Keep(sig, pos, v) => {
                    self.model.set_override(&sig, pos, |o| o.keep = Some(v))
                }
                GroupAction::Default(sig, pos, v) => {
                    self.model.set_override(&sig, pos, |o| o.default = Some(v))
                }
                GroupAction::Forced(sig, pos, v) => {
                    self.model.set_override(&sig, pos, |o| o.forced = Some(v))
                }
            }
        }
    }

    // ----- Vorschau ---------------------------------------------------------------------------

    fn preview_tab(&mut self, ui: &mut egui::Ui) {
        let sum = self.model.summary();
        let mode = self.model.output.mode();
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!(
                "{} Remux ({} zu schreiben) · {} nur Header · {} unverändert · {} übersprungen",
                sum.remux,
                format_size(sum.write_bytes),
                sum.propedit,
                sum.nothing,
                sum.skipped
            ));
            ui.checkbox(&mut self.preview_only_changes, s::PREVIEW_ONLY_CHANGES);
            if let Ok(mode) = &mode
                && ui.button(s::PREVIEW_COPY_ALL).clicked()
            {
                let mut all = String::new();
                for (i, p) in self.model.plans.iter().enumerate() {
                    if let Some(Ok(plan)) = p
                        && let Ok(cmds) =
                            planned_commands(plan, Some(&self.model.files[i].found.root), mode)
                    {
                        for c in cmds {
                            all.push_str(&c.to_shell());
                            all.push('\n');
                        }
                    }
                }
                ui.ctx().copy_text(all);
            }
        });
        if let Err(e) = &mode {
            ui.colored_label(YELLOW, format!("Ausgabe: {e}"));
        }
        for e in &self.model.rule_errors {
            ui.colored_label(RED, e);
        }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, entry) in self.model.files.iter().enumerate() {
                let Some(Some(p)) = self.model.plans.get(i) else {
                    continue;
                };
                match p {
                    Ok(plan) => {
                        if self.preview_only_changes && plan.strategy == Strategy::Nothing {
                            continue;
                        }
                        let (icon, color) = match plan.strategy {
                            Strategy::Remux => ("✂", YELLOW),
                            Strategy::Propedit => ("✎", GREEN),
                            Strategy::Nothing => ("·", Color32::GRAY),
                        };
                        let title = RichText::new(format!(
                            "{icon} {} — {} (−{} ~{})",
                            entry.name(),
                            plan.strategy.label(),
                            plan.removed_count(),
                            plan.changed_count()
                        ))
                        .color(color);
                        egui::CollapsingHeader::new(title)
                            .id_salt(("preview", i))
                            .show(ui, |ui| {
                                for t in &plan.tracks {
                                    let status = t.status();
                                    let changes: Vec<String> =
                                        t.changes().iter().map(|c| c.to_string()).collect();
                                    let text = match status {
                                        TrackStatus::Removed => {
                                            RichText::new(format!("− {}", t.label()))
                                                .color(RED)
                                                .strikethrough()
                                        }
                                        TrackStatus::Changed => RichText::new(format!(
                                            "~ {}  [{}]",
                                            t.label(),
                                            changes.join(", ")
                                        ))
                                        .color(YELLOW),
                                        TrackStatus::Unchanged => {
                                            RichText::new(format!("  {}", t.label())).weak()
                                        }
                                    };
                                    ui.label(text);
                                }
                                for w in &plan.warnings {
                                    ui.colored_label(YELLOW, format!("⚠ {w}"));
                                }
                                if let Ok(mode) = &mode {
                                    match planned_commands(plan, Some(&entry.found.root), mode) {
                                        Ok(cmds) => {
                                            for c in cmds {
                                                let shell = c.to_shell();
                                                ui.horizontal(|ui| {
                                                    if ui.small_button(s::PREVIEW_COPY).clicked() {
                                                        ui.ctx().copy_text(shell.clone());
                                                    }
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(&shell).monospace(),
                                                        )
                                                        .wrap(),
                                                    );
                                                });
                                            }
                                        }
                                        Err(e) => {
                                            ui.colored_label(RED, e.to_string());
                                        }
                                    }
                                }
                            });
                    }
                    Err(e) => {
                        ui.label(
                            RichText::new(format!("✗ {} — übersprungen: {e}", entry.name()))
                                .color(RED),
                        );
                    }
                }
            }
        });
    }

    // ----- Ausführen --------------------------------------------------------------------------

    fn network_count(&mut self) -> usize {
        let key = self.model.files.len();
        if let Some((k, n)) = self.network_files
            && k == key
        {
            return n;
        }
        let n = self
            .model
            .files
            .iter()
            .filter(|e| is_network_fs(&e.found.path))
            .count();
        self.network_files = Some((key, n));
        n
    }

    fn run_tab(&mut self, ui: &mut egui::Ui) {
        let running = self.running.is_some();
        ui.add_enabled_ui(!running, |ui| {
            ui.strong(s::RUN_MODE);
            let o = &mut self.model.output;
            ui.radio_value(&mut o.kind, ModeKind::Beside, s::MODE_BESIDE);
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label(s::SUFFIX);
                ui.add_enabled(
                    o.kind == ModeKind::Beside,
                    egui::TextEdit::singleline(&mut o.suffix).desired_width(140.0),
                );
            });
            ui.radio_value(&mut o.kind, ModeKind::Directory, s::MODE_DIR);
            let mut pick_out = false;
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label(s::OUT_DIR);
                ui.label(
                    o.out_dir
                        .as_ref()
                        .map(|d| d.display().to_string())
                        .unwrap_or_else(|| "–".into()),
                );
                pick_out = ui.button(s::CHOOSE).clicked();
            });
            ui.radio_value(&mut o.kind, ModeKind::Replace, s::MODE_REPLACE);
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.add_enabled(
                    o.kind == ModeKind::Replace,
                    egui::Checkbox::new(&mut o.fallback_dir, s::FALLBACK_DIR),
                );
            });
            ui.checkbox(&mut o.snapshot, s::SNAPSHOT);
            ui.horizontal(|ui| {
                ui.label(s::PARALLEL);
                ui.add(egui::Slider::new(&mut o.jobs, 1..=4));
                ui.weak(s::PARALLEL_HINT);
            });
            let replace_no_fallback = o.kind == ModeKind::Replace && !o.fallback_dir;
            if pick_out {
                self.worker.pick(Pick::OutDir, None);
            }
            if replace_no_fallback {
                let n = self.network_count();
                if n > 0 {
                    ui.colored_label(
                        YELLOW,
                        format!(
                            "⚠ {n} Datei(en) liegen auf einer Netzwerkfreigabe ohne Papierkorb – \
                             „Original ersetzen“ ist dort gesperrt (Remux wird übersprungen)."
                        ),
                    );
                }
            }
        });
        ui.separator();

        let work = self.model.work().len();
        let can_start = !running
            && self.tools_status.is_ok()
            && work > 0
            && self.model.rule_errors.is_empty()
            && self.model.output.mode().is_ok();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_start,
                    egui::Button::new(format!("{} ({work})", s::START)),
                )
                .clicked()
            {
                self.start();
            }
            if let Some(r) = &self.running
                && ui.button(s::CANCEL).clicked()
            {
                r.cancel.store(true, Ordering::Relaxed);
                self.log("■ Abbruch angefordert …");
            }
        });
        if let Some(r) = &self.running {
            let partial: f32 = self
                .model
                .files
                .iter()
                .map(|e| match e.job {
                    JobState::Running(p) => f32::from(p) / 100.0,
                    _ => 0.0,
                })
                .sum();
            let progress = (r.done as f32 + partial) / r.total.max(1) as f32;
            ui.add(
                egui::ProgressBar::new(progress)
                    .text(format!("{}: {}/{}", s::TOTAL, r.done, r.total))
                    .animate(true),
            );
        }

        ui.separator();
        let avail = ui.available_height();
        egui::ScrollArea::vertical()
            .id_salt("queue")
            .max_height(avail * 0.45)
            .show(ui, |ui| {
                for e in self.model.files.iter().filter(|e| e.job != JobState::Idle) {
                    ui.horizontal(|ui| {
                        ui.label(e.name());
                        job_or_probe_status(ui, e);
                    });
                }
            });
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong(s::LOG);
            if ui.small_button(s::PREVIEW_COPY).clicked() {
                ui.ctx().copy_text(self.log.join("\n"));
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("log")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for l in &self.log {
                    ui.add(egui::Label::new(RichText::new(l).monospace()).wrap());
                }
            });
    }
}

enum GroupAction {
    Reference(Option<trackwright_core::model::Signature>),
    Reset(trackwright_core::model::Signature),
    ResetTrack(trackwright_core::model::Signature, usize),
    Keep(trackwright_core::model::Signature, usize, bool),
    Default(trackwright_core::model::Signature, usize, bool),
    Forced(trackwright_core::model::Signature, usize, bool),
}

fn files_label(n: usize) -> String {
    if n == 1 {
        "1 Datei".to_string()
    } else {
        format!("{n} {}", s::GROUP_FILES)
    }
}

fn mark(overridden: bool) -> &'static str {
    if overridden { "✎" } else { "" }
}

fn job_or_probe_status(ui: &mut egui::Ui, e: &crate::state::FileEntry) {
    match &e.job {
        JobState::Idle => match &e.probe {
            ProbeState::Pending => {
                ui.spinner();
                ui.weak(s::STATUS_PROBING);
            }
            ProbeState::Ready(_) => {
                ui.weak(s::STATUS_READY);
            }
            ProbeState::Failed(err) => {
                ui.colored_label(RED, "✗ Fehler").on_hover_text(err);
            }
        },
        JobState::Queued => {
            ui.weak("in Warteschlange");
        }
        JobState::Running(p) => {
            ui.add(
                egui::ProgressBar::new(f32::from(*p) / 100.0)
                    .desired_width(140.0)
                    .show_percentage(),
            );
        }
        JobState::Done(t) => {
            ui.colored_label(GREEN, format!("✓ {t}"));
        }
        JobState::Failed(err) => {
            ui.colored_label(RED, "✗ fehlgeschlagen").on_hover_text(err);
        }
        JobState::Cancelled => {
            ui.weak("abgebrochen");
        }
    }
}

/// Textfeld mit Live-Prüfung. Liefert `true`, wenn sich der Text geändert hat.
fn rule_field(
    ui: &mut egui::Ui,
    label: &str,
    hint: &str,
    text: &mut String,
    check: fn(&str) -> Option<String>,
) -> bool {
    ui.label(RichText::new(label).strong());
    let changed = ui
        .add(
            egui::TextEdit::singleline(text)
                .hint_text(hint.to_string())
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        )
        .changed();
    if let Some(err) = check(text) {
        ui.colored_label(RED, err);
    }
    ui.add_space(4.0);
    changed
}

fn assign_rows(ui: &mut egui::Ui, id: &str, rows: &mut Vec<AssignRow>, value_hint: &str) -> bool {
    let mut changed = false;
    let mut remove = None;
    egui::Grid::new(id).num_columns(4).show(ui, |ui| {
        for (i, row) in rows.iter_mut().enumerate() {
            ui.label(s::RULE_WHEN);
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut row.when)
                        .hint_text("Nr. 1")
                        .desired_width(420.0)
                        .font(egui::TextStyle::Monospace),
                )
                .changed();
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut row.value)
                        .hint_text(value_hint.to_string())
                        .desired_width(160.0),
                )
                .changed();
            if ui.small_button("🗑").clicked() {
                remove = Some(i);
            }
            ui.end_row();
            if let Some(err) = expr_error(&row.when) {
                ui.label("");
                ui.colored_label(RED, err);
                ui.end_row();
            }
        }
    });
    if let Some(i) = remove {
        rows.remove(i);
        changed = true;
    }
    if ui.small_button(s::RULE_ADD).clicked() {
        rows.push(AssignRow::default());
        changed = true;
    }
    changed
}

fn type_rule_editor(ui: &mut egui::Ui, ed: &mut TypeRuleEditor, kind: TrackType) -> bool {
    let mut changed = false;
    changed |= rule_field(
        ui,
        s::RULE_KEEP,
        s::RULE_KEEP_HINT,
        &mut ed.keep,
        expr_error,
    );
    changed |= rule_field(
        ui,
        s::RULE_DEFAULT,
        s::RULE_FLAG_HINT,
        &mut ed.default,
        flag_error,
    );
    changed |= rule_field(
        ui,
        s::RULE_FORCED,
        s::RULE_FLAG_HINT,
        &mut ed.forced,
        flag_error,
    );
    ui.add_space(6.0);
    ui.strong(s::RULE_SET_LANG);
    changed |= assign_rows(ui, &format!("lang-{kind:?}"), &mut ed.set_language, "ger");
    ui.add_space(6.0);
    ui.strong(s::RULE_SET_NAME);
    changed |= assign_rows(
        ui,
        &format!("name-{kind:?}"),
        &mut ed.set_name,
        "Name (leer = entfernen)",
    );
    changed
}
