#!/usr/bin/env python3
"""Structural smoke test for the sayvane-speech worker protocol.

Usage:
  python3 -I speech-worker/tests/smoke.py --worker <path-to-sayvane-speech> \
      --model <path-to-ggml-tiny.en.bin> [--fixtures tests/fixtures]

Uses only the standard library. Does not assert exact recognized text.
"""
import argparse
import json
import os
import queue
import subprocess
import sys
import tempfile
import threading
import time
import uuid
import wave

PV = 1
FAILURES = []


def check(cond, what):
    status = "ok  " if cond else "FAIL"
    print(f"  [{status}] {what}")
    if not cond:
        FAILURES.append(what)
    return cond


class Worker:
    def __init__(self, path):
        self.p = subprocess.Popen(
            [path], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE
        )
        self.q = queue.Queue()
        self.stderr = []
        threading.Thread(target=self._read_out, daemon=True).start()
        threading.Thread(target=self._read_err, daemon=True).start()

    def _read_out(self):
        for raw in self.p.stdout:
            line = raw.decode("utf-8")
            try:
                self.q.put(json.loads(line))
            except ValueError:
                self.q.put({"__invalid__": line})
        self.q.put(None)

    def _read_err(self):
        for raw in self.p.stderr:
            self.stderr.append(raw.decode("utf-8", "replace"))

    def send_raw(self, data: bytes):
        self.p.stdin.write(data)
        self.p.stdin.flush()

    def send(self, command, **fields):
        rid = fields.pop("requestId", None) or str(uuid.uuid4())
        msg = {"protocolVersion": PV, "requestId": rid, "command": command}
        msg.update(fields)
        self.send_raw((json.dumps(msg) + "\n").encode("utf-8"))
        return rid

    def recv(self, timeout=60):
        msg = self.q.get(timeout=timeout)
        if msg is not None and "__invalid__" in msg:
            check(False, f"stdout line is valid JSON: {msg['__invalid__'][:200]!r}")
        return msg

    def recv_until(self, pred, timeout=120):
        """Collect messages until pred(msg) is true. Returns (match, others)."""
        others = []
        deadline = time.time() + timeout
        while True:
            msg = self.recv(timeout=max(0.1, deadline - time.time()))
            if msg is None:
                return None, others
            if pred(msg):
                return msg, others
            others.append(msg)


def common(msg, rid, typ):
    return (
        isinstance(msg, dict)
        and msg.get("protocolVersion") == PV
        and msg.get("requestId") == rid
        and msg.get("type") == typ
    )


def make_wav(path, src, repeat=1, rate=None):
    with wave.open(src, "rb") as w:
        params = w.getparams()
        frames = w.readframes(w.getnframes())
    with wave.open(path, "wb") as o:
        o.setnchannels(params.nchannels)
        o.setsampwidth(params.sampwidth)
        o.setframerate(rate or params.framerate)
        o.writeframes(frames * repeat)


def check_result_shape(msg, rid, job_id, gen):
    ok = common(msg, rid, "result") and msg.get("ok") is True
    check(ok, "result envelope (protocolVersion, requestId, type, ok)")
    check(msg.get("jobId") == job_id and msg.get("generation") == gen, "result echoes jobId and generation")
    r = msg.get("result") or {}
    check(isinstance(r.get("text"), str), "result.text is a string")
    check(r.get("language") is None or isinstance(r.get("language"), str), "result.language string or null")
    check(isinstance(r.get("segments"), list), "result.segments is a list")
    for s in r.get("segments", []):
        good = (
            isinstance(s.get("id"), int)
            and isinstance(s.get("start"), (int, float))
            and isinstance(s.get("end"), (int, float))
            and isinstance(s.get("text"), str)
            and (s.get("words") is None or isinstance(s.get("words"), list))
        )
        if not check(good, f"segment {s.get('id')} shape"):
            break
    check(isinstance(r.get("durationSeconds"), (int, float)), "result.durationSeconds is a number")
    check(r.get("backend") in ("cpu", "metal"), "result.backend is cpu or metal")
    return r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--worker", required=True)
    ap.add_argument("--model", required=True)
    here = os.path.dirname(os.path.abspath(__file__))
    ap.add_argument("--fixtures", default=os.path.join(here, "..", "..", "tests", "fixtures"))
    args = ap.parse_args()

    hello_wav = os.path.abspath(os.path.join(args.fixtures, "hello.wav"))
    silence_wav = os.path.abspath(os.path.join(args.fixtures, "silence.wav"))
    tmp = tempfile.mkdtemp(prefix="sayvane-smoke-")
    long_wav = os.path.join(tmp, "long.wav")
    make_wav(long_wav, hello_wav, repeat=8)
    longer_wav = os.path.join(tmp, "longer.wav")
    make_wav(longer_wav, hello_wav, repeat=24)  # ~90 s: several 30 s engine windows
    bad_rate_wav = os.path.join(tmp, "8k.wav")
    make_wav(bad_rate_wav, hello_wav, rate=8000)

    w = Worker(args.worker)

    print("hello")
    rid = w.send("hello")
    m = w.recv()
    check(common(m, rid, "hello") and m.get("ok") is True, "hello envelope")
    check(m.get("engineVersion") == "1.9.5", "hello.engineVersion == 1.9.5")
    check(m.get("cpuSupported") is True, "hello.cpuSupported")
    print("   ", json.dumps(m))

    print("capabilities")
    rid = w.send("capabilities")
    m = w.recv()
    check(common(m, rid, "capabilities"), "capabilities envelope")
    check("cpu" in m.get("backends", []), "capabilities.backends includes cpu")
    for k in ("arch", "engineVersion", "maxAudioSeconds", "supportsTranslate", "supportsWordTimestamps", "supportsBeamSearch"):
        check(k in m, f"capabilities.{k} present")
    check(m.get("loadedModel") is None, "no model loaded yet")
    print("   ", json.dumps(m))

    print("warm")
    rid = w.send("warm", modelPath=os.path.abspath(args.model), backend="auto")
    m = w.recv(timeout=120)
    check(common(m, rid, "warmed") and m.get("ok") is True, "warmed envelope")
    check(m.get("backend") in ("cpu", "metal"), "warmed.backend reported")
    check(m.get("multilingual") is False, "tiny.en reported as English-only")
    print("   ", json.dumps(m))

    rid = w.send("capabilities")
    m = w.recv()
    lm = m.get("loadedModel") or {}
    check(lm.get("multilingual") is False and lm.get("canTranslate") is False, "capabilities.loadedModel reflects tiny.en")

    print("transcribe hello.wav (word timestamps)")
    job, gen = str(uuid.uuid4()), 4
    t0 = time.time()
    rid = w.send("transcribe", jobId=job, generation=gen, modelPath=os.path.abspath(args.model),
                 audioPath=hello_wav, backend="auto",
                 options={"task": "transcribe", "language": None, "initialPrompt": "", "temperature": 0,
                          "decoding": "greedy", "wordTimestamps": True, "threads": 0})
    m, others = w.recv_until(lambda x: x is None or x.get("type") != "progress")
    elapsed = time.time() - t0
    progress = [o for o in others if o.get("type") == "progress"]
    check(all(p.get("jobId") == job and p.get("generation") == gen and 0 <= p.get("percent", -1) <= 100 for p in progress),
          f"progress messages well-formed ({len(progress)} received)")
    r = check_result_shape(m, rid, job, gen)
    text = r.get("text", "")
    check(len(text.strip()) > 0, "hello.wav text non-empty")
    check(any(word in text.lower() for word in ("hello", "world", "test", "dictation")),
          "hello.wav text contains an expected word")
    segs = r.get("segments", [])
    check(len(segs) > 0 and isinstance(segs[0].get("words"), list) and len(segs[0]["words"]) > 0,
          "word timestamps returned")
    if segs and segs[0].get("words"):
        wd = segs[0]["words"][0]
        check(all(k in wd for k in ("text", "start", "end", "p")), "word has text/start/end/p")
    check(abs(r.get("durationSeconds", 0) - 3.8) < 1.0, "durationSeconds about right")
    check(r.get("language") == "en", "language == en")
    print(f"    text={text!r} language={r.get('language')} backend={r.get('backend')} "
          f"duration={r.get('durationSeconds'):.2f}s elapsed={elapsed:.2f}s")

    print("transcribe silence.wav")
    job2 = str(uuid.uuid4())
    rid = w.send("transcribe", jobId=job2, generation=5, modelPath=os.path.abspath(args.model),
                 audioPath=silence_wav, options={})
    m, _ = w.recv_until(lambda x: x is None or x.get("type") != "progress")
    r = check_result_shape(m, rid, job2, 5)
    check(r.get("segments") is not None and all(s.get("words") is None for s in r.get("segments", [])),
          "words is null without wordTimestamps")
    check("[BLANK_AUDIO]" not in r.get("text", ""), "non-speech annotation filtered from text")
    print(f"    text={r.get('text')!r}")

    print("transcribe long audio, second request while busy, then cancel")
    job3 = str(uuid.uuid4())
    rid3 = w.send("transcribe", jobId=job3, generation=6, modelPath=os.path.abspath(args.model),
                  audioPath=long_wav, options={"decoding": "beam", "beamSize": 5})
    rid_busy = w.send("transcribe", jobId="other-job", generation=6, modelPath=os.path.abspath(args.model),
                      audioPath=hello_wav)
    rid_cancel = w.send("cancel", jobId=job3)
    t_cancel = time.time()
    got = {}
    while len(got) < 3:
        m = w.recv(timeout=60)
        if m is None:
            break
        if m.get("type") == "progress":
            continue
        got[m.get("requestId")] = m
    busy = got.get(rid_busy, {})
    check(busy.get("type") == "error" and busy.get("ok") is False and (busy.get("error") or {}).get("code") == "busy"
          and busy.get("jobId") == "other-job", "concurrent transcribe rejected with busy")
    ack = got.get(rid_cancel, {})
    check(common(ack, rid_cancel, "canceled") and ack.get("cancelRequested") is True and ack.get("jobId") == job3,
          "cancel acknowledged")
    jm = got.get(rid3, {})
    check(common(jm, rid3, "canceled") and jm.get("ok") is False and jm.get("jobId") == job3
          and jm.get("generation") == 6 and (jm.get("error") or {}).get("code") == "canceled",
          "canceled job replies type canceled")
    print(f"    cancel round trip {time.time() - t_cancel:.2f}s; job reply type={jm.get('type')}")

    print("cancel mid-recognition (after first progress > 0)")
    job4 = str(uuid.uuid4())
    rid4 = w.send("transcribe", jobId=job4, generation=7, modelPath=os.path.abspath(args.model),
                  audioPath=longer_wav, options={"decoding": "beam", "beamSize": 5})
    m, _ = w.recv_until(lambda x: x is None or (x.get("type") == "progress" and x.get("percent", 0) > 0)
                        or x.get("type") != "progress")
    saw_progress = m is not None and m.get("type") == "progress"
    check(saw_progress, f"progress > 0 observed before cancel ({m and m.get('percent')}%)")
    t_cancel = time.time()
    rid_cancel = w.send("cancel", jobId=job4)
    got = {}
    while len(got) < 2:
        m = w.recv(timeout=60)
        if m is None:
            break
        if m.get("type") == "progress":
            continue
        got[m.get("requestId")] = m
    jm = got.get(rid4, {})
    check(jm.get("type") == "canceled" and (jm.get("error") or {}).get("code") == "canceled",
          "mid-run cancel aborts the engine")
    check(got.get(rid_cancel, {}).get("cancelRequested") is True, "mid-run cancel acknowledged")
    print(f"    cancel latency {time.time() - t_cancel:.2f}s")

    print("translate with English-only model")
    rid = w.send("transcribe", jobId="tr", generation=7, modelPath=os.path.abspath(args.model),
                 audioPath=hello_wav, options={"task": "translate"})
    m, _ = w.recv_until(lambda x: x is None or x.get("type") != "progress")
    check(m.get("type") == "error" and (m.get("error") or {}).get("code") == "incompatible_model"
          and m.get("jobId") == "tr" and m.get("generation") == 7, "translate refused: incompatible_model")

    print("wrong sample rate")
    rid = w.send("transcribe", jobId="sr", generation=8, modelPath=os.path.abspath(args.model), audioPath=bad_rate_wav)
    m, _ = w.recv_until(lambda x: x is None or x.get("type") != "progress")
    check(m.get("type") == "error" and (m.get("error") or {}).get("code") == "audio_read_failed", "8 kHz audio: audio_read_failed")
    print("   ", (m.get("error") or {}).get("message"))

    print("missing audio file")
    rid = w.send("transcribe", jobId="nf", generation=8, modelPath=os.path.abspath(args.model), audioPath=os.path.join(tmp, "nope.wav"))
    m, _ = w.recv_until(lambda x: x is None or x.get("type") != "progress")
    check((m.get("error") or {}).get("code") == "audio_read_failed", "missing audio: audio_read_failed")

    print("missing model file")
    rid = w.send("warm", modelPath=os.path.join(tmp, "nope.bin"))
    m = w.recv()
    check(common(m, rid, "error") and (m.get("error") or {}).get("code") == "model_load_failed", "missing model: model_load_failed")

    print("validation errors")
    rid = w.send("transcribe", jobId="v", generation=9, modelPath=os.path.abspath(args.model), audioPath=hello_wav,
                 options={"bogusOption": 1})
    m = w.recv()
    check(common(m, rid, "error") and (m.get("error") or {}).get("code") == "bad_request" and m.get("jobId") == "v",
          "unknown option rejected with bad_request")
    rid = w.send("transcribe", jobId="v2", generation=-1, modelPath="x", audioPath="y")
    m = w.recv()
    check(common(m, rid, "error") and (m.get("error") or {}).get("code") == "bad_request", "negative generation rejected")
    rid = w.send("frobnicate")
    m = w.recv()
    check(common(m, rid, "error") and (m.get("error") or {}).get("code") == "bad_request", "unknown command rejected")
    w.send_raw(b'{"protocolVersion":2,"requestId":"pv","command":"hello"}\n')
    m = w.recv()
    check(m.get("type") == "error" and m.get("requestId") == "pv", "wrong protocolVersion rejected")

    print("malformed JSON line")
    w.send_raw(b'{"protocolVersion":1, this is not json\n')
    m = w.recv()
    check(m.get("type") == "error" and m.get("ok") is False and m.get("requestId") is None
          and (m.get("error") or {}).get("code") == "bad_request", "malformed JSON -> bad_request, requestId null")

    print("oversized line (> 1 MiB)")
    w.send_raw(b'{"x":"' + b"a" * (1024 * 1024 + 10) + b'"}\n')
    m = w.recv()
    check(m.get("type") == "error" and (m.get("error") or {}).get("code") == "bad_request", "oversized line rejected")
    rid = w.send("hello")
    m = w.recv()
    check(common(m, rid, "hello"), "worker still responsive after oversized line")

    print("unload")
    rid = w.send("unload")
    m = w.recv(timeout=30)
    check(common(m, rid, "unloaded"), "unloaded")

    print("shutdown")
    rid = w.send("shutdown")
    m = w.recv(timeout=30)
    check(common(m, rid, "bye"), "bye")
    try:
        code = w.p.wait(timeout=10)
    except subprocess.TimeoutExpired:
        code = None
        w.p.kill()
    check(code == 0, f"worker exited 0 after shutdown (got {code})")

    print("stdin EOF")
    w2 = Worker(args.worker)
    rid = w2.send("hello")
    w2.recv()
    w2.p.stdin.close()
    try:
        code = w2.p.wait(timeout=10)
    except subprocess.TimeoutExpired:
        code = None
        w2.p.kill()
    check(code == 0, f"worker exited 0 on stdin EOF (got {code})")

    if any(line.strip() for line in w.stderr):
        print("worker stderr (diagnostics):")
        for line in w.stderr[-10:]:
            print("    " + line.rstrip())

    print()
    if FAILURES:
        print(f"FAILED: {len(FAILURES)} check(s)")
        for f in FAILURES:
            print("  - " + f)
        return 1
    print("ALL CHECKS PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
