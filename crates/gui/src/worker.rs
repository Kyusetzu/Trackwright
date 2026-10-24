//! Hintergrundarbeit: Dateien suchen, einlesen, Dialoge, Ausführung. Ergebnisse kommen als
//! [`Msg`] über einen Kanal zur UI.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use trackwright_core::fsutil::{FoundFile, collect_mkv};
use trackwright_core::job::{JobEvent, JobOptions, JobReport, run_job};
use trackwright_core::model::MediaInfo;
use trackwright_core::plan::FilePlan;
use trackwright_core::probe::{ProbeCache, probe_many};
use trackwright_core::tools::Tools;

/// Welcher Dialog ein Ergebnis geliefert hat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    AddFolders,
    AddFiles,
    OutDir,
    PresetLoad,
    PresetSave,
}

#[derive(Debug)]
pub enum JobMsg {
    Started,
    Progress(u8),
    Log(String),
    Done(Result<JobReport, String>),
}

#[derive(Debug)]
pub enum Msg {
    Found {
        found: Vec<FoundFile>,
        errors: Vec<String>,
    },
    Probed {
        path: PathBuf,
        result: Result<MediaInfo, String>,
    },
    Picked(Pick, Vec<PathBuf>),
    Job {
        path: PathBuf,
        msg: JobMsg,
    },
    RunFinished {
        cancelled: bool,
    },
}

/// Ein Eintrag der Warteschlange.
pub struct WorkItem {
    pub root: PathBuf,
    pub media: MediaInfo,
    pub plan: FilePlan,
}

#[derive(Clone)]
pub struct Worker {
    tx: Sender<Msg>,
    ctx: egui::Context,
    pub tools: Tools,
    cache: Arc<ProbeCache>,
}

impl Worker {
    pub fn new(tx: Sender<Msg>, ctx: egui::Context, tools: Tools) -> Self {
        Worker {
            tx,
            ctx,
            tools,
            cache: Arc::new(ProbeCache::new()),
        }
    }

    fn send(&self, msg: Msg) {
        let _ = self.tx.send(msg);
        self.ctx.request_repaint();
    }

    /// Sucht `*.mkv` in Dateien/Ordnern (rekursiv).
    pub fn find(&self, paths: Vec<PathBuf>) {
        let w = self.clone();
        std::thread::spawn(move || {
            let (found, errors) = collect_mkv(&paths);
            w.send(Msg::Found { found, errors });
        });
    }

    /// Liest Dateien parallel ein (4 gleichzeitig).
    pub fn probe(&self, paths: Vec<PathBuf>, force: bool) {
        if paths.is_empty() {
            return;
        }
        let w = self.clone();
        std::thread::spawn(move || {
            if force {
                for p in &paths {
                    w.cache.invalidate(p);
                }
            }
            probe_many(&w.tools, &w.cache, &paths, 4, |i, result| {
                w.send(Msg::Probed {
                    path: paths[i].clone(),
                    result: result.map_err(|e| e.to_string()),
                });
            });
        });
    }

    /// Öffnet einen Datei-/Ordnerdialog (XDG-Portal) in einem eigenen Thread.
    pub fn pick(&self, kind: Pick, start: Option<PathBuf>) {
        let w = self.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new();
            if let Some(dir) = start.or_else(|| std::env::var_os("HOME").map(PathBuf::from)) {
                dialog = dialog.set_directory(dir);
            }
            let result: Vec<PathBuf> = match kind {
                Pick::AddFolders => dialog.pick_folders().unwrap_or_default(),
                Pick::AddFiles => dialog
                    .add_filter("Matroska", &["mkv", "MKV"])
                    .pick_files()
                    .unwrap_or_default(),
                Pick::OutDir => dialog.pick_folder().into_iter().collect(),
                Pick::PresetLoad => dialog
                    .add_filter("Preset", &["toml"])
                    .pick_file()
                    .into_iter()
                    .collect(),
                Pick::PresetSave => dialog
                    .add_filter("Preset", &["toml"])
                    .set_file_name("preset.toml")
                    .save_file()
                    .into_iter()
                    .collect(),
            };
            if !result.is_empty() {
                w.send(Msg::Picked(kind, result));
            }
        });
    }

    /// Führt die Warteschlange mit `jobs` parallelen Dateien aus.
    pub fn run(
        &self,
        items: Vec<WorkItem>,
        opts: JobOptions,
        jobs: usize,
        cancel: Arc<AtomicBool>,
    ) {
        let w = self.clone();
        std::thread::spawn(move || {
            let next = AtomicUsize::new(0);
            std::thread::scope(|scope| {
                for _ in 0..jobs.clamp(1, 8).min(items.len().max(1)) {
                    scope.spawn(|| {
                        loop {
                            if cancel.load(Ordering::Relaxed) {
                                break;
                            }
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            let Some(item) = items.get(i) else { break };
                            w.run_one(item, &opts, &cancel);
                        }
                    });
                }
            });
            w.send(Msg::RunFinished {
                cancelled: cancel.load(Ordering::Relaxed),
            });
        });
    }

    fn run_one(&self, item: &WorkItem, opts: &JobOptions, cancel: &AtomicBool) {
        let path = item.plan.path.clone();
        self.send(Msg::Job {
            path: path.clone(),
            msg: JobMsg::Started,
        });
        let mut last = u8::MAX;
        let result = run_job(
            &item.plan,
            &item.media,
            Some(Path::new(&item.root)),
            &self.tools,
            opts,
            cancel,
            &mut |ev| match ev {
                JobEvent::Progress(p) if p != last => {
                    last = p;
                    self.send(Msg::Job {
                        path: path.clone(),
                        msg: JobMsg::Progress(p),
                    });
                }
                JobEvent::Log(l) => self.send(Msg::Job {
                    path: path.clone(),
                    msg: JobMsg::Log(l),
                }),
                JobEvent::Progress(_) => {}
            },
        );
        self.send(Msg::Job {
            path,
            msg: JobMsg::Done(result.map_err(|e| e.to_string())),
        });
    }
}
