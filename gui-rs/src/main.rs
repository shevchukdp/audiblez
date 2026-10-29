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
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("--selftest") {
        std::process::exit(selftest(&args));
    }

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

/// Headless end-to-end check of the FULL-SERVER client path (the exact same
/// api::Client code the GUI uses): upload book -> submit job -> poll -> list
/// server outputs -> download the audiobook. Exits 0 with printed evidence.
///
/// audiblez --selftest <controller_url> <local_book> <out_file> [max_chapters] [tts_url]
fn selftest(args: &[String]) -> i32 {
    if args.len() < 5 {
        eprintln!("usage: audiblez --selftest <controller_url> <book> <out_file> [max_ch] [tts_url]");
        return 2;
    }
    let (url, book, out_file) = (&args[2], &args[3], &args[4]);
    let max_chapters: Option<i32> = args.get(5).and_then(|s| s.parse().ok());
    let tts_url = args
        .get(6)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"))
        .cloned();

    let bytes = match std::fs::read(book) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SELFTEST FAIL read book: {e}");
            return 1;
        }
    };
    let name = std::path::Path::new(book)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "book.epub".into());

    let rt = tokio::runtime::Runtime::new().expect("tokio");
    let client = {
        let _g = rt.enter();
        api::Client::new(url)
    };

    let server_path = match rt.block_on(client.upload(&name, bytes)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("SELFTEST FAIL upload: {e}");
            return 1;
        }
    };
    println!("SELFTEST uploaded {name} -> server path: {server_path}");

    let params = api::RunParams {
        file_path: server_path,
        voice: "af_bella".into(),
        speed: 1.0,
        output_folder: "auto".into(),
        model_path: None,
        device: None,
        tts_url,
        format_out: None,
        encode: None,
        bitrate: None,
        quality: None,
        include_cover: None,
        genre: None,
        pick_manually: Some(false),
        max_chapters,
    };
    if let Err(e) = rt.block_on(client.run(&params)) {
        eprintln!("SELFTEST FAIL run: {e}");
        return 1;
    }
    println!("SELFTEST job submitted; polling status...");

    let start = std::time::Instant::now();
    let final_status = loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        match rt.block_on(client.status()) {
            Ok(s) if s.status == "finished" || s.status == "error" => break s,
            Ok(_) => {}
            Err(e) => eprintln!("SELFTEST poll transient: {e}"),
        }
        if start.elapsed().as_secs() > 3600 {
            eprintln!("SELFTEST FAIL timeout waiting for job");
            return 1;
        }
    };
    println!(
        "SELFTEST job done in {:.0}s: status={} chapter={}/{} err='{}'",
        start.elapsed().as_secs_f32(),
        final_status.status,
        final_status.chapter,
        final_status.chapters_total,
        final_status.error
    );
    if final_status.status != "finished" {
        return 1;
    }

    let files = match rt.block_on(client.outputs("")) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("SELFTEST FAIL outputs: {e}");
            return 1;
        }
    };
    println!("SELFTEST server outputs: {} file(s)", files.len());
    for f in &files {
        println!("  - {} ({} bytes)", f.name, f.size);
    }
    let pick = files
        .iter()
        .filter(|f| f.name.ends_with(".m4b"))
        .max_by_key(|f| f.size)
        .or_else(|| files.iter().max_by_key(|f| f.size));
    let Some(pick) = pick else {
        eprintln!("SELFTEST FAIL server produced no outputs");
        return 1;
    };

    let data = match rt.block_on(client.download("", &pick.name)) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("SELFTEST FAIL download: {e}");
            return 1;
        }
    };
    if data.len() as u64 != pick.size {
        eprintln!("SELFTEST FAIL size mismatch: got {} want {}", data.len(), pick.size);
        return 1;
    }
    if let Some(par) = std::path::Path::new(out_file).parent() {
        let _ = std::fs::create_dir_all(par);
    }
    if let Err(e) = std::fs::write(out_file, &data) {
        eprintln!("SELFTEST FAIL write {out_file}: {e}");
        return 1;
    }
    println!("SELFTEST OK: downloaded {} ({} bytes) -> {out_file}", pick.name, data.len());
    0
}
