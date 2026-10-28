use std::sync::{Arc, Mutex};

use eframe::{egui, App as EframeApp};
use egui::RichText;

use crate::api::{Client, RunParams};
use crate::dialogs;
use crate::settings::Settings;

/// One synthesis job, polled from the controller over HTTP.
pub struct Job {
    pub status: String,
    pub progress: f32,
    pub eta: String,
    pub chapter: String,
    pub chapters_total: i32,
    pub error: String,
    pub running: bool,
}

impl Job {
    pub fn idle() -> Self {
        Job {
            status: "idle".into(),
            progress: 0.0,
            eta: String::new(),
            chapter: String::new(),
            chapters_total: 0,
            error: String::new(),
            running: false,
        }
    }
}

/// Native egui desktop GUI for audiblez.
///
/// Transport-agnostic: `client` talks to a local backend controller
/// (127.0.0.1) or a remote one on spark01 / spark02. Same UI, same JSON.
pub struct App {
    client: Client,
    settings: Arc<Mutex<Settings>>,
    job: Job,
    // text fields
    file_path: String,
    output_folder: String,
    model_path: String,
    device: String,
    format_out: String,
    encode: String,
    genre: String,
    // feedback
    last_message: String,
}

impl App {
    pub fn new(client: Client, settings: Settings) -> Self {
        let output = std::env::temp_dir();
        let mut this = App {
            client,
            settings: Arc::new(Mutex::new(settings)),
            job: Job::idle(),
            file_path: String::new(),
            output_folder: output.to_string_lossy().to_string(),
            model_path: String::new(),
            device: "auto".into(),
            format_out: "m4b".into(),
            encode: "cbr".into(),
            genre: "Audiobook".into(),
            last_message: String::new(),
        };
        this.refresh_status();
        this
    }

    fn poll(&mut self) {
        match pollster::block_on(self.client.status()) {
            Ok(s) => {
                self.job.status = s.status.clone();
                self.job.error = s.error.clone();
                self.job.running = s.status == "running";
                self.job.chapters_total = s.chapters_total.max(0) as i32;
                self.job.chapter = s.chapter.to_string();
                if let Some(stats) = s.stats.as_object() {
                    if let Some(p) = stats.get("progress").and_then(|v| v.as_f64()) {
                        self.job.progress = (p * 100.0) as f32;
                    }
                    if let Some(eta) = stats.get("eta").and_then(|v| v.as_str()) {
                        self.job.eta = eta.to_string();
                    }
                }
            }
            Err(e) => {
                if !self.job.running {
                    self.job.status = "unreachable".into();
                    self.job.error = e.clone();
                }
            }
        }
    }

    fn refresh_status(&mut self) {
        let reachable = self.client.is_reachable();
        if reachable {
            self.poll();
        } else {
            self.job.status = "unreachable".into();
            self.job.error = "controller not reachable".into();
        }
    }

    fn on_run(&mut self) {
        let params = RunParams {
            file_path: self.file_path.clone(),
            voice: self.settings.lock().unwrap().default_voice.clone(),
            speed: 1.0,
            output_folder: self.output_folder.clone(),
            model_path: non_blank(&self.model_path),
            device: non_blank(&self.device),
            format_out: non_blank(&self.format_out),
            encode: non_blank(&self.encode),
            bitrate: non_blank(&self.settings.lock().unwrap().bitrate.clone()),
            quality: Some(self.settings.lock().unwrap().quality),
            include_cover: Some(self.settings.lock().unwrap().cover),
            genre: non_blank(&self.genre),
            pick_manually: Some(false),
        };
        match pollster::block_on(self.client.run(&params)) {
            Ok(_) => self.last_message = "job submitted".into(),
            Err(e) => self.last_message = e,
        }
    }

    fn on_stop(&mut self) {
        match pollster::block_on(self.client.stop()) {
            Ok(_) => self.last_message = "stop requested".into(),
            Err(e) => self.last_message = e,
        }
    }

    fn open_file_dialog(&mut self) {
        match dialogs::open_file("Open book / audio file") {
            Some(path) => self.file_path = path,
            None => {} // cancelled or error
        }
    }

    fn open_output_folder(&mut self) {
        if self.output_folder.is_empty() {
            if let Some(path) = dialogs::save_file("Where to save", "audiblez-output") {
                self.output_folder = path;
            }
            return;
        }
        dialogs::open_folder(&self.output_folder);
    }
}

fn non_blank(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().weak());
}

impl EframeApp for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("audiblez — audiobook generator");
                ui.add_space(8.0);
                ui.add(egui::Hyperlink::from_label_and_url(
                    "github.com/shevchukdp/audiblez",
                    "github.com/shevchukdp/audiblez",
                ));
            });
        });

        // Left: controls. Right: status / progress.
        egui::SidePanel::left("controls").resizable(true).default_width(560.0).show(ctx, |ui| {
            ui.heading("Source");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.file_path);
                if ui.button("Browse\u{2026}").clicked() {
                    self.open_file_dialog();
                }
            });
            hint(ui, "An .epub, .pdf, .fb2, .txt file or a folder of audio files.");

            ui.add_space(8.0);
            ui.heading("Compute");
            let settings = self.settings.clone();
            egui::ComboBox::from_label("Mode")
                .selected_text(if settings.lock().unwrap().remote { "remote" } else { "local" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut settings.lock().unwrap().remote, false, "local");
                    ui.selectable_value(&mut settings.lock().unwrap().remote, true, "remote");
                });
            ui.text_edit_singleline(&mut settings.lock().unwrap().controller_url);
            hint(
                ui,
                "local = 127.0.0.1 controller; remote = http://spark01:8765 or spark02",
            );

            ui.add_space(8.0);
            ui.text_edit_singleline(&mut self.model_path);
            hint(ui, "Kokoro model path (local clone with weights + voices/). Leave blank for default.");
            egui::ComboBox::from_label("Device")
                .selected_text(self.device.clone())
                .show_ui(ui, |ui| {
                    for d in ["auto", "cpu", "cuda", "cuda:0", "cuda:1", "mps"] {
                        ui.selectable_value(&mut self.device, d.to_string(), d);
                    }
                });

            ui.add_space(8.0);
            ui.heading("Output");
            egui::ComboBox::from_label("Format")
                .selected_text(self.format_out.clone())
                .show_ui(ui, |ui| {
                    for f in ["m4b", "mp3", "wav"] {
                        ui.selectable_value(&mut self.format_out, f.to_string(), f);
                    }
                });
            egui::ComboBox::from_label("Encode")
                .selected_text(self.encode.clone())
                .show_ui(ui, |ui| {
                    for e in ["cbr", "vbr"] {
                        ui.selectable_value(&mut self.encode, e.to_string(), e);
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Bitrate (kbps)");
                egui::ComboBox::from_id_salt("bitrate").selected_text(
                    self.settings.lock().unwrap().bitrate.clone(),
                )
                .show_ui(ui, |ui| {
                    for b in ["64", "96", "128", "192", "256", "320"] {
                        ui.selectable_value(
                            &mut settings.lock().unwrap().bitrate,
                            b.to_string(),
                            b,
                        );
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label("VBR quality (0-9)");
                let mut guard = self.settings.lock().unwrap();
                ui.add(egui::Slider::new(&mut guard.quality, 0..=9));
            });

            ui.add_space(8.0);
            ui.checkbox(&mut self.settings.lock().unwrap().cover, "Include cover art (PIC/APIC)");
            ui.horizontal(|ui| {
                ui.label("Genre");
                ui.text_edit_singleline(&mut self.genre);
            });
            ui.add_space(8.0);
            ui.text_edit_singleline(&mut self.output_folder);
            if ui.button("Open output folder\u{2026}").clicked() {
                self.open_output_folder();
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if self.job.running {
                    if ui.button("\u{23F9} Stop").clicked() {
                        self.on_stop();
                    }
                } else {
                    if ui.button("\u{1F680} Start synthesis").clicked() {
                        self.on_run();
                    }
                }
            });
            if !self.last_message.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(&self.last_message).small());
            }
        });

        // Right: status panel.
        egui::SidePanel::right("status").resizable(true).default_width(320.0).show(ctx, |ui| {
            ui.heading("Status");
            ui.add_space(6.0);

            let (color, label) = if self.job.running {
                (egui::Color32::YELLOW, "running")
            } else {
                match self.job.status.as_str() {
                    "finished" => (egui::Color32::GREEN, "finished"),
                    "error" => (egui::Color32::RED, "error"),
                    "unreachable" => (egui::Color32::LIGHT_RED, "unreachable"),
                    _ => (egui::Color32::LIGHT_GRAY, "idle"),
                }
            };
            ui.label(RichText::new(label).color(color));
            ui.add_space(4.0);

            ui.add(egui::ProgressBar::new(self.job.progress / 100.0).text(format!(
                "{}%",
                self.job.progress as i32
            )));

            ui.add_space(6.0);
            if self.job.chapters_total > 0 {
                ui.label(format!("Chapter {} of {}", self.job.chapter, self.job.chapters_total));
            }
            if !self.job.eta.is_empty() {
                ui.label(format!("ETA: {}", self.job.eta));
            }
            if self.job.status == "error"
                || (self.job.status == "unreachable" && !self.job.error.is_empty())
            {
                ui.add_space(4.0);
                ui.label(RichText::new(&self.job.error).color(egui::Color32::RED));
            }
        });

        ctx.request_repaint(); // keep polling for progress
    }
}
