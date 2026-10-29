use std::sync::{Arc, Mutex};

use eframe::{egui, App as EframeApp};
use egui::RichText;

use crate::api::{self, Client, RunParams};
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
    rt: std::sync::Arc<tokio::runtime::Runtime>,
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
    // full-server mode: outputs listed by the server after the job finished,
    // plus the folder they came from (for downloads).
    server_files: Vec<api::OutputFile>,
    server_folder: String,
}

impl App {
    pub fn new(client: Client, settings: Settings, rt: std::sync::Arc<tokio::runtime::Runtime>) -> Self {
        let output = std::env::temp_dir();
        let mut this = App {
            client,
            rt,
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
            server_files: Vec::new(),
            server_folder: String::new(),
        };
        this.refresh_status();
        this
    }

    fn poll(&mut self) {
        let rt = self.rt.clone();
        match rt.block_on(self.client.status()) {
            Ok(s) => {
                self.job.status = s.status.clone();
                self.job.error = s.error.clone();
                self.job.running = s.status == "running";
                self.job.chapters_total = s.chapters_total.max(0) as i32;
                self.job.chapter = s.chapter.to_string();
                if let Some(stats) = s.stats.as_object() {
                    if let Some(p) = stats.get("progress").and_then(|v| v.as_f64()) {
                        // server sends 0-100 already
                        self.job.progress = p.clamp(0.0, 100.0) as f32;
                    }
                    if let Some(eta) = stats.get("eta").and_then(|v| v.as_str()) {
                        self.job.eta = eta.to_string();
                    }
                }
                // Full-server mode: once the server finished, list its outputs
                // so the user can download the audiobook to the laptop.
                if self.job.status == "finished"
                    && self.server_files.is_empty()
                    && self.settings.lock().unwrap().full_server
                {
                    match rt.block_on(self.client.outputs(&self.server_folder)) {
                        Ok(files) => self.server_files = files,
                        Err(e) => self.last_message = format!("outputs: {e}"),
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
        let rt = self.rt.clone();
        let reachable = rt.block_on(self.client.is_reachable());
        if reachable {
            self.poll();
        } else {
            self.job.status = "unreachable".into();
            self.job.error = "controller not reachable".into();
        }
    }

    fn on_run(&mut self) {
        let full_server = self.settings.lock().unwrap().full_server;
        // Full-server mode: the laptop only uploads the book; every byte of
        // processing (TTS, ffmpeg) runs on the server.
        let server_file_path = if full_server {
            if self.file_path.trim().is_empty() {
                self.last_message = "pick a book file first".into();
                return;
            }
            let bytes = match std::fs::read(&self.file_path) {
                Ok(b) => b,
                Err(e) => {
                    self.last_message = format!("read book: {e}");
                    return;
                }
            };
            let name = std::path::Path::new(&self.file_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "book.epub".into());
            let rt = self.rt.clone();
            match rt.block_on(self.client.upload(&name, bytes)) {
                Ok(p) => p,
                Err(e) => {
                    self.last_message = e;
                    return;
                }
            }
        } else {
            self.file_path.clone()
        };
        let params = RunParams {
            file_path: server_file_path,
            voice: self.settings.lock().unwrap().default_voice.clone(),
            speed: 1.0,
            output_folder: if full_server { "auto".into() } else { self.output_folder.clone() },
            model_path: non_blank(&self.model_path),
            device: non_blank(&self.device),
            tts_url: non_blank(&self.settings.lock().unwrap().tts_url.clone()),
            format_out: non_blank(&self.format_out),
            encode: non_blank(&self.encode),
            bitrate: non_blank(&self.settings.lock().unwrap().bitrate.clone()),
            quality: Some(self.settings.lock().unwrap().quality),
            include_cover: Some(self.settings.lock().unwrap().cover),
            genre: non_blank(&self.genre),
            pick_manually: Some(false),
            max_chapters: None,
        };
        let rt = self.rt.clone();
        let _ = self.settings.lock().unwrap().save();
        self.server_files.clear();
        self.server_folder = String::new();
        match rt.block_on(self.client.run(&params)) {
            Ok(_) => self.last_message = if full_server {
                "book uploaded; job running on server".into()
            } else {
                "job submitted".into()
            },
            Err(e) => self.last_message = e,
        }
    }

    /// Download one server-side output to a local path chosen by the user.
    fn save_server_file(&mut self, name: &str) {
        let folder = self.server_folder.clone();
        let name = name.to_string();
        let target = match dialogs::save_file("Save audiobook", &name) {
            Some(p) => p,
            None => return,
        };
        let rt = self.rt.clone();
        match rt.block_on(self.client.download(&folder, &name)) {
            Ok(bytes) => match std::fs::write(&target, &bytes) {
                Ok(_) => self.last_message =
                    format!("saved {} ({} bytes) to {}", name, bytes.len(), target),
                Err(e) => self.last_message = format!("write failed: {e}"),
            },
            Err(e) => self.last_message = e,
        }
    }

    fn on_stop(&mut self) {
        let rt = self.rt.clone();
        match rt.block_on(self.client.stop()) {
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

            ui.add_space(4.0);
            ui.label("TTS server (optional, CUDA GPU box):");
            ui.text_edit_singleline(&mut settings.lock().unwrap().tts_url);
            hint(
                ui,
                "e.g. http://100.66.167.48:8000 — runs Kokoro on the GPU. Blank = synthesize on the controller.",
            );
            ui.checkbox(
                &mut self.settings.lock().unwrap().full_server,
                "Full server mode (upload book; ALL processing incl. ffmpeg runs on the server)",
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

            if !self.server_files.is_empty() {
                ui.add_space(6.0);
                ui.label("Server outputs:");
                let files = self.server_files.clone();
                for f in files {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} ({:.1} MB)",
                            f.name,
                            f.size as f64 / 1_048_576.0
                        ));
                        if ui.button("Save\u{2026}").clicked() {
                            self.save_server_file(&f.name);
                        }
                    });
                }
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
