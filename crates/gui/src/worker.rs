//! Hintergrundarbeit: Dateien suchen, einlesen, Dialoge, Ausführung. Ergebnisse kommen als
//! [`Msg`] über einen Kanal zur UI.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    AddFiles,
    SourceFolder,
    Destination,
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
        aborted: bool,
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
                Pick::AddFiles => dialog
                    .add_filter("Matroska", &["mkv", "MKV"])
                    .pick_files()
                    .unwrap_or_default(),
                Pick::SourceFolder | Pick::Destination => {
                    dialog.pick_folder().into_iter().collect()
                }
            };
            if !result.is_empty() {
                w.send(Msg::Picked(kind, result));
            }
        });
    }

    /// Arbeitet die Warteschlange nacheinander ab (wie im Vorbild). Mit `abort_on_errors` stoppt
    /// der erste Fehler die restliche Warteschlange.
    pub fn run(
        &self,
        items: Vec<WorkItem>,
        opts: JobOptions,
        abort_on_errors: bool,
        cancel: Arc<AtomicBool>,
    ) {
        let w = self.clone();
        std::thread::spawn(move || {
            let mut aborted = false;
            for item in &items {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let ok = w.run_one(item, &opts, &cancel);
                if !ok && abort_on_errors && !cancel.load(Ordering::Relaxed) {
                    aborted = true;
                    break;
                }
            }
            w.send(Msg::RunFinished {
                cancelled: cancel.load(Ordering::Relaxed),
                aborted,
            });
        });
    }

    /// Liefert `true` bei Erfolg.
    fn run_one(&self, item: &WorkItem, opts: &JobOptions, cancel: &AtomicBool) -> bool {
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
        let ok = result.is_ok();
        self.send(Msg::Job {
            path,
            msg: JobMsg::Done(result.map_err(|e| e.to_string())),
        });
        ok
    }
}
