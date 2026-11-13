"""End-to-end export checks using an independent FFmpeg meter (Python stdlib only)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def command(args, expected=0):
    result = subprocess.run([str(x) for x in args], capture_output=True, text=True, encoding="utf-8", errors="replace")
    if result.returncode != expected:
        raise AssertionError(f"Exit {result.returncode}, expected {expected}: {args}\n{result.stdout}\n{result.stderr}")
    return result


def verify(binary, source, output):
    original = hashlib.sha256(source.read_bytes()).hexdigest()
    command([binary, source, "--format", "wav24,wav16,wav32f,flac24,flac16,mp3", "--sr", "44100", "--out", output])
    reports = sorted(output.glob("*.report.json"))
    assert len(reports) == 1
    report = json.loads(reports[0].read_text())
    results = []
    for item in report["exports"]:
        path = Path(item["path"])
        measured = command(["ffmpeg", "-hide_banner", "-nostdin", "-i", path, "-af", "ebur128=peak=true", "-f", "null", "-"])
        summary = measured.stderr.rsplit("Summary:", 1)[1]
        lufs = float(re.search(r"\bI:\s+([-\d.]+) LUFS", summary)[1])
        peak = float(re.search(r"\bPeak:\s+([-\d.]+) dBFS", summary)[1])
        assert peak <= report["ceiling_dbtp"] + 0.05, (path, summary)
        assert abs(lufs - item["decoded_metrics"]["integrated_lufs"]) <= 0.15, (path, lufs, item)
        probe = json.loads(command(["ffprobe", "-v", "error", "-show_streams", "-of", "json", path]).stdout)["streams"][0]
        assert int(probe["sample_rate"]) == 44100
        assert int(probe["channels"]) == 2
        results.append({"format": item["format"], "ffmpeg_lufs": lufs, "ffmpeg_true_peak": peak,
                        "encode_passes": item["encode_passes"], "path": str(path)})
    command([binary, source, "--format", "wav24", "--out", output])
    assert hashlib.sha256(source.read_bytes()).hexdigest() == original
    command([binary, source, output / "missing-input.wav", "--out", output], expected=1)
    command([binary, output / "missing-input.wav", "--out", output], expected=3)
    command([binary, source, "--lufs", "-1", "--out", output], expected=2)
    return {"exports": results, "checks": ["six formats", "independent loudness and true-peak", "44100Hz output", "source preserved", "collision handling", "mixed batch failure", "missing input", "invalid target"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path(__file__).resolve().parents[1] / ("target/release/slimeybeats-ez-master-cli" + (".exe" if os.name == "nt" else "")))
    parser.add_argument("--input", type=Path, default=Path(__file__).resolve().parents[2] / "examples/audio/demo-input.wav")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    if args.out:
        args.out.mkdir(parents=True, exist_ok=True)
        print(json.dumps(verify(args.binary, args.input, args.out), indent=2))
    else:
        with tempfile.TemporaryDirectory(prefix="mastercraft-exports-") as directory:
            print(json.dumps(verify(args.binary, args.input, Path(directory)), indent=2))
