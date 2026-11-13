"""Actual VST3 probe/render checks against a compatible gain fixture."""
import argparse
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import wave


def floats(path):
    data = path.read_bytes()
    assert data[:4] == b"RIFF" and data[8:12] == b"WAVE"
    position = 12
    while position + 8 <= len(data):
        kind = data[position:position+4]
        size = struct.unpack_from("<I", data, position+4)[0]
        if kind == b"data":
            return struct.unpack("<" + "f" * (size // 4), data[position+8:position+8+size])
        position += 8 + size + size % 2
    raise AssertionError("No WAV data")


def verify(host, plugin, latency=0, expect_log=False):
    def call(args):
        completed = subprocess.run(args, capture_output=True, text=True, timeout=30, check=True)
        if expect_log:
            assert 'SlimeyBeats QA fixture startup log' in completed.stderr
            assert 'SlimeyBeats QA native CRT startup log' in completed.stderr
        return json.loads(completed.stdout)
    probe = call([str(host), "probe", str(plugin)])
    effect = next(p for p in probe["plugins"] if p.get("parameters"))
    class_id = effect["class"]["id"]
    assert effect["parameters"][0]["id"] == 0
    with tempfile.TemporaryDirectory(prefix="SlimeyBeats plugin QA ") as temporary:
        root = Path(temporary)
        source = root / "source.wav"
        samples = [int((i % 1000 - 500) * 20) for i in range(48000)]
        results = []
        for channels in (1, 2):
            with wave.open(str(source), "wb") as wav:
                wav.setparams((channels, 2, 48000, 0, "NONE", ""))
                wav.writeframes(struct.pack("<" + "h" * len(samples), *samples))
            for gain in (0.0, 0.5, 1.0):
                output = root / f"gain-{channels}-{gain}.wav"
                args = [str(host), "render", str(plugin), "--class-id", class_id,
                        "--input", str(source), "--output", str(output),
                        "--parameters", json.dumps([{"id": 0, "normalized": gain}])]
                report = call(args)
                rendered = floats(output)
                assert len(rendered) == len(samples)
                error = max(abs(actual - original / 32768 * gain) for original, actual in zip(samples, rendered))
                assert error < 1e-7, (channels, gain, error)
                assert report["frames"] == len(samples) // channels and report["channels"] == channels
                assert report["latency_frames_compensated"] == latency
                # The helper must refuse to clobber an existing file.
                repeated = subprocess.run(args, capture_output=True, timeout=30)
                assert repeated.returncode != 0
                results.append({"channels": channels, "gain": gain, "max_sample_error": error, "latency_frames_compensated": latency})
        return {"class": effect["class"], "renders": results, "non_clobber": True, "plugin_logs_separated": expect_log}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--plugin", type=Path, required=True)
    parser.add_argument("--latency-frames", type=int, default=0)
    parser.add_argument("--expect-plugin-log", action='store_true')
    args = parser.parse_args()
    print(json.dumps(verify(args.host.resolve(), args.plugin.resolve(), args.latency_frames, args.expect_plugin_log), indent=2))
