#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Remote CUDA TTS server for audiblez.

Runs on a GPU box (spark01 / spark02) and exposes a small FastAPI HTTP API.
The native egui GUI (and the local backend controller) talk to this over HTTP
so synthesis runs on the remote GPU while the desktop GUI stays on the user's
Windows PC.

Design notes:
  * The KPipeline (Kokoro-82M) is loaded ONCE at startup and reused across
    requests. Requests are queued so a single GPU never runs two at once.
  * Input/output are raw bytes over JSON: send text + voice + speed, get back
    interleaved 24kHz float32 audio (the Kokoro sample rate).
  * espeak-ng must be installed on the server (phonemization happens inside
    KPipeline).

Run:
    uvicorn remote_tts_server.server:app --host 0.0.0.0 --port 8000
    # or
    python -m remote_tts_server.server --port 8000
"""
import argparse
import io
import json
import threading
from contextlib import asynccontextmanager

import numpy as np
from fastapi import FastAPI, Body
from pydantic import BaseModel
from starlette.responses import Response

# Kokoro. Import lazily so `--help` works without torch installed.
from kokoro import KPipeline

# Queue of pending jobs; a single worker thread consumes it, which keeps one
# GPU from ever running two syntheses concurrently (Kokoro is not thread-safe).
_queue = []
_queue_lock = threading.Lock()
_queue_cv = threading.Condition(_queue_lock)


class SynthesisRequest(BaseModel):
    text: str
    voice: str = "af_sky"
    speed: float = 1.0
    language: str = "en-us"          # passed to KPipeline as lang_code


class SynthesisResponse(BaseModel):
    ok: bool = True
    sample_rate: int = 24000
    format: str = "float32"
    audio_b64: str = ""
    error: str = ""


# ---------------------------------------------------------------------------
# Pipeline management
# ---------------------------------------------------------------------------

_pipeline = None
_pipeline_lock = threading.Lock()


def get_pipeline(model_path: str | None, device: str, language: str = "a"):
    global _pipeline
    with _pipeline_lock:
        if _pipeline is None:
            if device in ("", "auto"):
                import torch
                device = "cuda" if torch.cuda.is_available() else "cpu"
            lang = (language or "a")[:1]  # voice-first-letter codes: a/b/e/...
            try:
                # kokoro >= 0.8: repo_id identifies the HF model
                _pipeline = KPipeline(lang_code=lang, repo_id=model_path or None,
                                      device=device)
            except TypeError:
                # kokoro 0.7.x: no repo_id kwarg (downloads are pre-cached)
                _pipeline = KPipeline(lang_code=lang, device=device)
            print(f"[remote_tts] pipeline loaded on {device} (lang={lang})", flush=True)
    return _pipeline


# ---------------------------------------------------------------------------
# Request queue worker
# ---------------------------------------------------------------------------

def _worker():
    while True:
        with _queue_cv:
            while not _queue:
                _queue_cv.wait()
            job = _queue.pop(0)

        try:
            req: SynthesisRequest = job["request"]
            # Prefer the voice's language code (a/b/e/...) like audiblez.core does.
            lang = (req.voice or "af_bella")[:1]
            pipeline = get_pipeline(job["model_path"], job["device"], lang)
            segments = []
            for _, _, audio in pipeline(req.text, voice=req.voice, speed=req.speed):
                segments.append(np.asarray(audio, dtype=np.float32))
            if segments:
                audio = np.concatenate(segments)
            else:
                audio = np.zeros(0, dtype=np.float32)
            job["result"] = ("ok", audio)
        except Exception as e:  # noqa: BLE001
            job["result"] = ("error", e)
        finally:
            job["done"].set()


# ---------------------------------------------------------------------------
# App
# ---------------------------------------------------------------------------

@asynccontextmanager
async def lifespan(app: FastAPI):
    t = threading.Thread(target=_worker, daemon=True)
    t.start()
    app.state.worker = t
    yield


app = FastAPI(title="audiblez remote TTS", version="1.0.0", lifespan=lifespan)


@app.post("/synthesize", response_model=SynthesisResponse)
def synthesize(req: SynthesisRequest) -> SynthesisResponse:
    done = threading.Event()
    job = {"request": req, "model_path": None, "device": "auto", "done": done}
    with _queue_cv:
        _queue.append(job)
        _queue_cv.notify()
    done.wait(timeout=600)  # generous timeout for long texts
    status, payload = job["result"]
    if status == "ok":
        import base64
        b64 = base64.b64encode(payload.tobytes()).decode("ascii")
        return SynthesisResponse(audio_b64=b64, sample_rate=24000)
    return SynthesisResponse(ok=False, audio_b64="", error=str(payload))


@app.get("/health")
def health():
    try:
        import torch
        dev = "cuda" if torch.cuda.is_available() else "cpu"
    except Exception:
        dev = "unknown"
    return {"ok": True, "device": dev, "pipeline_loaded": _pipeline is not None}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="audiblez remote TTS server")
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8000)
    args = parser.parse_args()
    import uvicorn
    uvicorn.run(app, host=args.host, port=args.port)
