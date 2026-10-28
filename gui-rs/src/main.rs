//! Native Windows GUI entry point for audiblez (egui).
//!
//! Build: `cargo build --release` inside `gui-rs`.
//! Run:   `target/release/audiblez`

mod api;
mod app;
#[cfg(all(target_os = "windows", target_env = "gnullvm"))]
mod crt;
mod dialogs;
mod settings;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 720.0])
            .with_title("audiblez"),
        ..Default::default()
    };

    let settings = settings::Settings::load();

    // Single multi-thread Tokio runtime that owns all reqwest I/O. It must outlive
    // the GUI (App holds an Arc to it) and `reqwest::Client` construction must
    // happen while the runtime context is entered.
    let rt = std::sync::Arc::new(
        tokio::runtime::Runtime::new().expect("failed to start tokio runtime"),
    );
    let client = {
        let _guard = rt.enter();
        api::Client::new(&settings.controller_url)
    };
    let app = app::App::new(client, settings, rt);

    eframe::run_native(
        "audiblez",
        native,
        // Note: the error type is intentionally left un-annotated so it infers
        // to `Box<dyn Error + Send + Sync>` (eframe's AppCreator error type),
        // rather than being pinned to `eframe::Error`.
        Box::new(move |_cc: &eframe::CreationContext<'_>| Ok(Box::new(app))),
    )
    .map_err(|e| e.to_string())
    .ok();
}
