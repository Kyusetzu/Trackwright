//! Hauptfenster nach dem Vorbild mkv-muxing-batch-gui: Reiter „Videos“ und „Mux-Einstellungen“.

use crate::settings::Settings;
use crate::state::{
    DefaultMode, JobState, Model, ProbeState, TypeControls, format_percent_change, format_size,
    format_size_change,
};
use crate::strings as s;
use crate::worker::{JobMsg, Msg, Pick, WorkItem, Worker};
use egui::containers::PopupCloseBehavior;
use egui::{Color32, RichText};
use egui_extras::{Column, TableBuilder};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use trackwright_core::env::check_tools;
use trackwright_core::job::{OriginalLocation, planned_commands};
use trackwright_core::model::TrackType;
use trackwright_core::plan::Strategy;
use trackwright_core::selection::{Options, TrackRef};
use trackwright_core::tools::Tools;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Videos,
    Mux,
}

struct Running {
    cancel: Arc<AtomicBool>,
    total: usize,
    done: usize,
    sizes: SizeTotals,
}

/// Summe der Dateigrößen vorher/nachher über alle geschriebenen Dateien eines Laufs.
#[derive(Clone, Copy, Default)]
struct SizeTotals {
    before: u64,
    after: u64,
    files: usize,
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
    log: Vec<String>,
    running: Option<Running>,
    notice: Option<String>,
    confirm_replace: bool,
    last_run_sizes: Option<SizeTotals>,
    settings: Settings,
    /// Offenes Einstellungsfenster mit dem bearbeiteten Entwurf.
    settings_draft: Option<Settings>,
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
        App {
            model: Model::default(),
            worker: Worker::new(tx, ctx, tools),
            rx,
            tab: Tab::Videos,
            tools_status,
            log: Vec::new(),
            running: None,
            notice: None,
            confirm_replace: false,
            last_run_sizes: None,
            settings: Settings::default(),
            settings_draft: None,
        }
    }

    /// Übernimmt gespeicherte Einstellungen; ein fester Ausgabeordner wird Zielordner.
    pub fn apply_settings(&mut self, settings: Settings) {
        self.model.destination = settings.output_dir.clone();
        self.model.mark_dirty();
        self.settings = settings;
    }

    pub fn add_paths(&mut self, paths: Vec<PathBuf>) {
        if !paths.is_empty() {
            self.worker.find(paths);
        }
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
                self.worker.probe(new, false);
            }
            Msg::Probed { path, result } => {
                if let Err(e) = &result {
                    self.log(format!("✗ {}: {e}", path.display()));
                }
                self.model.set_probe(&path, result);
            }
            Msg::Picked(kind, paths) => match kind {
                Pick::AddFiles => self.add_paths(paths),
                Pick::SourceFolder => {
                    if let Some(p) = paths.into_iter().next() {
                        self.model.source_path = p.display().to_string();
                        self.add_paths(vec![p]);
                    }
                }
                Pick::Destination => {
                    if let Some(p) = paths.into_iter().next() {
                        self.model.destination = p.display().to_string();
                        self.model.mark_dirty();
                    }
                }
                Pick::SettingsOutputDir => {
                    if let (Some(p), Some(d)) = (paths.into_iter().next(), &mut self.settings_draft)
                    {
                        d.output_dir = p.display().to_string();
                    }
                }
            },
            Msg::Job { path, msg } => self.handle_job(&path, msg),
            Msg::RunFinished { cancelled, aborted } => {
                for e in &mut self.model.files {
                    if e.job == JobState::Queued {
                        e.job = JobState::Cancelled;
                    }
                }
                self.log(if cancelled {
                    "■ Abgebrochen".to_string()
                } else if aborted {
                    "■ Wegen eines Fehlers abgebrochen („Bei Fehlern abbrechen“)".to_string()
                } else {
                    "■ Fertig".to_string()
                });
                if let Some(sizes) = self.running.take().map(|r| r.sizes)
                    && sizes.files > 0
                {
                    self.log(format!(
                        "  {}: {} ({} Dateien)",
                        s::SIZE_TOTAL,
                        format_size_change(sizes.before, sizes.after),
                        sizes.files
                    ));
                    self.last_run_sizes = Some(sizes);
                }
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
                self.model.files[i].size_change = None;
                self.log(format!("▶ {name}"));
            }
            JobMsg::Progress(p) => self.model.files[i].job = JobState::Running(p),
            JobMsg::Log(l) => self.log(format!("  {l}")),
            JobMsg::Skipped(reason) => {
                if let Some(r) = &mut self.running {
                    r.done += 1;
                }
                self.log(format!("– {name}: übersprungen ({reason})"));
                self.model.files[i].job = JobState::Skipped(reason);
            }
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
                        // Status in der Liste ohne Pfade; das Log bekommt die Details.
                        let (mut status, mut log) = match &report.output {
                            Some(out) if out == path => {
                                ("fertig".to_string(), "fertig".to_string())
                            }
                            Some(out) => {
                                ("fertig".to_string(), format!("fertig → {}", out.display()))
                            }
                            None => ("unverändert".to_string(), "unverändert".to_string()),
                        };
                        match &report.original_moved_to {
                            Some(OriginalLocation::Trash) => {
                                status.push_str(" (Original im Papierkorb)");
                                log.push_str(" (Original im Papierkorb)");
                            }
                            Some(OriginalLocation::Folder(p)) => {
                                status.push_str(" (Original verschoben)");
                                log.push_str(&format!(" (Original: {})", p.display()));
                            }
                            None => {}
                        }
                        if let Some(after) = report.size_after {
                            log.push_str(&format!(
                                " · {}",
                                format_size_change(report.size_before, after)
                            ));
                            self.model.files[i].size_change = Some((report.size_before, after));
                            if let Some(r) = &mut self.running {
                                r.sizes.before += report.size_before;
                                r.sizes.after += after;
                                r.sizes.files += 1;
                            }
                        }
                        for w in &report.warnings {
                            self.log(format!("  Warnung: {w}"));
                        }
                        self.log(format!("✓ {name}: {log}"));
                        self.model.files[i].job = JobState::Done(status);
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

    fn can_start(&self) -> bool {
        self.running.is_none() && self.tools_status.is_ok() && !self.model.work().is_empty()
    }

    fn request_start(&mut self) {
        if self.model.replaces_originals() {
            self.confirm_replace = true;
        } else {
            self.start();
        }
    }

    fn start(&mut self) {
        let items: Vec<WorkItem> = self
            .model
            .work()
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
            sizes: SizeTotals::default(),
        });
        self.last_run_sizes = None;
        self.worker.run(
            items,
            self.model.job_options(),
            self.model.abort_on_errors,
            cancel,
        );
    }

    // -----------------------------------------------------------------------------------------
    // Oberfläche

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll();
        let ctx = ui.ctx().clone();
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_paths(dropped);
            self.tab = Tab::Videos;
        }
        self.model.refresh();

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(s::APP_TITLE);
                ui.separator();
                ui.selectable_value(&mut self.tab, Tab::Videos, s::TAB_VIDEOS);
                ui.selectable_value(&mut self.tab, Tab::Mux, s::TAB_MUX);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(s::SETTINGS_BUTTON).clicked() && self.settings_draft.is_none() {
                        self.settings_draft = Some(self.settings.clone());
                    }
                });
            });
            ui.add_space(2.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Videos => self.videos_tab(ui),
            Tab::Mux => self.mux_tab(ui),
        });

        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("trackwright-drop-overlay"),
            ));
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(170));
            painter.rect_stroke(
                screen.shrink(12.0),
                8.0,
                egui::Stroke::new(3.0, GREEN),
                egui::StrokeKind::Inside,
            );
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                s::DROP_HINT,
                egui::FontId::proportional(26.0),
                Color32::WHITE,
            );
        }
        self.dialogs(&ctx);
        if self.running.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.settings_draft else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut pick = false;
        egui::Window::new(s::SETTINGS_TITLE)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(s::SETTINGS_OUTPUT_DIR);
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.output_dir)
                            .hint_text(s::SETTINGS_OUTPUT_HINT)
                            .desired_width(360.0),
                    );
                    pick = ui.button(s::CHOOSE).clicked();
                    if ui.button(s::SETTINGS_CLEAR).clicked() {
                        draft.output_dir.clear();
                    }
                });
                ui.weak(s::SETTINGS_OUTPUT_INFO);
                ui.add_space(6.0);
                save = ui.button(s::SETTINGS_SAVE).clicked();
            });
        if pick {
            let start = Some(PathBuf::from(draft.output_dir.trim())).filter(|p| p.is_dir());
            self.worker.pick(Pick::SettingsOutputDir, start);
        }
        if save {
            let mut new = draft.clone();
            new.output_dir = new.output_dir.trim().to_string();
            let result = Settings::default_path()
                .ok_or_else(|| "Kein Konfigurationsordner (HOME fehlt)".to_string())
                .and_then(|p| new.save(&p));
            match result {
                Ok(()) => {
                    self.log(s::SETTINGS_SAVED);
                    if self.running.is_none() {
                        self.model.destination = new.output_dir.clone();
                        self.model.mark_dirty();
                    }
                    self.settings = new;
                    self.settings_draft = None;
                }
                Err(e) => self.notice = Some(format!("Speichern fehlgeschlagen: {e}")),
            }
        } else if !open {
            self.settings_draft = None;
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        self.settings_window(ctx);
        if let Some(text) = self.notice.clone() {
            let mut open = true;
            egui::Window::new("Hinweis")
                .collapsible(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(text);
                    if ui.button("OK").clicked() {
                        self.notice = None;
                    }
                });
            if !open {
                self.notice = None;
            }
        }
        if self.confirm_replace {
            let mut decision = None;
            egui::Window::new(s::CONFIRM_TITLE)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_max_width(460.0);
                    ui.label(s::CONFIRM_TEXT);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(s::CONFIRM_YES).clicked() {
                            decision = Some(true);
                        }
                        if ui.button(s::CONFIRM_NO).clicked() {
                            decision = Some(false);
                        }
                    });
                });
            if let Some(yes) = decision {
                self.confirm_replace = false;
                if yes {
                    self.start();
                }
            }
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match &self.tools_status {
                Ok(v) => ui.weak(v),
                Err(e) => ui.colored_label(RED, format!("{}: {e}", s::TOOLS_MISSING)),
            };
            ui.separator();
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
            let sum = self.model.summary();
            ui.label(format!(
                "{} Remux ({}), {} nur Header, {} unverändert, {} übersprungen",
                sum.remux,
                format_size(sum.write_bytes),
                sum.propedit,
                sum.nothing,
                sum.skipped
            ));
            let sizes = self
                .running
                .as_ref()
                .map(|r| r.sizes)
                .filter(|t| t.files > 0)
                .or(self.last_run_sizes);
            if let Some(t) = sizes {
                ui.separator();
                let saved = t.before.saturating_sub(t.after);
                let text = format!(
                    "{}: {}",
                    s::SIZE_TOTAL,
                    format_size_change(t.before, t.after)
                );
                ui.label(text).on_hover_text(format!(
                    "{} Dateien, {} {}",
                    t.files,
                    format_size(saved),
                    s::SIZE_SAVED
                ));
            }
        });
    }

    // ----- Videos -----------------------------------------------------------------------------

    fn videos_tab(&mut self, ui: &mut egui::Ui) {
        let running = self.running.is_some();
        ui.horizontal(|ui| {
            ui.label(s::SOURCE_FOLDER);
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.model.source_path)
                    .hint_text(s::SOURCE_HINT)
                    .desired_width(ui.available_width() - 260.0),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let p = PathBuf::from(self.model.source_path.trim());
                if !p.as_os_str().is_empty() {
                    self.add_paths(vec![p]);
                }
            }
            if ui.button(s::BROWSE).clicked() {
                self.worker.pick(Pick::SourceFolder, None);
            }
            if ui.button(s::ADD_FILES).clicked() {
                self.worker.pick(Pick::AddFiles, None);
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new(s::REMOVE_CHECKED))
                .clicked()
            {
                self.model.remove_where(|e| e.checked);
            }
            if ui
                .add_enabled(!running, egui::Button::new(s::CLEAR_LIST))
                .clicked()
            {
                self.model.remove_where(|_| true);
            }
        });
        ui.separator();

        if self.model.files.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.label(RichText::new(s::DROP_HINT).size(20.0).weak());
            });
            return;
        }

        let list_height = (ui.available_height() * 0.55).max(120.0);
        let mut clicked = None;
        ui.push_id("videos", |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .sense(egui::Sense::click())
                .max_scroll_height(list_height)
                .min_scrolled_height(list_height)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::auto())
                .column(Column::auto().at_least(32.0))
                .column(Column::remainder().at_least(240.0).clip(true))
                .column(Column::auto().at_least(80.0))
                .column(Column::auto().at_least(110.0))
                .column(Column::auto().at_least(140.0))
                .header(22.0, |mut h| {
                    h.col(|_| {});
                    for t in ["#", s::COL_NAME, s::COL_SIZE, s::COL_TRACKS, s::COL_STATUS] {
                        h.col(|ui| {
                            ui.strong(t);
                        });
                    }
                })
                .body(|body| {
                    body.rows(22.0, self.model.files.len(), |mut row| {
                        let i = row.index();
                        row.set_selected(self.model.selected == Some(i));
                        let entry = &mut self.model.files[i];
                        row.col(|ui| {
                            ui.checkbox(&mut entry.checked, "");
                        });
                        row.col(|ui| {
                            ui.label(format!("{}", i + 1));
                        });
                        row.col(|ui| {
                            ui.label(entry.name())
                                .on_hover_text(entry.found.path.display().to_string());
                        });
                        row.col(|ui| size_cell(ui, entry));
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
                            probe_status(ui, entry);
                        });
                        if row.response().clicked() {
                            clicked = Some(i);
                        }
                    });
                });
        });
        if clicked.is_some() {
            self.model.selected = clicked;
        }

        ui.separator();
        let Some(m) = self
            .model
            .selected
            .and_then(|i| self.model.files.get(i))
            .and_then(|e| e.media())
            .cloned()
        else {
            ui.weak(s::SELECT_FILE_HINT);
            return;
        };
        ui.strong(format!("{} {}", s::TRACKS_OF, m.file_name()));
        ui.push_id("tracks", |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::auto().at_least(30.0))
                .column(Column::auto().at_least(80.0))
                .column(Column::auto().at_least(36.0))
                .column(Column::auto().at_least(120.0))
                .column(Column::auto().at_least(140.0))
                .column(Column::auto().at_least(50.0))
                .column(Column::remainder().at_least(120.0).clip(true))
                .column(Column::auto().at_least(64.0))
                .column(Column::auto().at_least(56.0))
                .header(20.0, |mut h| {
                    for t in [
                        s::COL_ID,
                        s::COL_TYPE,
                        s::COL_NR,
                        s::COL_LANG,
                        s::COL_CODEC,
                        s::COL_CHANNELS,
                        s::COL_TRACK_NAME,
                        s::COL_DEFAULT,
                        s::COL_FORCED,
                    ] {
                        h.col(|ui| {
                            ui.strong(t);
                        });
                    }
                })
                .body(|body| {
                    body.rows(20.0, m.tracks.len(), |mut row| {
                        let t = &m.tracks[row.index()];
                        let tick = |b: bool| if b { "✔" } else { "" };
                        let cells = [
                            t.id.to_string(),
                            t.kind.label().to_string(),
                            t.type_index.to_string(),
                            t.lang().display_name(),
                            t.codec.clone(),
                            t.channels.map(|c| c.to_string()).unwrap_or_default(),
                            t.name.clone().unwrap_or_default(),
                            tick(t.flags.default).to_string(),
                            tick(t.flags.forced).to_string(),
                        ];
                        for c in cells {
                            row.col(|ui| {
                                ui.label(c);
                            });
                        }
                    });
                });
        });
    }

    // ----- Mux-Einstellungen ------------------------------------------------------------------

    fn mux_tab(&mut self, ui: &mut egui::Ui) {
        let running = self.running.is_some();
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.strong(s::SETTINGS);
            ui.add_enabled_ui(!running, |ui| {
                ui.horizontal(|ui| {
                    ui.label(s::DESTINATION);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.model.destination)
                            .hint_text(s::DESTINATION_HINT)
                            .desired_width(ui.available_width() - 90.0),
                    );
                    if r.changed() {
                        self.model.mark_dirty();
                    }
                    if ui.button(s::CHOOSE).clicked() {
                        self.worker.pick(Pick::Destination, None);
                    }
                });
                ui.add_space(4.0);
                egui::Grid::new("mux-settings")
                    .num_columns(4)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        for (kind, keep_label, default_label) in [
                            (TrackType::Audio, s::KEEP_AUDIOS, s::DEFAULT_AUDIO),
                            (TrackType::Subtitles, s::KEEP_SUBTITLES, s::DEFAULT_SUBTITLE),
                        ] {
                            let opts = self.model.options(kind).clone();
                            let c = self.model.controls_mut(kind);
                            let mut changed =
                                ui.checkbox(&mut c.keep_enabled, keep_label).changed();
                            changed |= ui
                                .add_enabled_ui(c.keep_enabled, |ui| {
                                    keep_dropdown(ui, kind, c, &opts)
                                })
                                .inner;
                            changed |= tristate(ui, &mut c.default_mode, default_label);
                            changed |= ui
                                .add_enabled_ui(c.default_mode != DefaultMode::Off, |ui| {
                                    default_dropdown(ui, kind, c, &opts)
                                })
                                .inner;
                            ui.end_row();
                            if changed {
                                self.model.mark_dirty();
                            }
                        }
                    });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.model.abort_on_errors, s::ABORT_ON_ERRORS);
                    ui.add_enabled_ui(self.model.replaces_originals(), |ui| {
                        if ui
                            .checkbox(&mut self.model.fallback_dir, s::FALLBACK_DIR)
                            .changed()
                        {
                            self.model.mark_dirty();
                        }
                    });
                });
            });
        });
        ui.add_space(6.0);
        self.queue(ui);
    }

    fn queue(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong(s::QUEUE);
            let work = self.model.work().len();
            if ui
                .add_enabled(
                    self.can_start(),
                    egui::Button::new(format!("{} ({work})", s::START)),
                )
                .clicked()
            {
                self.request_start();
            }
            if let Some(r) = &self.running
                && ui.button(s::CANCEL).clicked()
            {
                r.cancel.store(true, Ordering::Relaxed);
                self.log("■ Abbruch angefordert …");
            }
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
        });

        let mode = self.model.output_mode();
        let log_height = 140.0;
        let table_height = (ui.available_height() - log_height - 40.0).max(120.0);
        let mut copy = None;
        ui.push_id("queue", |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .max_scroll_height(table_height)
                .min_scrolled_height(table_height)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::auto().at_least(32.0))
                .column(Column::remainder().at_least(160.0).clip(true))
                .column(Column::auto().at_least(80.0))
                .column(Column::initial(260.0).at_least(120.0).clip(true))
                .column(Column::initial(260.0).at_least(120.0).clip(true))
                .header(22.0, |mut h| {
                    for t in ["#", s::COL_NAME, s::COL_SIZE, s::COL_ACTION, s::COL_STATUS] {
                        h.col(|ui| {
                            ui.strong(t);
                        });
                    }
                })
                .body(|body| {
                    body.rows(22.0, self.model.files.len(), |mut row| {
                        let i = row.index();
                        let entry = &self.model.files[i];
                        let plan = self.model.plans.get(i).cloned().flatten();
                        row.col(|ui| {
                            ui.label(format!("{}", i + 1));
                        });
                        row.col(|ui| {
                            ui.label(entry.name());
                        });
                        row.col(|ui| {
                            if let Some(m) = entry.media() {
                                ui.label(format_size(m.size));
                            }
                        });
                        row.col(|ui| match &plan {
                            Some(Ok(p)) => {
                                let (text, color) = match p.strategy {
                                    Strategy::Remux => {
                                        (format!("Remux (−{} Spuren)", p.removed_count()), YELLOW)
                                    }
                                    Strategy::Propedit => {
                                        (format!("nur Header (~{})", p.changed_count()), GREEN)
                                    }
                                    Strategy::Nothing => ("nichts zu tun".into(), Color32::GRAY),
                                };
                                let commands: Vec<String> =
                                    planned_commands(p, Some(&entry.found.root), &mode)
                                        .map(|c| c.iter().map(|c| c.to_shell()).collect())
                                        .unwrap_or_else(|e| vec![e.to_string()]);
                                let mut details = p.render_diff();
                                for w in &p.warnings {
                                    details.push_str(&format!("⚠ {w}\n"));
                                }
                                for c in &commands {
                                    details.push_str(&format!("\n$ {c}"));
                                }
                                let text = if p.warnings.is_empty() {
                                    text
                                } else {
                                    format!("⚠ {text}")
                                };
                                let r = ui
                                    .colored_label(color, text)
                                    .on_hover_text(RichText::new(details).monospace());
                                r.context_menu(|ui| {
                                    if ui
                                        .add_enabled(
                                            !commands.is_empty(),
                                            egui::Button::new(s::COPY_COMMAND),
                                        )
                                        .clicked()
                                    {
                                        copy = Some(commands.join("\n"));
                                        ui.close();
                                    }
                                });
                            }
                            Some(Err(e)) => {
                                ui.colored_label(RED, format!("übersprungen: {e}"))
                                    .on_hover_text(e);
                            }
                            None => {
                                probe_status(ui, entry);
                            }
                        });
                        row.col(|ui| job_status(ui, &entry.job));
                    });
                });
        });
        if let Some(text) = copy {
            ui.ctx().copy_text(text);
        }

        egui::CollapsingHeader::new(s::LOG)
            .default_open(true)
            .show(ui, |ui| {
                if ui.small_button(s::COPY).clicked() {
                    ui.ctx().copy_text(self.log.join("\n"));
                }
                egui::ScrollArea::vertical()
                    .id_salt("log")
                    .stick_to_bottom(true)
                    .max_height(log_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for l in &self.log {
                            ui.add(egui::Label::new(RichText::new(l).monospace()).wrap());
                        }
                    });
            });
    }
}

fn files_label(n: usize) -> String {
    if n == 1 {
        "1 Datei".to_string()
    } else {
        format!("{n} Dateien")
    }
}

/// Checkbox mit drei Zuständen wie im Vorbild: aus → halb (Standard) → ganz (Standard + Forced).
fn tristate(ui: &mut egui::Ui, mode: &mut DefaultMode, label: &str) -> bool {
    let mut checked = *mode != DefaultMode::Off;
    let r = ui
        .add(egui::Checkbox::new(&mut checked, label).indeterminate(*mode == DefaultMode::Default))
        .on_hover_text(s::DEFAULT_TOOLTIP);
    if r.clicked() {
        *mode = mode.next();
        true
    } else {
        false
    }
}

fn sections(opts: &Options) -> [(&'static str, &[TrackRef]); 3] {
    [
        (s::SECTION_NUMBER, opts.numbers.as_slice()),
        (s::SECTION_LANGUAGE, opts.languages.as_slice()),
        (s::SECTION_NAME, opts.names.as_slice()),
    ]
}

fn kind_label(kind: TrackType, r: &TrackRef) -> String {
    match r {
        TrackRef::Number(n) => format!("{} {n}", kind.label()),
        other => other.label(),
    }
}

/// Größe; nach einem Lauf die neue Größe mit Änderung in Prozent.
fn size_cell(ui: &mut egui::Ui, entry: &crate::state::FileEntry) {
    match entry.size_change {
        Some((before, after)) => {
            ui.label(format_size(after));
            if let Some(pct) = format_percent_change(before, after) {
                let color = if after < before { GREEN } else { YELLOW };
                ui.colored_label(color, pct);
            }
            ui.response().on_hover_text(format!(
                "{}: {}\n{}: {}",
                s::SIZE_BEFORE,
                format_size(before),
                s::SIZE_AFTER,
                format_size(after)
            ));
        }
        None => {
            if let Some(m) = entry.media() {
                ui.label(format_size(m.size));
            }
        }
    }
}

/// Dropdown mit Checkboxen: „Nur diese … behalten“.
fn keep_dropdown(ui: &mut egui::Ui, kind: TrackType, c: &mut TypeControls, opts: &Options) -> bool {
    let mut changed = false;
    let text = if !c.keep_enabled {
        RichText::new("–")
    } else if c.keep.is_empty() {
        RichText::new(s::DISCARD_ALL).italics()
    } else {
        RichText::new(
            c.keep
                .iter()
                .map(|r| kind_label(kind, r))
                .collect::<Vec<_>>()
                .join(", "),
        )
    };
    egui::ComboBox::from_id_salt(("keep", kind))
        .selected_text(text)
        .width(320.0)
        .height(420.0)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            if opts.is_empty() {
                ui.weak(s::NO_TRACKS);
                return;
            }
            let total = opts.all().count();
            let selected = opts.all().filter(|r| c.keep.contains(r)).count();
            let mut all = selected == total;
            if ui
                .add(
                    egui::Checkbox::new(&mut all, s::ALL_TRACKS)
                        .indeterminate(selected > 0 && selected < total),
                )
                .clicked()
            {
                if selected == total {
                    c.keep.clear();
                } else {
                    c.keep.extend(opts.all().cloned());
                }
                changed = true;
            }
            for (title, refs) in sections(opts) {
                if refs.is_empty() {
                    continue;
                }
                ui.separator();
                ui.weak(title);
                for r in refs {
                    let mut on = c.keep.contains(r);
                    if ui.checkbox(&mut on, kind_label(kind, r)).changed() {
                        if on {
                            c.keep.insert(r.clone());
                        } else {
                            c.keep.remove(r);
                        }
                        changed = true;
                    }
                }
            }
        });
    changed
}

/// Dropdown mit Einfachauswahl: „Diese … als Standard“.
fn default_dropdown(
    ui: &mut egui::Ui,
    kind: TrackType,
    c: &mut TypeControls,
    opts: &Options,
) -> bool {
    let mut changed = false;
    let text = if c.default_mode == DefaultMode::Off {
        "–".to_string()
    } else {
        c.default_ref
            .as_ref()
            .map(|r| kind_label(kind, r))
            .unwrap_or_else(|| s::DEFAULT_NONE.to_string())
    };
    egui::ComboBox::from_id_salt(("default", kind))
        .selected_text(text)
        .width(260.0)
        .height(420.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(c.default_ref.is_none(), s::DEFAULT_NONE)
                .clicked()
            {
                c.default_ref = None;
                changed = true;
            }
            for (title, refs) in sections(opts) {
                if refs.is_empty() {
                    continue;
                }
                ui.separator();
                ui.weak(title);
                for r in refs {
                    if ui
                        .selectable_label(c.default_ref.as_ref() == Some(r), kind_label(kind, r))
                        .clicked()
                    {
                        c.default_ref = Some(r.clone());
                        changed = true;
                    }
                }
            }
        });
    changed
}

fn probe_status(ui: &mut egui::Ui, e: &crate::state::FileEntry) {
    match &e.probe {
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
    }
}

fn job_status(ui: &mut egui::Ui, job: &JobState) {
    match job {
        JobState::Idle => {}
        JobState::Queued => {
            ui.weak("in Warteschlange");
        }
        JobState::Running(p) => {
            ui.add(
                egui::ProgressBar::new(f32::from(*p) / 100.0)
                    .desired_width(180.0)
                    .show_percentage(),
            );
        }
        JobState::Done(t) => {
            ui.colored_label(GREEN, format!("✓ {t}")).on_hover_text(t);
        }
        JobState::Failed(err) => {
            ui.colored_label(RED, "✗ fehlgeschlagen").on_hover_text(err);
        }
        JobState::Skipped(reason) => {
            ui.weak("übersprungen").on_hover_text(reason);
        }
        JobState::Cancelled => {
            ui.weak("abgebrochen");
        }
    }
}
