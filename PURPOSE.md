# Purpose — audiblez

## What audiblez is

audiblez turns **e-books into audiobooks**. You feed it a book, it extracts the
text, runs it through a high-quality text-to-speech model ([Kokoro-82M](https://huggingface.co/hexgrad/Kokoro-82M)),
and produces a single `.m4b` audiobook file with cover art and chapter markers —
ready to load into any audiobook player or on any device.

Original work by **Claudio Santini** (MIT licensed). This fork extends it toward a
**distributed, GPU-first** workflow with a richer set of input/output formats and
full control from the desktop GUI.

## The core idea

```
book (epub / fb2 / audio)  ->  extract text + chapters  ->  TTS (Kokoro)  ->  encode (+ tags)  ->  audiobook
```

Everything except the **TTS step** is lightweight:
- parsing the book,
- finding/splitting chapters,
- splitting text into sentences,
- assembling the final `.m4b` with ffmpeg.

Only the **text-to-speech inference** is expensive, and only it needs a GPU.
That is the seam this fork exploits.

## Why this fork exists

The upstream app is single-machine and EPUB-only, with a fixed ffmpeg chain and
no user controls. This fork's purpose is to make audiblez:

1. **Fast** — run synthesis on a powerful remote GPU (named `spark01`, `spark02`)
   while the **GUI stays on the local Windows machine**. No need to own a GPU.
2. **Flexible on input** — EPUB, FictionBook (`.fb2`), and audio files (`.mp3`/`.aac`/`.wav`).
3. **Flexible on output** — choose container (`m4b`/`mp3`/`wav`), encode mode
   (**CBR**/**VBR**), bitrate, VBR quality, cover art (**PIC/APIC**), and ID3/ID3v2 tags.
4. **Configurable on compute** — pick the **model path** and the **device/CUDA**
   from the interface.

## Guiding principle

**Split at the TTS boundary.** The GUI, ffmpeg encoding, and file parsing stay
local on Windows. Only `KPipeline(...)` becomes pluggable — either a local
backend (CPU/CUDA) or a **remote backend** that talks HTTP to a GPU server.
This keeps the desktop app simple and portable while letting compute scale
where it matters.

## Repository layout

```
audiblez/            package
  core.py            engine: parse -> extract -> TTS -> encode
  cli.py             command-line entry point
  ui.py              wxPython desktop GUI
  tts_backend.py     pluggable backends: local vs remote GPU server
  ...
remote_tts_server/   standalone GPU server (FastAPI + Kokoro) for spark01 / spark02
tests/               tests
PURPOSE.md           this file
DEVELOPMENT_PLAN.md  roadmap
CHANGELOG.md         release notes
```

## License

MIT. Original author: Claudio Santini. See [LICENSE](./LICENSE).
