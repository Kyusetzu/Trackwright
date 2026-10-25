//! Trackwright – grafische Oberfläche (egui/eframe).
//!
//! Aufruf: `trackwright-gui [DATEIEN|ORDNER …]`

mod app;
mod settings;
mod state;
mod strings;
mod worker;

use trackwright_core::tools::Tools;

struct GuiApp(app::App);

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.show(ui);
    }
}

/// winit 0.30 liefert unter Wayland keine Datei-Drops (nur X11). Deshalb läuft die GUI
/// standardmäßig über XWayland, damit Drag & Drop funktioniert. `TRACKWRIGHT_WAYLAND=1`
/// erzwingt das native Wayland-Backend (ohne Drag & Drop).
#[cfg(target_os = "linux")]
fn prefer_x11(builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>) {
    use winit::platform::x11::EventLoopBuilderExtX11;
    let native_wayland = std::env::var_os("TRACKWRIGHT_WAYLAND").is_some_and(|v| v != "0");
    if !native_wayland && std::env::var_os("DISPLAY").is_some() {
        builder.with_x11();
    }
}

#[cfg(not(target_os = "linux"))]
fn prefer_x11(_: &mut eframe::EventLoopBuilder<eframe::UserEvent>) {}

fn main() -> eframe::Result {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::WARN)
        .init();
    let initial: Vec<std::path::PathBuf> = std::env::args_os().skip(1).map(Into::into).collect();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(strings::APP_TITLE)
            .with_app_id("trackwright")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 500.0])
            .with_drag_and_drop(true),
        event_loop_builder: Some(Box::new(prefer_x11)),
        ..Default::default()
    };
    eframe::run_native(
        "trackwright",
        options,
        Box::new(move |cc| {
            let mut app = app::App::new(cc.egui_ctx.clone(), Tools::default());
            if let Some(path) = settings::Settings::default_path() {
                app.apply_settings(settings::Settings::load(&path));
            }
            if let [dir] = initial.as_slice()
                && dir.is_dir()
            {
                app.model.source_path = dir.display().to_string();
            }
            app.add_paths(initial);
            Ok(Box::new(GuiApp(app)))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Rendert alle Tabs headless – fängt Panics und Layoutfehler ab.
    #[test]
    fn renders_all_tabs_headless() {
        let ctx = egui::Context::default();
        let mut app = app::App::new(ctx.clone(), Tools::default());
        let fixtures =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/json");
        // Ohne echte Dateien: Fixture-JSONs als eingelesene Dateien einspeisen.
        for (i, name) in ["full.json", "und.json", "full.json"].iter().enumerate() {
            let path = std::path::PathBuf::from(format!("/media/Serie/E0{i}.mkv"));
            let json = std::fs::read_to_string(fixtures.join(name)).expect("fixture");
            let mut m = trackwright_core::probe::parse_identification(&json, &path).expect("parse");
            m.size = 1_000_000;
            app.model
                .add_found(vec![trackwright_core::fsutil::FoundFile {
                    path: path.clone(),
                    root: "/media".into(),
                }]);
            app.model.set_probe(&path, Ok(m));
        }
        app.model.audio.keep_enabled = true;
        app.model
            .audio
            .keep
            .insert(trackwright_core::selection::TrackRef::Number(1));
        app.model.subtitles.default_mode = state::DefaultMode::DefaultForced;
        app.model.selected = Some(0);
        app.model.mark_dirty();
        for tab in [app::Tab::Videos, app::Tab::Mux] {
            app.tab = tab;
            for _ in 0..2 {
                let out = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
                out.drop_without_applying_deltas();
            }
        }
        assert_eq!(app.model.summary().remux, 3);
    }

    /// Echte Dateien über den Worker einlesen (überspringt ohne MKVToolNix).
    #[test]
    fn worker_probes_real_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let script =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/make.sh");
        let ok = std::process::Command::new("bash")
            .arg(&script)
            .arg(dir.path())
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            eprintln!("übersprungen: Fixtures konnten nicht erzeugt werden");
            return;
        }
        let ctx = egui::Context::default();
        let mut app = app::App::new(ctx.clone(), Tools::default());
        app.add_paths(vec![dir.path().to_path_buf()]);
        let start = Instant::now();
        loop {
            app.poll();
            app.model.refresh();
            let ready =
                app.model.files.len() == 2 && app.model.files.iter().all(|f| f.media().is_some());
            if ready {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(30), "Timeout");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(app.model.audio_options.numbers.len(), 3);
    }

    /// Screenshots für die README: echte Beispieldateien, echter Lauf.
    ///
    ///   cargo test -p trackwright-gui --release -- --ignored readme_screenshots
    ///
    /// Öffnet kurz ein Fenster (braucht Display und MKVToolNix) und schreibt
    /// `docs/screenshot-mux.png` und `docs/screenshot-videos.png`.
    #[test]
    #[ignore = "öffnet ein Fenster; manuell für die README"]
    fn readme_screenshots() {
        use std::sync::{Arc, Mutex};
        use trackwright_core::lang::Lang;
        use trackwright_core::selection::TrackRef;
        use winit::platform::x11::EventLoopBuilderExtX11;

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let tmp = tempfile::tempdir().expect("tempdir");
        let raw = tmp.path().join("raw");
        let ok = std::process::Command::new("bash")
            .arg(root.join("tests/fixtures/make.sh"))
            .arg(&raw)
            .status()
            .is_ok_and(|s| s.success());
        assert!(ok, "Fixtures konnten nicht erzeugt werden");
        let src = tmp.path().join("Example Show");
        std::fs::create_dir_all(&src).expect("mkdir");
        for i in 1..=6 {
            std::fs::copy(
                raw.join("full.mkv"),
                src.join(format!("Example Show S01E0{i}.mkv")),
            )
            .expect("copy");
        }
        std::fs::copy(raw.join("und.mkv"), src.join("Example Show Bonus.mkv")).expect("copy");
        let out = tmp.path().join("Fertig");

        struct Shot {
            app: app::App,
            out: std::path::PathBuf,
            docs: std::path::PathBuf,
            phase: u8,
            frames: u32,
            image: Arc<Mutex<Option<Arc<egui::ColorImage>>>>,
            requested: bool,
        }

        impl Shot {
            fn save(&self, name: &str, img: &egui::ColorImage) {
                let path = self.docs.join(name);
                let file = std::fs::File::create(&path).expect("png");
                let mut enc = png::Encoder::new(
                    std::io::BufWriter::new(file),
                    img.size[0] as u32,
                    img.size[1] as u32,
                );
                enc.set_color(png::ColorType::Rgba);
                enc.set_depth(png::BitDepth::Eight);
                let data: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                enc.write_header()
                    .and_then(|mut w| w.write_image_data(&data))
                    .expect("png schreiben");
                eprintln!("geschrieben: {}", path.display());
            }

            /// Fordert nach ein paar ruhigen Frames einen Screenshot an; `true`, sobald gespeichert.
            fn shoot(&mut self, ctx: &egui::Context, name: &str) -> bool {
                self.frames += 1;
                if self.frames > 15 && !self.requested {
                    self.requested = true;
                    let slot = self.image.clone();
                    ctx.request_screenshot(move |img| *slot.lock().expect("lock") = Some(img));
                }
                let img = self.image.lock().expect("lock").take();
                match img {
                    Some(img) => {
                        self.save(name, &img);
                        self.frames = 0;
                        self.requested = false;
                        true
                    }
                    None => false,
                }
            }
        }

        impl eframe::App for Shot {
            fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
                let ctx = ui.ctx().clone();
                self.app.show(ui);
                ctx.request_repaint();
                let m = &mut self.app.model;
                match self.phase {
                    0 => {
                        if m.files.len() == 7 && m.files.iter().all(|f| f.media().is_some()) {
                            // Nur für die Anzeige: lesbare statt /tmp-Pfade.
                            m.source_path = "~/Videos/Example Show".into();
                            m.destination = "~/Videos/Done".into();
                            m.audio.keep_enabled = true;
                            m.audio.keep.insert(TrackRef::Lang(Lang::parse("ger")));
                            m.audio.keep.insert(TrackRef::Lang(Lang::parse("jpn")));
                            m.audio.default_mode = state::DefaultMode::Default;
                            m.audio.default_ref = Some(TrackRef::Lang(Lang::parse("jpn")));
                            m.subtitles.keep_enabled = true;
                            m.subtitles.keep.insert(TrackRef::Lang(Lang::parse("ger")));
                            m.selected = Some(1);
                            m.mark_dirty();
                            self.app.tab = app::Tab::Mux;
                            self.phase = 1;
                        }
                    }
                    1 => {
                        if self.shoot(&ctx, "screenshot-mux.png") {
                            self.app.model.destination = self.out.display().to_string();
                            self.app.model.mark_dirty();
                            self.app.model.refresh();
                            self.app.start();
                            self.app.tab = app::Tab::Videos;
                            self.phase = 2;
                        }
                    }
                    2 => {
                        if !self.app.is_running() {
                            self.phase = 3;
                        }
                    }
                    _ => {
                        if self.shoot(&ctx, "screenshot-videos.png") {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
            }
        }

        let docs = root.join("docs");
        std::fs::create_dir_all(&docs).expect("docs");
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title(strings::APP_TITLE)
                .with_inner_size([1200.0, 760.0])
                .with_resizable(false),
            event_loop_builder: Some(Box::new(|b| {
                b.with_x11();
                b.with_any_thread(true);
            })),
            ..Default::default()
        };
        eframe::run_native(
            "trackwright-screenshot",
            options,
            Box::new(move |cc| {
                let mut app = app::App::new(cc.egui_ctx.clone(), Tools::default());
                app.add_paths(vec![src]);
                Ok(Box::new(Shot {
                    app,
                    out,
                    docs,
                    phase: 0,
                    frames: 0,
                    image: Arc::default(),
                    requested: false,
                }))
            }),
        )
        .expect("eframe");
        drop(tmp);
    }
}
