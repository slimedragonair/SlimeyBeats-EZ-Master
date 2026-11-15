"""Render the native stem editor using an isolated, synthetic four-part project.

This verifies project loading/rendering and captures pixels, not pointer input,
hardware audibility, or installation. Audio and app recovery stay in --out.
"""
import argparse
import array
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys
import wave


def fixture(folder):
    folder.mkdir(parents=True, exist_ok=False)
    rate, frames = 16000, 16000 * 24
    parts = [
        ("vocals", "Lead vocal", 220, 0.21),
        ("drums", "Drums & percussion", 96, 0.09),
        ("bass", "Sub bass", 55, 0.16),
        ("guitar", "Guitar layers", 330, 0.13),
    ]
    mixed = [0] * (frames * 2)
    stems = []

    def asset(name, samples):
        values = array.array("h", samples)
        if sys.byteorder != "little":
            values.byteswap()
        path = folder / name
        with wave.open(str(path), "wb") as writer:
            writer.setparams((2, 2, rate, 0, "NONE", "not compressed"))
            writer.writeframes(values.tobytes())
        data = path.read_bytes()
        return {"name": name, "identity": {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}}

    for number, (kind, name, frequency, cycle) in enumerate(parts):
        samples = []
        for frame in range(frames):
            seconds = frame / rate
            envelope = (0.25 + 0.75 * abs(math.sin(seconds * math.tau * cycle)))
            if number == 0 and int(seconds / 3) % 3 == 0:
                envelope *= 0.04
            if number == 1:
                envelope = math.exp(-(seconds % 0.5) * 16)
            for channel in range(2):
                value = round(math.sin(seconds * math.tau * frequency + channel * 0.12) * envelope * 2200)
                samples.append(value)
                mixed[frame * 2 + channel] += value
        stems.append({"kind": kind, "display_name": name, "file": asset(kind + ".wav", samples)})
    source = asset("source.wav", mixed)
    document = {
        "format": "slimeybeats-studio", "version": 1,
        "source_name": "Synthetic editor session.wav", "source": source,
        "stems": stems,
        "mix": {"tracks": [{"kind": kind, "gain_db": 0, "balance": 0, "mute": False, "solo": False} for kind, *_ in parts]},
        "before": source, "preview": source, "label": "Synthetic four-part editor fixture",
        "notes": ["Generated tones; this is a renderer check, not a music demonstration."],
        "view": "stems",
    }
    path = folder / "session.studio.json"
    path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    return path


def verify(args):
    args.out.mkdir(parents=True, exist_ok=False)
    project = fixture(args.out / "project")
    results = {"scope": __doc__, "cases": []}
    for width, height in [(1480, 940), (1100, 720)]:
        image = (args.out / f"timeline-{width}x{height}.png").resolve()
        completed = subprocess.run([
            str(args.gui.resolve()), "--studio-session", str(project.resolve()),
            "--view", "stems", "--capture-ui", str(image),
            "--capture-size", str(width), str(height),
            "--recovery-data", str((args.out / f"recovery-{width}").resolve()),
        ], capture_output=True, text=True, timeout=120)
        assert completed.returncode == 0, (completed.returncode, completed.stderr)
        state = json.loads(image.with_suffix(".state.json").read_text(encoding="utf-8"))
        assert state["error"] is None and state["preview_ready"] and state["playback_ready"], state
        results["cases"].append({"size": [width, height], "state": state, "image": image.name})
    results["passed"] = True
    (args.out / "results.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gui", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    verify(parser.parse_args())
