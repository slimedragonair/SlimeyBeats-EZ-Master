"""Convert opaque project selections into fixed private native requests."""

import math
import shutil

from .project_audio import decode_upload
from .project_commands import selected_audio, selected_stems
from .project_limits import MAX_BANK_PCM_BYTES, WAVEFORM_BINS
from .project_references import reference_projection


def private_request(command, config, operation, settings):
    return {"api_version": 2, "operation": operation,
               "project_root": str(command.project.directory), "out_dir": str(command.stage / "out"),
               "settings": dict(settings),
               "limits": {"max_pcm_bytes": config.max_decoded_bytes,
                          "max_bank_pcm_bytes": min(MAX_BANK_PCM_BYTES, config.max_job_disk_bytes),
                          "max_duration_seconds": config.max_audio_seconds,
                          "decode_timeout_seconds": max(1, math.ceil(config.decode_timeout_seconds)),
                          "waveform_bins": WAVEFORM_BINS}}


def build_request(command, config, runner):
    project, public = command.project, command.request
    operation = public["operation"]
    native_operation = {"reference": "import", "export": "render", "clone": "import", "bundle": "render"}.get(operation, operation)
    request = private_request(command, config, native_operation, public.get("settings", {}))
    request["settings"]["formats"] = [public.get("format", "wav24")]
    temporary_inputs = []
    if operation == "clone":
        request["input"] = str(command.stage / "cloned-input.wav")
    elif operation in ("import", "reference", "import_stems"):
        paths = []
        uploads = [project.uploads[upload_id] for upload_id in public["upload_ids"]]
        for upload in uploads:
            if upload.role == "stem_bundle":
                # ZIP validation/extraction is handled by the shared native Suno
                # planner, which refuses traversal/links and bounds expansion.
                paths.append(upload.path)
            else:
                folder = command.stage / ("decoded-" + upload.id)
                folder.mkdir(mode=0o700)
                destination = folder / "input.wav"
                decode_upload(upload, destination, command, config, runner)
                paths.append(destination)
                temporary_inputs.append(folder)
        if operation == "import_stems" and uploads[0].role != "stem_bundle":
            # Labels are private planner input only. Opaque local filenames must
            # not erase Suno part identities or the native WAV/MP3 pairing rules.
            request["inputs"] = [{"path": str(path), "name": upload.source_name}
                                 for upload, path in zip(uploads, paths)]
            for entry, upload in zip(request["inputs"], uploads):
                if upload.kind:
                    entry["kind"] = upload.kind
        else:
            request["input"] = str(paths[0])
    elif operation == "mix":
        stems = selected_stems(project, public)
        request["stems"] = [{"path": str(asset["path"]), "kind": kind} for asset, kind in stems]
        mix = dict(public.get("mix", {}))
        mix.setdefault("tracks", [{"kind": kind, "gain_db": 0, "balance": 0, "mute": False, "solo": False}
                                  for _, kind in stems])
        request["mix"] = mix
    else:
        request["input"] = str(selected_audio(project, public)["path"])
    if operation == "master":
        reference = reference_projection(project, public, config)
        if reference is not None:
            request["reference_analysis"] = reference
    for key in ("voice", "repair", "spectral", "region", "spectrogram"):
        if key in public:
            request[key] = public[key]
    if operation == "separate":
        request["runtime"] = {"python": str(config.stem_python), "models": str(config.stem_models)}
        request["separation_mode"] = public.get("separation_mode", "vocal_instrumental")
    return request, temporary_inputs


def remove_inputs(paths):
    for path in paths:
        shutil.rmtree(path)
