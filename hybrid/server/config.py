"""Operator configuration and the fixed, built-in mastering allowlist."""

import argparse
from dataclasses import dataclass, field
import ipaddress
import math
import os
from pathlib import Path
import shutil
import sys
import tempfile
from urllib.parse import urlsplit

MIB = 1024 * 1024
PROFILES = {
    "neutral": "Neutral", "metal": "Metal", "rock": "Rock", "pop": "Pop",
    "hiphop_trap": "Hip-Hop/Trap", "edm": "EDM", "acoustic_folk": "Acoustic/Folk",
    "country": "Country", "rnb_soul": "R&B/Soul", "lofi": "Lo-fi",
    "orchestral_cinematic": "Orchestral/Cinematic", "alt_rock": "Alt Rock",
    "metalcore_djent": "Metalcore / Djent", "pop_punk": "Pop-Punk",
    "synthwave_retro": "Synthwave / Retro", "game_ost": "Game / Instrumental Score",
}


def native_executable(path):
    executable = Path(path).resolve(strict=True)
    if not executable.is_file() or not os.access(executable, os.X_OK):
        raise ValueError("A native executable is required")
    with executable.open("rb") as stream:
        magic = stream.read(4)
    if not (magic.startswith(b"MZ") or magic == b"\x7fELF"
            or magic in (b"\xfe\xed\xfa\xce", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf",
                         b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca")):
        raise ValueError("A packaged native executable is required, not a shell script")
    return executable


def parse_origins(value):
    origins = frozenset(part.strip() for part in value.split(",") if part.strip())
    for origin in origins:
        parsed = urlsplit(origin)
        if (parsed.scheme not in ("http", "https") or not parsed.hostname
                or parsed.username or parsed.password or parsed.path
                or parsed.query or parsed.fragment or "*" in origin
                or any(ord(c) <= 32 or ord(c) >= 127 for c in origin)):
            raise ValueError("Origins must be exact http(s) origins without paths or wildcards")
        _ = parsed.port
    return origins


@dataclass(frozen=True)
class Config:
    cli: Path
    token: str = field(repr=False)
    data_dir: Path = field(default_factory=lambda: Path(tempfile.gettempdir()) / "slimeybeats-hybrid-private")
    host: str = "127.0.0.1"
    port: int = 8787
    origins: frozenset = frozenset()
    ffmpeg: Path | None = None
    max_upload_bytes: int = 512 * MIB
    chunk_bytes: int = MIB
    max_jobs: int = 4
    retention_seconds: int = 3600
    max_decoded_bytes: int = 512 * MIB
    max_audio_seconds: int = 1800
    max_job_disk_bytes: int = 2 * 1024 * MIB
    min_free_bytes: int = 64 * MIB
    worker_timeout_seconds: float = 900
    decode_timeout_seconds: float = 120
    memory_bytes: int = 4 * 1024 * MIB
    cpu_seconds: int = 900
    request_timeout_seconds: float = 15
    cleanup_interval_seconds: float = 5
    max_connections: int = 16

    def __post_init__(self):
        if (len(self.token) < 32 or not self.token.isascii()
                or any(ord(c) <= 32 or ord(c) == 127 for c in self.token)):
            raise ValueError("SLIMEYBEATS_HYBRID_TOKEN must contain at least 32 printable ASCII characters")
        cli = native_executable(self.cli)
        object.__setattr__(self, "cli", cli)
        object.__setattr__(self, "data_dir", Path(self.data_dir).absolute())
        object.__setattr__(self, "origins", parse_origins(",".join(self.origins)))
        address = ipaddress.ip_address(self.host)
        if address.version != 4:
            raise ValueError("This preview binds an explicit IPv4 address")
        if not address.is_loopback and not sys.platform.startswith("linux"):
            raise ValueError("Non-loopback preview requires Linux resource limits")
        if not address.is_loopback:
            import resource
            if not all(hasattr(resource, name) for name in ("RLIMIT_AS", "RLIMIT_CPU", "RLIMIT_FSIZE")):
                raise ValueError("Non-loopback preview requires Linux resource limits")
        if type(self.port) is not int or not 0 <= self.port <= 65535:
            raise ValueError("Invalid port")
        for name in ("max_upload_bytes", "chunk_bytes", "max_jobs", "retention_seconds",
                     "max_decoded_bytes", "max_audio_seconds", "max_job_disk_bytes",
                     "min_free_bytes", "memory_bytes", "cpu_seconds", "max_connections"):
            if type(getattr(self, name)) is not int or getattr(self, name) <= 0:
                raise ValueError(f"{name} must be a positive integer")
        for name in ("worker_timeout_seconds", "decode_timeout_seconds", "request_timeout_seconds",
                     "cleanup_interval_seconds"):
            if not math.isfinite(getattr(self, name)) or getattr(self, name) <= 0:
                raise ValueError(f"{name} must be positive and finite")
        if self.max_decoded_bytes > 512 * MIB or self.chunk_bytes > MIB:
            raise ValueError("Decoded audio and upload chunks exceed preview safety bounds")
        if self.ffmpeg is not None:
            ffmpeg = native_executable(self.ffmpeg)
            object.__setattr__(self, "ffmpeg", ffmpeg)

    def capabilities(self):
        return {
            "api_version": 1, "operations": ["master"], "formats": ["wav24", "mp3"],
            "profiles": [{"id": key, "label": value} for key, value in PROFILES.items()],
            "limits": {name: getattr(self, name) for name in
                       ("max_upload_bytes", "chunk_bytes", "max_jobs", "retention_seconds",
                        "max_audio_seconds", "max_decoded_bytes")},
            "privacy": {"temporary_storage": True, "retention_seconds": self.retention_seconds},
        }


def from_arguments(argv=None):
    parser = argparse.ArgumentParser(description="Private owner/invite hybrid preview; protect remote access with TLS")
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--ffmpeg", type=Path)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8787)
    parser.add_argument("--data-dir", "--storage-dir", type=Path,
                        default=Path(tempfile.gettempdir()) / "slimeybeats-hybrid-private")
    for name in ("max-upload-bytes", "chunk-bytes", "max-jobs", "retention-seconds",
                 "max-decoded-bytes", "max-audio-seconds", "max-job-disk-bytes",
                 "min-free-bytes", "memory-bytes", "cpu-seconds"):
        parser.add_argument("--" + name, type=int, default=argparse.SUPPRESS)
    parser.add_argument("--worker-timeout-seconds", type=float, default=argparse.SUPPRESS)
    parser.add_argument("--decode-timeout-seconds", type=float, default=argparse.SUPPRESS)
    args = vars(parser.parse_args(argv))
    args["token"] = os.environ.get("SLIMEYBEATS_HYBRID_TOKEN", "")
    args["origins"] = parse_origins(os.environ.get("SLIMEYBEATS_HYBRID_ORIGINS", ""))
    if args["ffmpeg"] is None:
        sibling = args["cli"].absolute().parent / ("ffmpeg.exe" if os.name == "nt" else "ffmpeg")
        discovered = sibling if sibling.is_file() else shutil.which("ffmpeg")
        args["ffmpeg"] = Path(discovered) if discovered else None
    return Config(**args)
