#!/usr/bin/env python3
"""
离线 mock ASR（容器内，由 02_toggle_happy_path.py 拉起）：原生 DashScope HTTP 协议。
仅接受 POST --path（默认 /api/v1/services/aigc/multimodal-generation/generation），
逐项校验 Authorization == Bearer $MOCK_EXPECTED_KEY、X-DashScope-SSE: disable、
model、parameters（format=wav、sample_rate=16000）与 WAV 头（16kHz/mono/16bit PCM）
且音频非空非静音；通过返回固定 --text（{text, output:{text}}），否则回协议错误；
每请求记一行 JSONL 到 --requests-log（E2E 断言 ok=false 记录为 0），不记录 key 本身。
"""
import argparse
import base64
import json
import os
import struct
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

DEFAULT_PATH = "/api/v1/services/aigc/multimodal-generation/generation"
MIN_BYTES = 8000   # 16kHz 16bit mono ≈ 0.25s
PEAK_MIN = 300     # int16 峰值下限（测试 WAV 实测 peak≈9700）
ARGS = KEY = None
_lock = threading.Lock()


def log(m):
    print(f"[mock_asr] {m}", flush=True)


def record(checks, n, peak, ok, reason):
    rec = {"ts": round(time.time(), 3), **checks, "audio_bytes": n,
           "peak": peak, "ok": ok, "reason": reason}
    with _lock, open(ARGS.requests_log, "a", encoding="utf-8") as f:
        f.write(json.dumps(rec, ensure_ascii=False) + "\n")


def check_wav(wav: bytes):
    """校验 RIFF/WAVE fmt = PCM/16kHz/单声道/16bit；返回 (错误信息, data chunk PCM)。"""
    if wav[:4] != b"RIFF" or wav[8:12] != b"WAVE":
        return "非 RIFF/WAVE", b""
    fmt, pcm, off = None, b"", 12
    while off + 8 <= len(wav):
        cid, size = wav[off:off + 4], struct.unpack_from("<I", wav, off + 4)[0]
        if cid == b"fmt " and size >= 16:
            fmt = struct.unpack_from("<HHIIHH", wav, off + 8)
        elif cid == b"data":
            pcm = wav[off + 8:off + 8 + size]
        off += 8 + size + (size & 1)
    if fmt is None:
        return "缺 fmt chunk", b""
    audio_format, channels, rate, _, _, bits = fmt
    if audio_format != 1:
        return f"非 PCM 编码 (format={audio_format})", b""
    if rate != 16000:
        return f"采样率非 16kHz (rate={rate})", b""
    if channels != 1:
        return f"非单声道 (channels={channels})", b""
    if bits != 16:
        return f"非 16bit (bits={bits})", b""
    return "", pcm


def pcm_peak(pcm: bytes) -> int:
    return max(map(abs, struct.unpack_from(f"<{len(pcm) // 2}h", pcm)), default=0)


class Handler(BaseHTTPRequestHandler):
    def _json(self, code, obj):
        b = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def do_POST(self):
        checks = {"path_ok": self.path == ARGS.path,
                  "auth_ok": self.headers.get("Authorization", "") == f"Bearer {KEY}",
                  "sse_ok": self.headers.get("X-DashScope-SSE") == "disable",
                  "model_ok": False, "params_ok": False, "wav_ok": False}
        if not checks["path_ok"]:
            record(checks, 0, 0, False, f"路径错误: {self.path}")
            return self._json(404, {"error": {"message": "not found"}})
        n = peak = 0
        why = ""
        try:
            body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
            checks["model_ok"] = body.get("model") == ARGS.model
            params = body.get("parameters") or {}
            checks["params_ok"] = (params.get("format") == "wav"
                                   and str(params.get("sample_rate")) == "16000")
            data = next(c["input_audio"]["data"]
                        for m in body["input"]["messages"]
                        for c in m.get("content", [])
                        if isinstance(c, dict) and c.get("type") == "input_audio")
            why, pcm = check_wav(base64.b64decode(data.split("base64,", 1)[1]))
            checks["wav_ok"], n, peak = not why, len(pcm), pcm_peak(pcm)
        except Exception as e:
            record(checks, 0, 0, False, f"请求/音频解析失败: {e}")
            return self._json(400, {"error": {"message": "bad request"}})
        if not why and not all(checks.values()):
            why = "校验失败: " + ",".join(k for k, v in checks.items() if not v)
        if not why and (n < MIN_BYTES or peak < PEAK_MIN):
            why = f"音频无效/静音 (bytes={n}, peak={peak})"
        if why:
            record(checks, n, peak, False, why)
            return self._json(401 if not checks["auth_ok"] else 400,
                              {"error": {"message": why}})
        record(checks, n, peak, True, "ok")
        self._json(200, {"text": ARGS.text, "output": {"text": ARGS.text}})


def main():
    global ARGS, KEY
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, required=True)
    ap.add_argument("--path", default=DEFAULT_PATH)
    ap.add_argument("--model", required=True)
    ap.add_argument("--text", required=True)
    ap.add_argument("--requests-log", required=True)
    ARGS = ap.parse_args()
    if not (KEY := os.environ.get("MOCK_EXPECTED_KEY", "")):
        log("!! 缺 MOCK_EXPECTED_KEY（离线也必须校验鉴权）")
        return 2
    Path(ARGS.requests_log).write_text("", encoding="utf-8")
    log(f"http://127.0.0.1:{ARGS.port}{ARGS.path} 就绪")
    ThreadingHTTPServer(("127.0.0.1", ARGS.port), Handler).serve_forever()


if __name__ == "__main__":
    sys.exit(main())
