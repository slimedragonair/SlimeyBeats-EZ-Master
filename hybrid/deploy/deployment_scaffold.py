"""Offline consistency checks for this small, deliberately fixed example format."""

from __future__ import annotations

import configparser
from pathlib import Path

from cloudflare_scaffold import check_cloudflare_scaffold


def check_scaffold(directory: Path) -> dict[str, bool]:
    unit = configparser.ConfigParser(interpolation=None, strict=False)
    unit.optionxform = str
    try:
        unit_text = (directory / "slimeybeats-hybrid.service").read_text(encoding="utf-8")
        unit.read_string(unit_text)
        environment = dict(
            line.split("=", 1) for line in
            (directory / "slimeybeats-hybrid.env.example").read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        )
        service = unit["Service"]
        unit_section = unit["Unit"]
    except (OSError, ValueError, KeyError, configparser.Error):
        return {"source_examples_parse": False}
    checks = {
        "source_examples_parse": True,
        "dedicated_user_and_group": service.get("User") == service.get("Group") == "slimeybeats-hybrid",
        "fixed_python_entrypoint": service.get("ExecStart") ==
        "/usr/bin/python3 -m hybrid.server --host 127.0.0.1 --port 8787 "
        "--cli ${SLIMEYBEATS_HYBRID_CLI} --data-dir ${SLIMEYBEATS_HYBRID_STORAGE}",
        "mandatory_private_environment": service.get("EnvironmentFile") == "/etc/slimeybeats-hybrid/environment",
        "inventory_gates_startup": service.get("ExecStartPre") ==
        "/usr/bin/python3 /opt/slimeybeats-hybrid/repository/hybrid/deploy/validate_deployment.py inventory"
        and unit_section.get("RequiresMountsFor") == "/var/lib/slimeybeats-hybrid",
        "dedicated_private_storage": service.get("StateDirectory") == "slimeybeats-hybrid"
        and service.get("StateDirectoryMode") == "0700" and service.get("UMask") == "0077"
        and environment.get("SLIMEYBEATS_HYBRID_STORAGE") == "/var/lib/slimeybeats-hybrid",
        "cpu_and_memory_limits": service.get("CPUQuota") == "200%"
        and service.get("MemoryHigh") == "3G" and service.get("MemoryMax") == "4G"
        and service.get("MemorySwapMax") == "0" and service.get("TasksMax") == "64",
        "privilege_and_filesystem_sandbox": service.get("NoNewPrivileges") == "yes"
        and service.get("ProtectSystem") == "strict" and service.get("ProtectHome") == "yes"
        and service.get("PrivateTmp") == "yes" and service.get("PrivateDevices") == "yes"
        and service.get("CapabilityBoundingSet") == "" and service.get("AmbientCapabilities") == "",
        "worker_children_stopped_together": service.get("KillMode") == "control-group",
        "token_placeholder_rejected_at_startup": environment.get("SLIMEYBEATS_HYBRID_TOKEN") == "REPLACE_ME",
        "exact_pages_origin": environment.get("SLIMEYBEATS_HYBRID_ORIGINS") == "https://slimedragonair.github.io",
        "fixed_native_cli": environment.get("SLIMEYBEATS_HYBRID_CLI") ==
        "/opt/slimeybeats-hybrid/repository/rust/target/release/slimeybeats-ez-master-cli",
        "explicit_codec_path_and_pool": "Environment=PATH=/usr/local/bin:/usr/bin:/bin" in unit_text
        and "Environment=RAYON_NUM_THREADS=4" in unit_text,
    }
    checks.update(check_cloudflare_scaffold(directory))
    return checks
