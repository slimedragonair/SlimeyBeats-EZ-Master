"""Separate v2 operator limits and capability projection; v1 defaults stay intact."""

from pathlib import Path

from .config import MIB, PROFILES, native_executable

MAX_UPLOADS = 512
MAX_ASSETS = 512
MAX_ARTIFACTS = 32
MAX_STEMS = 128
MAX_STORAGE_FILES = 4096
MAX_BANK_PCM_BYTES = 4 * 1024**3
WAVEFORM_BINS = 1024
CHUNK_BYTES = MIB
FORMATS = ("wav24", "wav16", "wav32f", "flac24", "flac16", "mp3")
INPUT_FORMATS = ("wav", "mp3", "flac", "m4a", "aac", "aiff", "aif", "ogg")
OPERATIONS = ("import", "reference", "analyze", "master", "render", "issues", "repair",
              "voice_cleanup", "spectral_repair", "separate", "mix", "import_stems", "bundle", "export",
              "history", "replace", "promote", "spectrogram", "region_edit", "clone", "suggest")


def validate_config(config):
    bounds = {"v2_max_upload_bytes": 8 * 1024 * MIB, "v2_max_decoded_bytes": 4 * 1024 * MIB - 64,
              "v2_max_audio_seconds": 4 * 3600, "v2_max_project_disk_bytes": 128 * 1024 * MIB,
              "v2_retention_seconds": 3600, "v2_memory_bytes": 256 * 1024 * MIB,
              "v2_cpu_seconds": 86400, "v2_worker_timeout_seconds": 14400,
              "v2_decode_timeout_seconds": 3600, "v2_threads": 32}
    for name, maximum in bounds.items():
        value = getattr(config, name)
        if type(value) is not int or not 1 <= value <= maximum:
            raise ValueError(f"{name} is outside its operator bounds")
    if config.v2_max_upload_bytes > config.v2_max_project_disk_bytes:
        raise ValueError("Project storage must cover an upload")
    if type(config.v2_preview_kbps) is not int or not 96 <= config.v2_preview_kbps <= 320:
        raise ValueError("Preview bitrate must be 96 to 320 kbps")
    if config.studio_cli is not None:
        object.__setattr__(config, "studio_cli", native_executable(config.studio_cli))
    if config.stem_python is not None:
        # Validate the executable target but invoke the venv path. Resolving a
        # venv symlink before exec silently selects the base interpreter.
        native_executable(config.stem_python)
        object.__setattr__(config, "stem_python", Path(config.stem_python).absolute())
    if (config.stem_python is None) != (config.stem_models is None):
        raise ValueError("Stem Python and models must be configured together")
    if config.stem_models is not None:
        path = Path(config.stem_models).resolve(strict=True)
        if not path.is_dir():
            raise ValueError("Stem models must be an operator directory")
        object.__setattr__(config, "stem_models", path)


def processing_config(config):
    # v1 deliberately forbids larger decoded limits in Config.__post_init__.
    # A narrow view lets the existing process supervisor enforce project disk
    # bounds without constructing a weakened v1 Config.
    class View:
        def __getattr__(self, name):
            return getattr(config, name)
    view = View()
    view.max_job_disk_bytes = config.v2_max_project_disk_bytes
    view.max_decoded_bytes = config.v2_max_decoded_bytes
    view.max_audio_seconds = config.v2_max_audio_seconds
    view.max_storage_files = MAX_STORAGE_FILES
    view.memory_bytes = config.v2_memory_bytes
    view.cpu_seconds = config.v2_cpu_seconds
    view.worker_timeout_seconds = config.v2_worker_timeout_seconds
    view.decode_timeout_seconds = config.v2_decode_timeout_seconds
    view.worker_threads = config.v2_threads
    return view


def capabilities(config):
    available = config.studio_cli is not None and config.ffmpeg is not None
    separation = available and config.stem_python is not None
    return {"api_version": 2, "available": available,
            "operations": [op for op in OPERATIONS if op != "separate" or separation] if available else [],
            "formats": list(FORMATS), "input_formats": list(INPUT_FORMATS),
            "profiles": [{"id": key, "label": value} for key, value in PROFILES.items()],
            "features": {"streaming_preview": available, "byte_ranges": True, "manual_eq": True,
                         "reference_upload": True, "stem_separation": separation},
            "limits": {"max_upload_bytes": config.v2_max_upload_bytes,
                       "max_decoded_bytes": config.v2_max_decoded_bytes,
                       "max_audio_seconds": config.v2_max_audio_seconds,
                       "max_project_disk_bytes": config.v2_max_project_disk_bytes,
                       "retention_seconds": config.v2_retention_seconds, "max_jobs": config.max_jobs,
                       "chunk_bytes": CHUNK_BYTES, "max_uploads": MAX_UPLOADS, "max_assets": MAX_ASSETS,
                       "max_artifacts": MAX_ARTIFACTS, "max_stems": MAX_STEMS,
                       "max_storage_files": MAX_STORAGE_FILES, "max_bank_decoded_bytes": MAX_BANK_PCM_BYTES,
                       "waveform_bins": WAVEFORM_BINS, "commands_per_hour": 120,
                       "preview_kbps": config.v2_preview_kbps, "max_projects_per_guest": 2,
                       "separation_max_decoded_bytes": 512 * MIB},
            "privacy": {"temporary_storage": True, "retention_seconds": config.v2_retention_seconds}}
