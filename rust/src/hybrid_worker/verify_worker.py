"""Exercise the actual native process through a private loopback HTTP parent.

No third-party Python modules. Optional --long-seconds tests server duration
admission with 8 kHz mono PCM; all normal command fixtures are 16 kHz stereo.
"""
import argparse
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import math
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import threading
from urllib.error import HTTPError
from urllib.request import Request, urlopen
import wave
import zipfile


MESSAGES = {"Reading audio", "Processing audio", "Mastering audio", "Encoding audio",
            "Loading stems", "Mixing stems", "Separating stems", "Publishing result", "Complete"}


def generate(path, seconds=1, rate=16000, channels=2):
    with wave.open(str(path), "wb") as audio:
        audio.setnchannels(channels)
        audio.setsampwidth(2)
        audio.setframerate(rate)
        for offset in range(0, round(rate * seconds), rate):
            count = min(rate, round(rate * seconds) - offset)
            samples = [round(5000 * math.sin(2 * math.pi * 280 * (offset+i) / rate))
                       for i in range(count) for _ in range(channels)]
            audio.writeframesraw(struct.pack("<" + "h" * len(samples), *samples))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--long-seconds", type=int, default=0)
    parser.add_argument("--long-master", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    results = []
    with tempfile.TemporaryDirectory(prefix="hybrid native HTTP ") as temporary:
        root = Path(temporary).resolve()
        source = root / "source.wav"
        generate(source)
        original_hash = hashlib.sha256(source.read_bytes()).hexdigest()

        class Parent(BaseHTTPRequestHandler):
            def log_message(self, *unused):
                pass

            def do_POST(self):
                supplied = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                stage = root / ("command-" + str(len(list(root.glob("command-*")))))
                stage.mkdir()
                request = {"api_version": 2, "project_root": str(root), "input": str(source),
                           "out_dir": str(stage), "settings": {}, **supplied}
                if request["operation"] in ("mix", "import_stems") and "stems" in supplied:
                    request.pop("input", None)
                request_path, response_path = stage / "request.json", stage / "response.json"
                request_path.write_text(json.dumps(request), encoding="utf-8")
                process = subprocess.run([str(binary), "--request", str(request_path),
                                          "--response", str(response_path), "--threads", "2"],
                                         capture_output=True, text=True, timeout=300)
                for line in process.stdout.splitlines():
                    event = json.loads(line)
                    assert set(event) == {"progress", "message"}
                    assert type(event["progress"]) in (int, float) and 0 <= event["progress"] <= 1
                    assert event["message"] in MESSAGES
                if process.returncode:
                    assert not response_path.exists()
                    body = json.dumps({"error": process.stderr}).encode()
                    self.send_response(422)
                else:
                    response = json.loads(response_path.read_text(encoding="utf-8"))
                    assert response["api_version"] == 2 and response["operation"] == request["operation"]
                    for asset in response["assets"]:
                        path = Path(asset["path"]).resolve(strict=True)
                        assert path.is_relative_to(stage)
                        assert asset["role"] in {"original", "master", "preview", "stem"}
                        assert asset["frames"] > 0 and asset["channels"] in (1, 2)
                        assert len(asset["channel_peaks"]) == asset["channels"]
                        assert all(math.isfinite(v) and v >= 0 for v in asset["channel_peaks"])
                    envelope = response.get("waveform", {}).get("peaks", [])
                    assert len(envelope) <= 1024
                    if "waveform" in response:
                        assert len(response["waveform"]["channel_peaks"]) == response["audio"]["channels"]
                    assert all(len(pair) == 2 and all(type(v) in (int, float) and math.isfinite(v) for v in pair)
                               and pair[0] <= pair[1] for pair in envelope)
                    body = json.dumps(response).encode()
                    self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Parent)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        endpoint = "http://127.0.0.1:" + str(server.server_port)

        def call(operation, expected=200, **options):
            payload = json.dumps({"operation": operation, **options}).encode()
            request = Request(endpoint, data=payload, headers={"Content-Type": "application/json"})
            try:
                with urlopen(request, timeout=310) as response:
                    status, body = response.status, json.load(response)
            except HTTPError as response:
                status, body = response.code, json.load(response)
            assert status == expected, (operation, status, body)
            results.append({"operation": operation, "status": status})
            return body

        try:
            imported = call("import")
            analyzed = call("analyze")
            assert analyzed["audio"]["frames"] == 16000
            mastered = call("master", settings={"safety": False, "intensity": 0},
                            reference_analysis=imported["analysis"])
            assert len(call("suggest")["suggestion"]["bands"]) == 7
            for format_name in ("wav16", "wav24", "wav32f", "flac16", "flac24", "mp3"):
                rendered = call("render", input=mastered["audio"]["path"],
                                settings={"formats": [format_name], "eq_hold_loudness": False, "eq_output_db": -3})
                assert rendered["assets"][0]["role"] == "preview"
                assert rendered["assets"][1]["format"] == format_name
                if shutil.which("ffprobe"):
                    probe = json.loads(subprocess.check_output(["ffprobe", "-v", "error", "-select_streams", "a:0",
                        "-show_entries", "stream=sample_rate,channels", "-of", "json", rendered["assets"][1]["path"]]))
                    assert probe["streams"][0]["sample_rate"] == "16000"
                    assert probe["streams"][0]["channels"] == 2
            call("issues")
            call("repair", repair={"action": "reduce_low_mids", "start_seconds": .2,
                 "end_seconds": .6, "strength": .5})
            call("voice_cleanup", voice={"noise_reduction": 0, "deess": 0})
            call("spectral_repair", spectral={"start_seconds": .2, "end_seconds": .6,
                 "low_hz": 200, "high_hz": 1000, "reduction_db": -6, "mode": "attenuate"})
            call("spectrogram", spectrogram={"columns": 32, "rows": 16})
            call("region_edit", region={"start_seconds": .2, "end_seconds": .6,
                 "operation": {"kind": "reverse"}})
            stems = [{"path": str(source), "kind": "vocals"}, {"path": str(source), "kind": "drums"}]
            call("mix", stems=stems)
            imported_bank = call("import_stems", stems=stems)
            assert len(imported_bank["assets"]) == 4
            original = next(a for a in imported_bank["assets"] if a["role"] == "original")
            preview = next(a for a in imported_bank["assets"] if a["role"] == "preview")
            assert imported_bank["audio"]["path"] == preview["path"]
            assert original["channel_peaks"][0] > imported["assets"][0]["channel_peaks"][0] * 1.99
            call("render", input=original["path"], settings={"formats":["wav32f"]})
            archive = root / "Suno stems.zip"
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
                bundle.write(source, "Song (Vocals).wav")
                bundle.write(source, "Song (Drums).wav")
            assert len(call("import_stems", input=str(archive))["assets"]) == 4
            loose = root / "Song (Drums).wav"
            shutil.copyfile(source, loose)
            assert len(call("import_stems", input=None, inputs=[str(source), str(loose)])["assets"]) == 4
            assert len(call("import_stems", input=None, inputs=[
                {"path": str(source), "name": "Song (Vocals).wav", "kind": "vocals"},
                {"path": str(loose), "name": "Song (Drums).wav", "kind": "drums"}])["assets"]) == 4
            display = call("spectrogram")
            assert len(json.dumps(display).encode()) < 1024 * 1024
            call("analyze", expected=422, settings={"unexpected": True})
            call("repair", expected=422, repair={"action": "declip", "start_seconds": 0,
                 "end_seconds": 3, "strength": 1})
            call("analyze", expected=422, limits={"max_pcm_bytes": 16})
            with tempfile.TemporaryDirectory() as outside:
                external = Path(outside) / "source.wav"
                shutil.copyfile(source, external)
                call("analyze", expected=422, input=str(external))
            if args.long_seconds:
                long_source = root / "long server source.wav"
                generate(long_source, args.long_seconds, rate=8000, channels=1)
                long_import = call("import", input=str(long_source))
                assert long_import["audio"]["frames"] == args.long_seconds * 8000
                call("analyze", expected=422, input=str(long_source),
                     limits={"max_duration_seconds": args.long_seconds - .01})
                call("analyze", input=str(long_source), limits={"max_duration_seconds": args.long_seconds})
                if args.long_master:
                    long_master = call("master", input=str(long_source), settings={"safety": False, "intensity": 0})
                    assert long_master["audio"]["frames"] == args.long_seconds * 8000
                    assert long_master["audio"]["sample_rate"] == 8000
            assert hashlib.sha256(source.read_bytes()).hexdigest() == original_hash
        finally:
            server.shutdown()
            thread.join(timeout=10)
            server.server_close()
    print(json.dumps({"binary": str(binary), "checks": results, "passed": len(results),
                      "source_unchanged": True, "long_seconds": args.long_seconds}))


if __name__ == "__main__":
    main()
