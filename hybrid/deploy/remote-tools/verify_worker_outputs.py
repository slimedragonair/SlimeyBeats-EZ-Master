"""Validate actual neural worker WAVs using the isolated runtime's SciPy decoder."""
import hashlib
import json
from pathlib import Path
import sys
import numpy as np
from scipy.io import wavfile


def verify(request_path, response_path):
    request = json.loads(request_path.read_text())
    response = json.loads(response_path.read_text())
    if response["api_version"] != 2 or response["operation"] != "separate":
        raise ValueError("Unexpected worker response")
    if response["resource_limits"]["separation_max_pcm_bytes"] != 536870912:
        raise ValueError("Unexpected neural cap")
    rate, source = wavfile.read(request["input"])
    if source.dtype != np.int16 or source.ndim != 2 or source.shape[1] != 2:
        raise ValueError("Expected the benchmark's original stereo PCM16 fixture")
    source = source.astype(np.float32) / np.float32(32768)
    combined = np.zeros_like(source, dtype=np.float64)
    stems = []
    output_root = Path(request["out_dir"]).resolve()
    for asset in response["assets"]:
        path = Path(asset["path"])
        if path.is_symlink() or not path.resolve().is_relative_to(output_root) or asset["role"] != "stem" or asset["format"] != "wav32f":
            raise ValueError("Invalid stem asset")
        actual_rate, audio = wavfile.read(path)
        if actual_rate != rate or audio.dtype != np.float32 or audio.shape != source.shape or not np.isfinite(audio).all():
            raise ValueError("Stem changed layout or produced non-finite samples")
        if (asset["frames"], asset["sample_rate"], asset["channels"]) != (len(source), rate, 2):
            raise ValueError("Manifest dimensions do not match exported WAV")
        peaks = np.max(np.abs(audio), axis=0)
        metadata_peaks = asset.get("channel_peaks")
        if metadata_peaks is not None and (len(metadata_peaks) != 2 or not np.allclose(metadata_peaks, peaks, rtol=1e-6, atol=1e-8)):
            raise ValueError("Asset channel peaks do not match exported WAV")
        combined += audio
        stems.append({"kind": asset["kind"], "frames": len(audio), "sample_rate": rate, "channels": 2,
                      "rms": float(np.sqrt(np.mean(audio.astype(np.float64) ** 2))), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                      "channel_peaks": peaks.tolist(), "channel_peaks_checked": metadata_peaks is not None})
    expected = ["vocals", "instrumental"] if request["separation_mode"] == "vocal_instrumental" else ["vocals", "drums", "bass", "other"]
    if [stem["kind"] for stem in stems] != expected:
        raise ValueError("Unexpected stem identities")
    error = float(np.sqrt(np.mean((source - combined) ** 2)))
    if error > 1e-6 or not np.isfinite(error):
        raise ValueError("Stem resummation failed")
    return {"stems": stems, "model": response["tool_metadata"], "recombination_error_rms": error, "resource_limits": response["resource_limits"]}


if __name__ == "__main__":
    print(json.dumps(verify(Path(sys.argv[1]), Path(sys.argv[2]))))
