# audiblez native GUI (Rust + egui)

Native **Windows** desktop GUI for audiblez, written in **Rust** with [egui](https://www.egui.rs/).
This replaces the Python/wxPython GUI (`audiblez/ui.py`).

## Why Rust?

- Ships a single compiled native `.exe` — no Python install / venv needed on the
  user's machine.
- Native Windows windowing and file dialogs.
- Small footprint, fast startup.

## Architecture

```
┌──────────────────────────┐      HTTP/JSON        ┌───────────────────────────┐
│  Rust egui GUI (Windows)  │  ──────────────────▶  │  backend controller (py)   │
│  target/release/audiblez  │                         │  runs audiblez.core in a   │
│                          │◀───────────────────│  background thread + ffmpeg    │
└──────────────────────────┘                        └───────────────────────────┘
```

The GUI is **transport-agnostic**. Two modes:

- **local** — GUI talks to a controller on `127.0.0.1:8765` (the bundled
  `audiblez/backend_controller.py`).
- **remote** — GUI talks to a controller on `spark01` / `spark02` (the same
  `backend_controller.py` running on the GPU box).

Either way, synthesis runs on the server; the GUI only sends JSON and renders
progress.

## Build

```powershell
cd gui-rs
cargo build --release --target x86_64-pc-windows-gnullvm
# binary: gui-rs\target\x86_64-pc-windows-gnullvm\release\audiblez.exe
```

Requires the Rust toolchain (`rustup`) plus the LLVM-GNU target:

```powershell
rustup target add x86_64-pc-windows-gnullvm
```

### Linking (Windows, no MSVC/clang)

This target links with the bundled `rust-lld` (see `gui-rs\.cargo\config.toml`)
because the machine has neither an MSVC `link.exe` nor a 64-bit clang/LLVM. The
`rust-lld` linker is pointed at the toolchain's bundled Windows import libraries
(`kernel32`, `user32`, `opengl32`, ...) via the `rustflags` in that config, so no
system linker or `dlltool` is needed.

> Note: building with the plain `--target x86_64-pc-windows-gnu` target instead
> hits `dlltool` import-lib generation errors on some crates; the gnullvm target
> avoids that.

## Run

```powershell
.\target\release\audiblez.exe
```

Settings (controller URL, remote/local, bitrate, quality, cover, default voice)
are persisted next to the executable as `audiblez.json`.

## Files

| File | Purpose |
|------|---------|
| `src/main.rs` | entry point, eframe bootstrapping |
| `src/app.rs` | egui app + UI (source, compute, output panels, status) |
| `src/api.rs` | HTTP client (`GET /status`, `POST /run`, `POST /stop`) |
| `src/settings.rs` | persistent JSON config |

## Backend controller (Python)

`audiblez/backend_controller.py` — a tiny threaded HTTP server that runs the
existing audiblez engine in a background thread and streams progress as JSON.

```powershell
python -m audiblez.backend_controller --port 8765
```

## Remote GPU server (spark01 / spark02)

`remote_tts_server/server.py` — FastAPI server that loads Kokoro-on-CUDA once
and reuses it across requests. Point the GUI at `http://spark01:8000` or
`http://spark02:8000` when using the remote server directly.

## License

MIT. Original author: Claudio Santini.
