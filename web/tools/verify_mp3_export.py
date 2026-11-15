"""Exercise the local MP3 worker, then independently decode and measure its files.

Uses existing Playwright, NumPy and local FFmpeg/ffprobe; never uploads audio.
Add --integration --site <built-site> to force real codec overshoot through the
master/Studio verification helpers, with original-source and independent peak checks.
"""
import argparse
import base64
import functools
import hashlib
import http.server
import json
from pathlib import Path
import re
import struct
import subprocess
import threading
from urllib.parse import urlparse

import numpy as np
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]


def float_wav(samples, rate, channels, extra=b"", extensible=False):
    fmt = struct.pack("<HHIIHH", 65534 if extensible else 3, channels,
                      rate, rate * channels * 4, channels * 4, 32)
    if extensible:
        fmt += struct.pack("<HHI", 22, 32, 0) + bytes.fromhex("0300000000001000800000aa00389b71")
    data = np.asarray(samples, dtype="<f4").tobytes()
    body = b"WAVEfmt " + struct.pack("<I", len(fmt)) + fmt + extra + b"data" + struct.pack("<I", len(data)) + data
    return b"RIFF" + struct.pack("<I", len(body)) + body


def make_fixtures(folder):
    references = {}
    for rate in [32000, 44100, 48000]:
        time = np.arange(rate * 3) / rate
        for channels in [1, 2]:
            source = np.stack([0.24 * np.sin(2 * np.pi * frequency * time)
                               + 0.06 * np.sin(2 * np.pi * (frequency * 2.13) * time)
                               for frequency in [997, 733][:channels]], axis=1).astype("<f4")
            # Soft ends expose delay without introducing lossy edge transients.
            fade = np.minimum(1, np.minimum(time / .03, (3 - time) / .03))
            source *= fade[:, None]
            name = f"{rate}-{channels}"
            references[name] = source
            (folder / (name + ".wav")).write_bytes(float_wav(source, rate, channels))
    source = references["48000-2"]
    normal = float_wav(source, 48000, 2)
    (folder / "unaligned.wav").write_bytes(float_wav(source, 48000, 2, extra=b"JUNK\x02\x00\x00\x00xx"))
    (folder / "extensible.wav").write_bytes(float_wav(source, 48000, 2, extensible=True))
    (folder / "truncated.wav").write_bytes(normal[:-1])
    integer = bytearray(normal); struct.pack_into("<H", integer, 20, 1)
    (folder / "integer.wav").write_bytes(integer)
    (folder / "partial-frame.wav").write_bytes(float_wav(np.zeros(3), 48000, 2))
    duplicate = bytearray(normal + b"data\x00\x00\x00\x00"); struct.pack_into("<I", duplicate, 4, len(duplicate) - 8)
    (folder / "duplicate-data.wav").write_bytes(duplicate)
    overlong = bytearray(normal); struct.pack_into("<I", overlong, 40, 0x7FFFFFF0)
    (folder / "overlong.wav").write_bytes(overlong)
    return references


def verify_provenance(site):
    folder = ROOT / "web/site/audio/encoders"
    manifest = json.loads((folder / "provenance.json").read_text())
    for name, expected in manifest["files"].items():
        assert hashlib.sha256((folder / name).read_bytes()).hexdigest() == expected, name
    verified, missing = 0, []
    for name, expected in manifest["files"].items():
        asset = site / "audio/encoders" / name
        if asset.is_file():
            assert hashlib.sha256(asset.read_bytes()).hexdigest() == expected, "Site provenance mismatch: " + name
            verified += 1
        else:
            missing.append(name)
    assert "lame-3.100-wme-0.7.0.wasm" not in missing, "Site is missing the pinned MP3 encoder"
    return {"package": manifest["package"], "version": manifest["version"],
            "source_files_hash_checked": len(manifest["files"]), "site_files_hash_checked": verified,
            "site_missing_provenance_files": missing, "lame_commit": manifest["lameSourceCommit"]}


def boundary_fixture(folder):
    # Stream the 345.6 MB WAV so fixture creation does not require a full-song array.
    frames, rate, channels = 900 * 48000, 48000, 2
    header = bytearray(float_wav(np.zeros(0), rate, channels))
    struct.pack_into("<I", header, 4, frames * channels * 4 + len(header) - 8)
    struct.pack_into("<I", header, 40, frames * channels * 4)
    block = np.zeros((48000, 2), dtype="<f4").tobytes()
    with (folder / "boundary.wav").open("wb") as writer:
        writer.write(header)
        for unused in range(900):
            writer.write(block)


def boundary_roundtrip(out, case):
    path = out / "boundary.mp3"
    path.write_bytes(base64.b64decode(case.pop("bytes")))
    stream = json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_streams", "-of", "json", str(path)]))["streams"][0]
    assert stream["codec_name"] == "mp3" and int(stream["sample_rate"]) == 48000 and stream["channels"] == 2
    process = subprocess.Popen(["ffmpeg", "-v", "error", "-nostdin", "-i", str(path),
                                "-f", "f32le", "-"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    total, peak = 0, 0.
    while block := process.stdout.read(1024 * 1024):
        total += len(block)
        samples = np.frombuffer(block, dtype="<f4")
        assert np.isfinite(samples).all()
        peak = max(peak, float(np.abs(samples).max()))
    error = process.stderr.read().decode()
    assert process.wait() == 0, error
    frames = total // 8
    assert case["frames"] < frames <= case["frames"] + 3000
    assert peak == 0
    case.update(ffmpeg_frames=frames, ffmpeg_padding_frames=frames - case["frames"], peak=peak)
    return case


def roundtrip(out, case, references):
    path = out / (case["name"] + ".mp3")
    if "bytes" in case:
        path.write_bytes(base64.b64decode(case.pop("bytes")))
    metadata = json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_streams", "-of", "json", str(path)]))
    stream = metadata["streams"][0]
    assert stream["codec_name"] == "mp3" and int(stream["sample_rate"]) == case["rate"]
    assert stream["channels"] == case["channels"]
    pcm = np.frombuffer(subprocess.check_output([
        "ffmpeg", "-v", "error", "-nostdin", "-i", str(path), "-f", "f32le", "-"]), dtype="<f4").reshape(-1, case["channels"])
    assert np.isfinite(pcm).all()
    padding = len(pcm) - case["frames"]
    assert 0 <= padding <= 3000, (case["name"], "unexpected codec padding", padding)
    case.update(decoded_frames=len(pcm), padding_frames=padding,
                peak=float(np.abs(pcm).max()), ffprobe_bitrate=stream.get("bit_rate"))
    reference = references.get(case["name"], references["48000-2"] if case["name"] == "attenuated" or case["name"].startswith("bitrate-") else None)
    if reference is not None:
        if case["name"] == "attenuated":
            reference = reference * 10 ** (-6 / 20)
        # Search actual decoded alignment; no assumed encoder priming constant.
        probe = reference[2000:12000, 0].astype(np.float64)
        lags = range(1800)
        errors = [np.square(pcm[2000 + lag:12000 + lag, 0] - probe).mean() for lag in lags]
        lag = int(np.argmin(errors))
        actual = pcm[lag:lag + len(reference)].astype(np.float64)
        snr = 10 * np.log10(np.square(reference).mean(axis=0) /
                            np.maximum(1e-30, np.square(actual - reference).mean(axis=0)))
        assert np.all(snr > 25), (case["name"], snr)
        case.update(alignment_delay_frames=lag, snr_db=snr.tolist())
        if case["channels"] == 2:
            assert np.corrcoef(actual.T)[0, 1] < .1, "Stereo channels collapsed"
    if case["name"] in ["silence", "one-frame"]:
        assert case["peak"] == 0
    return case


def independent_peak(path):
    # Independent reconstruction cross-check, not a standards conformance claim.
    measured = subprocess.run([
        "ffmpeg", "-hide_banner", "-nostats", "-nostdin", "-i", str(path),
        "-af", "aresample=384000,astats=metadata=1:reset=0", "-f", "null", "-",
    ], capture_output=True, text=True, check=True)
    peaks = [float(value) for value in re.findall(r"Peak level dB:\s+([-\d.]+)", measured.stderr)]
    assert peaks and all(np.isfinite(peaks)), measured.stderr
    return max(peaks)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--browser", choices=["chromium", "firefox", "webkit"], default="chromium")
    parser.add_argument("--executable")
    parser.add_argument("--boundary", action="store_true", help="Also encode/decode an exact 15-minute 48 kHz stereo file")
    parser.add_argument("--integration", action="store_true", help="Test real verified MP3 safety retries; --site must contain built Rust WASM")
    parser.add_argument("--site", type=Path, default=ROOT / "web/site")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve(); out.mkdir(parents=True, exist_ok=True)
    fixture_folder = out / "fixtures"; fixture_folder.mkdir(exist_ok=True)
    references = make_fixtures(fixture_folder)
    if args.boundary:
        boundary_fixture(fixture_folder)
    result = {"errors": [], "requests": [], "site": str(args.site.resolve()),
              "provenance": verify_provenance(args.site.resolve())}
    if args.integration:
        result["integration_asset_sha256"] = {
            name: hashlib.sha256((args.site / name).read_bytes()).hexdigest()
            for name in ["audio/verified-mp3-export.js", "audio/saved-audio-analysis.js", "pkg/dsp_bg.wasm",
                         "worker.js", "studio-worker.js"]}

    class Handler(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *unused):
            pass

        def translate_path(self, path):
            name = urlparse(path).path
            if name == "/audit.js":
                return str(ROOT / "web/tools/fixtures/mp3-export-audit.js")
            if name == "/peak-audit.js":
                return str(ROOT / "web/tools/fixtures/mp3-export-peak-audit.js")
            if name.startswith("/fixtures/"):
                return str(fixture_folder / Path(name).name)
            return super().translate_path(path)

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Handler, directory=str(args.site.resolve())))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    base = f"http://127.0.0.1:{server.server_address[1]}/"
    try:
        with sync_playwright() as p:
            browser = getattr(p, args.browser).launch(headless=True, **({"executable_path": args.executable} if args.executable else {}))
            try:
                result["browser"] = args.browser + " " + browser.version
                page = browser.new_page()
                result["console_errors"] = []
                page.on("console", lambda message: result["console_errors"].append(message.text) if message.type == "error" else None)
                if args.integration:
                    page.add_init_script("""const NativeWorker=Worker;window.Worker=class extends NativeWorker {
                        constructor(...args){super(...args);this.addEventListener('error',event=>
                            console.error('MP3 probe worker error',String(args[0]),event.message,event.filename,event.lineno));}
                    };""")
                page.on("pageerror", lambda error: result["errors"].append(str(error)))
                page.on("request", lambda request: result["requests"].append({"method": request.method, "url": request.url}))
                # Avoid executing the central app or requiring Rust WASM for this focused module test.
                page.route(base + "probe", lambda route: route.fulfill(body="<!doctype html><title>Local MP3 probe</title>", content_type="text/html"))
                page.goto(base + "probe")
                result.update(page.evaluate("async base => (await import(base+'audit.js')).auditMp3Export(base)", base))
                for case in result["cases"]:
                    (out / (case["name"] + ".mp3")).write_bytes(base64.b64decode(case.pop("bytes")))
                if args.boundary:
                    result["boundary"] = boundary_roundtrip(out, page.evaluate(
                        "async base => (await import(base+'audit.js')).auditMp3Boundary(base)", base))
                if args.integration:
                    result["peak_retry"] = page.evaluate(
                        "async base => (await import(base+'peak-audit.js')).auditMp3PeakRetries(base)", base)
                    retry = result["peak_retry"]
                    if retry["status"] == "passed":
                        source = out / "peak-safe-source.wav"
                        source.write_bytes(base64.b64decode(retry.pop("source_wav_bytes")))
                        first = out / "peak-first-pass.mp3"
                        first.write_bytes(base64.b64decode(retry.pop("first_pass_bytes")))
                        retry["ffmpeg_8x_source_dbtp"] = independent_peak(source)
                        retry["ffmpeg_8x_first_pass_dbtp"] = independent_peak(first)
                        assert retry["ffmpeg_8x_source_dbtp"] <= retry["ceiling_dbtp"]
                        assert retry["ffmpeg_8x_first_pass_dbtp"] > retry["ceiling_dbtp"] + .005
                        for case in retry["results"]:
                            artifact = out / ("peak-verified-" + case["name"] + ".mp3")
                            artifact.write_bytes(base64.b64decode(case.pop("bytes")))
                            case["ffmpeg_8x_peak_dbtp"] = independent_peak(artifact)
                            assert case["ffmpeg_8x_peak_dbtp"] <= retry["ceiling_dbtp"] + .005
                assert not result["errors"], result["errors"]
                assert all(request["method"] == "GET" and request["url"].startswith(base) for request in result["requests"]), "Nonlocal request or upload"
                result["cases"] = [roundtrip(out, case, references) for case in result["cases"]]
                assert all(int(case["ffprobe_bitrate"]) == case.get("bitrate", 320000 if case["name"] == "attenuated" else 192000) for case in result["cases"])
                for name, expected in result.get("integration_asset_sha256", {}).items():
                    assert hashlib.sha256((args.site / name).read_bytes()).hexdigest() == expected, "Site changed during verification: " + name
                result["status"] = "passed"
            finally:
                browser.close()
    except Exception as error:
        result["failure"] = str(error)
        raise
    finally:
        server.shutdown(); server.server_close()
        if args.boundary:
            (fixture_folder / "boundary.wav").unlink(missing_ok=True)
        (out / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: value for key, value in result.items() if key not in ["requests", "cases"]}, indent=2))
    print(f"Validated {len(result['cases'])} MP3 roundtrips: {out / 'result.json'}")


if __name__ == "__main__":
    main()
