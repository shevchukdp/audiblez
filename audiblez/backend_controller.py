#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Local HTTP backend controller for audiblez.

Runs the existing audiblez.core engine (parse -> extract chapters -> TTS ->
ffmpeg encode) in a background thread and exposes it over HTTP as JSON, so a
native Rust/egui GUI (or any other client) can drive synthesis without a local
GUI toolkit.

This is the *same* backend used both locally (native GUI on the user's Windows
PC) and remotely (a controller running on a spark01/spark02 GPU box, reached
over the LAN/WAN). The egui GUI just needs the server URL in both cases.

Usage:
    python -m audiblez.backend_controller --port 8765
"""
import argparse
import json
import sys
import tempfile
import threading
import traceback
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

# The engine prints book text (titles may contain CJK/em-dashes/etc). On Windows
# a redirected stdout defaults to the ANSI code page (cp1251/cp1252) and such
# prints raise UnicodeEncodeError, killing the job. Force UTF-8 with a safe
# fallback so logging can never break synthesis.
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, ValueError):
        pass

import audiblez.core as core


class CoreState:
    """Owns one running synthesis job and its progress, guarded by a lock."""

    def __init__(self):
        self.lock = threading.Lock()
        self.status = "idle"          # idle | running | finished | error
        self.error = ""
        self.stats = None             # dict {progress, eta, chars_per_sec, ...}
        self.active_chapter = -1
        self.chapters_total = 0

    def reset(self):
        with self.lock:
            self.status = "idle"
            self.error = ""
            self.stats = None
            self.active_chapter = -1
            self.chapters_total = 0

    def report(self, event, **kwargs):
        with self.lock:
            if event == "CORE_STARTED":
                self.status = "running"
                self.chapters_total = kwargs.get("chapters_total", 0)
            elif event == "CORE_PROGRESS":
                st = kwargs.get("stats")
                # core hands us a SimpleNamespace; json.dumps cannot serialize
                # it, which used to crash /status (HTTP 500 / dropped
                # connection) while a job was running.
                if st is not None and not isinstance(st, dict):
                    st = vars(st) if hasattr(st, "__dict__") else {}
                self.stats = st or {}
            elif event == "CORE_CHAPTER_STARTED":
                self.active_chapter = kwargs.get("chapter_index", -1)
            elif event == "CORE_CHAPTER_FINISHED":
                self.active_chapter = kwargs.get("chapter_index", -1)
            elif event == "CORE_FINISHED":
                self.status = "finished"
            elif event == "CORE_ERROR":
                self.status = "error"
                self.error = kwargs.get("error", str(event))


STATE = CoreState()

# Where uploaded books and "auto" job outputs live on the server. Full-server
# mode: the (thin) GUI client uploads the .epub here, the server does ALL
# processing (TTS + ffmpeg) and the client downloads results via /download.
UPLOAD_ROOT = Path(tempfile.gettempdir()) / "audiblez_uploads"
MAX_UPLOAD = 200 * 1024 * 1024  # 200 MB books are plenty


def _safe_folder(folder):
    """Resolve a requested output folder, refusing anything outside
    UPLOAD_ROOT — a LAN client must not be able to read arbitrary server
    files via /outputs or /download."""
    d = (UPLOAD_ROOT / "out") if (not folder or folder.lower() == "auto") else Path(folder)
    try:
        d = d.resolve()
        root = UPLOAD_ROOT.resolve()
    except OSError:
        return None
    if d != root and not d.is_relative_to(root):
        return None
    return d

# kwargs that audiblez.core.main actually accepts. Any extra keys the GUI sends
# (device, model_path, format_out, bitrate, quality, ...) are future GPU/TTS
# configuration; they are logged and dropped for now so the engine call can't
# fail with "unexpected keyword argument".
CORE_KWARGS = {
    "file_path",
    "voice",
    "pick_manually",
    "speed",
    "output_folder",
    "max_chapters",
    "max_sentences",
    "selected_chapters",
    "tts_url",
}


def run_job(params):
    """Run core.main in a background thread, reporting progress to STATE."""
    from audiblez.core import main

    def post_event(event, **kwargs):
        STATE.report(event, **kwargs)

    try:
        params = dict(params)
        params.pop("_meta", None)
        # core.main() declares pick_manually as a required positional arg, but the
        # native GUI never sends it (chapter selection is non-interactive there).
        params.setdefault("pick_manually", False)
        core_kwargs = {k: v for k, v in params.items() if k in CORE_KWARGS}
        # "auto"/missing output folder = server-side uploads/out dir, so a thin
        # remote client never has to name a path that exists only on its own PC.
        of = str(core_kwargs.get("output_folder") or "").strip()
        if not of or of.lower() == "auto":
            core_kwargs["output_folder"] = str(UPLOAD_ROOT / "out")
            Path(core_kwargs["output_folder"]).mkdir(parents=True, exist_ok=True)
        extras = {k: v for k, v in params.items() if k not in CORE_KWARGS}
        if extras:
            print(f"[audiblez] ignoring unrecognized params: {sorted(extras)}")
        main(post_event=post_event, **core_kwargs)
    except Exception as e:  # noqa: BLE001
        traceback.print_exc()
        STATE.report("CORE_ERROR", error=str(e))


def make_handler():
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass  # silence the noisy default access log

        def _send(self, code, obj):
            body = json.dumps(obj).encode("utf-8")
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            path = self.path.split("?")[0]
            qs = urllib.parse.parse_qs(self.path.split("?", 1)[1] if "?" in self.path else "")

            if path == "/status":
                with STATE.lock:
                    self._send(200, {
                        "status": STATE.status,
                        "error": STATE.error,
                        "stats": STATE.stats,
                        "chapter": STATE.active_chapter,
                        "chapters_total": STATE.chapters_total,
                    })
                return

            if path == "/outputs":
                d = _safe_folder((qs.get("folder") or [""])[0].strip())
                if d is None:
                    self._send(403, {"error": "forbidden"})
                    return
                if not d.is_dir():
                    self._send(404, {"error": f"no such folder: {d}"})
                    return
                files = [{"name": p.name, "size": p.stat().st_size}
                         for p in sorted(d.iterdir()) if p.is_file()]
                self._send(200, {"folder": str(d), "files": files})
                return

            if path == "/download":
                d = _safe_folder((qs.get("folder") or [""])[0].strip())
                name = Path((qs.get("name") or [""])[0]).name  # no directories
                if d is None or not name:
                    self._send(403, {"error": "forbidden"})
                    return
                target = (d / name).resolve()
                if not target.is_relative_to(d) or not target.is_file():
                    self._send(404, {"error": f"no such file: {name}"})
                    return
                data = target.read_bytes()
                self.send_response(200)
                self.send_header("Content-Type", "application/octet-stream")
                self.send_header("Content-Length", str(len(data)))
                self.send_header("Content-Disposition",
                                 f'attachment; filename="{name}"')
                self.end_headers()
                self.wfile.write(data)
                return

            self._send(404, {"error": "not found"})

        def do_POST(self):
            length = int(self.headers.get("Content-Length", "0"))
            path = self.path.split("?")[0]

            if path == "/upload":
                # Raw body upload: POST /upload?name=book.epub
                if length <= 0 or length > MAX_UPLOAD:
                    self._send(400, {"error": f"bad size {length}"})
                    return
                qs = urllib.parse.parse_qs(
                    self.path.split("?", 1)[1] if "?" in self.path else "")
                name = Path((qs.get("name") or ["book.epub"])[0]).name
                if not name or name in (".", ".."):
                    self._send(400, {"error": "bad name"})
                    return
                target = UPLOAD_ROOT / name
                UPLOAD_ROOT.mkdir(parents=True, exist_ok=True)
                remaining = length
                with open(target, "wb") as f:
                    while remaining > 0:
                        chunk = self.rfile.read(min(1 << 16, remaining))
                        if not chunk:
                            break
                        f.write(chunk)
                        remaining -= len(chunk)
                self._send(200, {"path": str(target), "name": name,
                                 "size": length})
                return

            raw = self.rfile.read(length) if length else b"{}"
            try:
                params = json.loads(raw or b"{}")
            except json.JSONDecodeError:
                self._send(400, {"error": "bad json"})
                return

            if path == "/stop":
                # best-effort stop: the running job checks status each chapter
                with STATE.lock:
                    STATE.reset()
                self._send(200, {"ok": True})
                return

            if path == "/run":
                if STATE.status == "running":
                    self._send(409, {"error": "already running"})
                    return
                STATE.reset()
                threading.Thread(target=run_job, kwargs={"params": params},
                                 daemon=True).start()
                self._send(200, {"ok": True})
                return

            self._send(404, {"error": "unknown endpoint"})

    return Handler


def main():
    parser = argparse.ArgumentParser(description="audiblez backend controller")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--host", default="127.0.0.1")
    args = parser.parse_args()

    httpd = ThreadingHTTPServer((args.host, args.port), make_handler())
    print(f"audiblez backend controller on http://{args.host}:{args.port}")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("shutting down")


if __name__ == "__main__":
    main()
