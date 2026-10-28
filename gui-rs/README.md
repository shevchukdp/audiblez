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

Verified working path is the standard **`x86_64-pc-windows-gnu`** target with a
real mingw-w64 toolchain on `PATH` (for `gcc`, and for `as`/`dlltool` which
generate the `raw-dylib` import libs used by `windows-sys`):

```powershell
cd gui-rs
# one-time: download & unpack a portable mingw-w64 (e.g. winlibs GCC), then:
$bin = "C:\temp\opencode\mingw-w64\mingw64\bin"   # dir containing gcc.exe/as.exe/dlltool.exe
$env:PATH = "$bin;$env:PATH"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "$bin\gcc.exe"
cargo build --release --target x86_64-pc-windows-gnu
# binary: gui-rs\target\x86_64-pc-windows-gnu\release\audiblez.exe
```

Requires the Rust toolchain (`rustup`):

```powershell
rustup target add x86_64-pc-windows-gnu
```

> **Why not `gnullvm`?** The `x86_64-pc-windows-gnullvm` target is *freestanding*:
> it expects clang to supply the full mingw CRT startup (`crtbegin/crtend/crtn.o`).
> With only rustup's bundled `crt2.o` and no clang, the produced exe crashes with
> `0xC0000005` in `__tmainCRTStartup` **before `main` runs**. Do not ship that
> target unless clang + the full CRT objects are available at link time.
>
> **Why not MSVC?** Needs the Windows SDK `Lib` (`kernel32.lib`, `libucrt.lib`, …);
> VS BuildTools alone (no SDK) cannot link.

### Linking (windows-gnu)

The `windows-sys` crates in the graph use `#[link(..., kind = "raw-dylib")]`, so
rustc spawns `dlltool` to synthesize import libraries at link time. rustup's
minimal self-contained `dlltool` cannot do this (it has no assembler to spawn),
so a **real mingw-w64 bin dir must be on `PATH`** — its `dlltool` + `as` handle
the job. Any winlibs/MSYS2 mingw-w64 (GCC 13+) works; e.g.
[winlibs](https://winlibs.com/) `winlibs-x86_64-posix-seh-gcc-*-mingw-w64ucrt-*.zip`.

## Run

```powershell
.\target\x86_64-pc-windows-gnu\release\audiblez.exe
```

Runtime evidence on the last verified build: process stays alive, Win32 reports
`IsWindow=True IsWindowVisible=True Title='audiblez'`, empty stderr, no panic.

## Remote CUDA GPU (spark01 / spark02)

Synthesis (Kokoro-82M) is the only heavy step — run it on a GPU box and keep
everything else local:

1. On the GPU box (any Python 3.10–3.12, NVIDIA GPU):

   ```bash
   pip install "kokoro>=0.7" fastapi uvicorn soundfile spacy
   # xx_ent_wiki_sm is needed on the *controller* machine, not here.
   # espeak-ng must be installed (apt install espeak-ng / equivalent).
   git clone https://github.com/shevchukdp/audiblez && cd audiblez
   uvicorn remote_tts_server.server:app --host 0.0.0.0 --port 8000
   ```

   The first `/synthesize` request downloads/loads `hexgrad/Kokoro-82M`
   (~330 MB) onto the GPU and reuses it for all later requests.

2. In the GUI, fill **TTS server** with e.g. `http://100.66.167.48:8000`
   (blank = synthesize on the controller's CPU). Check it via
   `GET http://<box>:8000/health` → `{"ok":true,"device":"cuda"}`.

Controller (`python -m audiblez.backend_controller --port 8765`) keeps running
locally so it can read your book files; only per-sentence inference goes over
HTTP. Verified: `tts_url` job finished, server logged the `/synthesize` hits,
`mao.m4b` produced.

> Note: `100.66.167.48:8888/v1` (the TensorFold LLM server) is an **LLM**, not
> TTS — it cannot synthesize speech. ffmpeg's CUDA options are video-only and
> irrelevant here; AAC encoding is not the bottleneck.

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
