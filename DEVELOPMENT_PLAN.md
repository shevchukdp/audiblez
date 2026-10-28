# Development Plan — audiblez

Roadmap for the fork at https://github.com/shevchukdp/audiblez.
Each item maps to a task, an expected version bump, and notes on the change.

> Versioning follows semver. Current released version: **0.4.9**
> (kokoro `0.9.4`). New features are targeted at **1.0.0**.

## Architecture seam (applies to every task)

Only the `KPipeline(...)` call in `audiblez/core.py` is GPU-bound.
Everything else (book parsing, chapter detection, spacy, numpy, soundfile,
ffmpeg assembly, and the whole wxPython GUI) stays local on Windows.

```
make_tts(voice, model_path, device) -> backend
backend.synthesize(text, voice, speed) -> audio bytes
```

---

## Task 1 — Remote CUDA backend (GUI on Windows, compute on a GPU server)

- **Goal:** synthesis runs on a remote GPU box; the wxPython GUI and ffmpeg stay local.
- **GPU servers:** `spark01` and `spark02` (see `remote_tts_server/`).
- **Change:** pluggable backend abstraction (`audiblez/tts_backend.py`):
  - `LocalBackend` — wraps `KPipeline` locally (CPU/CUDA).
  - `RemoteBackend` — sends text per sentence over HTTP, receives raw WAV bytes.
- **Server:** standalone FastAPI app (`remote_tts_server/`) that loads
  `KPipeline(device="cuda", repo_id=<model_path>)` once and reuses it.
- **GUI:** add **TTS mode** (Local / Remote) + **Server URL** field.
- **Version:** 1.0.0.

## Task 2 — Model path + device selection in the interface

- **Goal:** let the user choose the model location and device from the GUI.
- **Why:** `KPipeline(lang_code, repo_id=..., device=...)` accepts a local path
  and a device string.
- **Change:** GUI params panel gains **Model path** (accepts a local Kokoro repo
  clone — weights **and** `voices/`) and **Device** combo (`auto/cpu/cuda/cuda:0/cuda:1/mps`).
- **Caveat:** folder must contain both weights and `voices/`, else voice loading
  falls back to HF download (needs internet).
- **Version:** 1.0.0.

## Task 3 — Audio input support (MP3 / AAC / WAV)

- **Goal:** generate audiobooks from audio files, not just EPUB.
- **Change:** extension-dispatching input abstraction
  (`load_document_chapters(file_path)`):
  - `.epub` — existing `epub.read_epub` path (unchanged).
  - `.fb2` / `.fb2.zip` — Task 6.
  - `.mp3`/`.aac`/`.wav`/`.m4a` — decode with `soundfile`, split by embedded
    chapters / `.cue` / track order / single track.
- Produce the **same chapter objects** as EPUB so existing extraction + GUI table work.
- **GUI:** add audio to the file-dialog wildcard + an **Input type** dropdown.
- **Version:** 1.1.0.

## Task 4 — Encode mode (CBR / VBR) + bitrate / quality

- **Goal:** user-controlled encoding instead of the two hardcoded ffmpeg lines.
- **Change:** GUI controls **Encode mode** (CBR/VBR), **Bitrate**
  (`64/96/128/192/256/320` kbps), **VBR quality** (`0–9`).
- `core.py` builds ffmpeg args from these (`-b:a`, `-q:a`).
- **Version:** 1.1.0.

## Task 5 — Output format + tags (PIC / APIC + ID3/ID3v2)

- **Goal:** choose output container and write cover art + metadata tags.
- **Change:**
  - **Format** combo (`m4b`/`mp3`/`wav`); **Cover art** checkbox (default on);
    **Genre** field; optionally editable Title/Artist/Album.
  - `m4b` keeps its PIC (already present), now also carries title/artist/album/genre.
  - `mp3` gets an **APIC** (PIC) frame + **ID3v2/ID3v1** tags via ffmpeg.
  - `creator` → `artist` + `album_artist`; `title` → `title` + `album`.
- **Version:** 1.1.0.

## Task 6 — FictionBook (Fb2) input support

- **Goal:** support `.fb2` and `.fb2.zip`.
- **Why:** `ebooklib` is EPUB-only, but Fb2 is a zip of XML parseable by the
  same BeautifulSoup path used for EPUB XHTML.
- **Change:** `read_fb2(file_path)` turns each `<section>` into the same
  chapter objects EPUB produces; title/author extracted from
  `<description><title-info>`.
- **GUI:** add Fb2 to the file dialog + input dropdown; reuse chapter table.
- **Version:** 1.1.0.

---

## Cross-cutting tasks

- [ ] **CLI parity** — mirror every GUI option in `audiblez/cli.py`.
- [ ] **Settings persistence** — remember last-used model path / device / TTS mode
      / URL / format / bitrate / quality / cover / genre (JSON prefs file).
- [ ] **Validation & feedback** — surface errors for incomplete model folders,
      unreachable remote URLs, or missing espeak-ng.
- [ ] **Remote-mode ETA** — keep progress accurate at sentence granularity.
- [ ] **Docs** — update README with new flags, remote-server setup, and the
      standalone TTS server.
- [ ] **Changelog** — maintain [CHANGELOG.md](./CHANGELOG.md) per release.

## Suggested implementation order

1. **Task 2** (model path + device) — small; unblocks fully-offline local runs.
2. **Task 1** (remote backend + server) — the headline feature.
3. **Task 5 + Task 4** (format, tags, CBR/VBR/bitrate).
4. **Task 6 + Task 3** (Fb2, then audio input).
5. Cross-cutting polish + version bump + release.

## Release process

- Bump `version` in `pyproject.toml` and `audiblez/__init__.py`.
- Write release notes in `CHANGELOG.md`.
- Push to `main` and create a GitHub **Release** with the built artifacts.
