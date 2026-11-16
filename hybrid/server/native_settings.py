"""Generate private native settings and reference files from validated requests."""

import json
import os


def write_private_json(path, value):
    # Exclusive creation prevents following an existing link or replacing a file.
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
        json.dump(value, stream, allow_nan=False, separators=(",", ":"))


def prepare_native_settings(job, out):
    request = job.request
    settings_path = job.directory / "settings.json"
    write_private_json(settings_path, {
        "profile": request.profile, "mode": request.mode, "target_lufs": request.target_lufs,
        "ceiling_dbtp": request.ceiling_dbtp, "intensity": request.intensity,
        "safety": request.safety, "formats": [request.format], "out_dir": str(out),
        "output_sr": 0, "dither": request.format != "wav32f",
    })
    paths = [settings_path]
    arguments = ["--settings", settings_path]
    if request.reference_analysis is not None:
        reference_path = job.directory / "reference-analysis.json"
        write_private_json(reference_path, request.reference_analysis)
        arguments.extend(["--reference-analysis", reference_path])
        paths.append(reference_path)
    return arguments, paths
