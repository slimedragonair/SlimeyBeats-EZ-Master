"""Inspect a server without creating directories or running a mastering job."""

from __future__ import annotations

import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

DEFAULT_CLI = "/opt/slimeybeats-hybrid/repository/rust/target/release/slimeybeats-ez-master-cli"
DEFAULT_STORAGE = "/var/lib/slimeybeats-hybrid"
MINIMUM_FREE_BYTES = 16 * 1024**3
MAXIMUM_USED_FRACTION = 0.85


def _codec_ready(name: str) -> bool:
    executable = shutil.which(name)
    if not executable:
        return False
    environment = os.environ.copy()
    environment.pop("SLIMEYBEATS_HYBRID_TOKEN", None)
    try:
        result = subprocess.run(
            [executable, "-version"], env=environment, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, timeout=10, check=False,
        )
        return result.returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def check_inventory(cli: Path | None, storage: Path | None, require_tunnel: bool = False) -> dict[str, bool]:
    cli = cli or Path(os.environ.get("SLIMEYBEATS_HYBRID_CLI", DEFAULT_CLI))
    storage = storage or Path(os.environ.get("SLIMEYBEATS_HYBRID_STORAGE", DEFAULT_STORAGE))
    try:
        storage_ready = storage.is_absolute() and storage.is_dir()
        usage = shutil.disk_usage(storage) if storage_ready else None
        free_space = usage is not None and usage.free >= MINIMUM_FREE_BYTES
        volume_headroom = usage is not None and usage.used / usage.total <= MAXIMUM_USED_FRACTION
        separate_volume = storage_ready and storage.stat().st_dev != Path("/").stat().st_dev
        private_storage = storage_ready and storage.stat().st_mode & 0o077 == 0
        cli_ready = cli.is_absolute() and cli.is_file() and os.access(cli, os.X_OK)
    except OSError:
        storage_ready = free_space = volume_headroom = separate_volume = private_storage = cli_ready = False
    checks = {
        "linux": platform.system() == "Linux",
        "python_3_11_or_newer": sys.version_info >= (3, 11),
        "systemd_analyze_available": shutil.which("systemd-analyze") is not None,
        "configured_cli_executable": cli_ready,
        "dedicated_storage_exists": storage_ready,
        "storage_private_to_owner": private_storage,
        "storage_on_non_root_volume": separate_volume,
        "storage_writable_by_caller": storage_ready and os.access(storage, os.W_OK | os.X_OK),
        "storage_at_least_16_gib_free": bool(free_space),
        "storage_volume_at_most_85_percent_used": bool(volume_headroom),
        "ffmpeg_on_explicit_path": _codec_ready("ffmpeg"),
        "ffprobe_on_explicit_path": _codec_ready("ffprobe"),
    }
    if require_tunnel:
        checks["cloudflared_available"] = shutil.which("cloudflared") is not None
    return checks
