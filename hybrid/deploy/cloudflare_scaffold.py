"""Offline checks of the fixed connector examples, never the operator's secrets."""

from __future__ import annotations

import configparser
import json
from pathlib import Path


def check_cloudflare_scaffold(directory: Path) -> dict[str, bool]:
    unit = configparser.ConfigParser(interpolation=None)
    unit.optionxform = str
    try:
        unit.read_string((directory / "slimeybeats-cloudflared.service").read_text(encoding="utf-8"))
        service, startup = unit["Service"], unit["Unit"]
        remote = json.loads((directory / "cloudflared-remote.json.example").read_text(encoding="utf-8"))
        local = (directory / "slimeybeats-cloudflared-local.conf.example").read_text(encoding="utf-8")
        account = (directory / "slimeybeats-cloudflared.sysusers.conf").read_text(encoding="utf-8")
        tunnel = [line.strip() for line in (directory / "cloudflared.yml.example").read_text(encoding="utf-8").splitlines()
                  if line.strip() and not line.lstrip().startswith("#")]
    except (OSError, ValueError, KeyError, configparser.Error):
        return {"cloudflare_source_examples_parse": False}

    # Compare the shipped subset, not arbitrary remote settings or systemd syntax.
    return {
        "cloudflare_source_examples_parse": True,
        "cloudflare_dedicated_account": service.get("User") == service.get("Group") == "slimeybeats-cloudflared"
        and [line for line in account.splitlines() if line and not line.startswith("#")] == [
            'u slimeybeats-cloudflared - "SlimeyBeats Cloudflare connector" /nonexistent /usr/sbin/nologin'
        ],
        "cloudflare_private_token_file": service.get("LoadCredential") ==
        "tunnel-token:/etc/slimeybeats-cloudflared/tunnel.token"
        and service.get("ExecStartPre") == "/usr/bin/test -s %d/tunnel-token"
        and service.get("ExecStart") ==
        "/usr/local/libexec/cloudflared tunnel --no-autoupdate --loglevel info "
        "--metrics 127.0.0.1:20241 run --token-file %d/tunnel-token",
        "cloudflare_no_secret_environment": service.get("Environment") == "HOME=/nonexistent"
        and "EnvironmentFile" not in service and "PassEnvironment" not in service
        and service.get("WorkingDirectory") == "/",
        "cloudflare_control_plane_allowed_paths": remote == {"config": {"ingress": [
            {"hostname": "api.slimedragon.net", "path": "^/(health|v1/(status|capabilities))$",
             "service": "http://127.0.0.1:8787"},
            {"hostname": "api.slimedragon.net", "service": "http_status:404"},
            {"service": "http_status:404"},
        ]}},
        # Fixed shipped YAML subset only; cloudflared validates the real configuration.
        "tunnel_ingress_subset": tunnel == [
            "tunnel: REPLACE_WITH_TUNNEL_UUID",
            "credentials-file: /etc/cloudflared/REPLACE_WITH_TUNNEL_UUID.json",
            "ingress:", "- hostname: api.slimedragon.net",
            "path: ^/(health|v1/(status|capabilities))$", "service: http://127.0.0.1:8787",
            "- hostname: api.slimedragon.net", "service: http_status:404", "- service: http_status:404",
        ],
        "cloudflare_local_mode_clears_remote_credentials": [
            line for line in local.splitlines() if line and not line.startswith("#")
        ] == [
            "[Service]", "LoadCredential=", "ExecStartPre=",
            "ExecStartPre=/usr/local/libexec/cloudflared tunnel --config /etc/cloudflared/config.yml ingress validate",
            "ExecStart=", "ExecStart=/usr/local/libexec/cloudflared tunnel --no-autoupdate --loglevel info "
            "--metrics 127.0.0.1:20241 --config /etc/cloudflared/config.yml run",
        ],
        "cloudflare_bounded_resources": all(service.get(key) == value for key, value in {
            "CPUQuota": "100%", "MemoryHigh": "128M", "MemoryMax": "256M", "MemorySwapMax": "0",
            "TasksMax": "64", "LimitNOFILE": "1024", "LimitCORE": "0", "OOMPolicy": "stop",
        }.items()),
        "cloudflare_bounded_restarts": startup.get("StartLimitIntervalSec") == "300"
        and startup.get("StartLimitBurst") == "5" and service.get("Restart") == "on-failure"
        and service.get("RestartSec") == "10" and service.get("TimeoutStopSec") == "40"
        and service.get("KillMode") == "control-group",
        "cloudflare_sandbox": all(service.get(key) == value for key, value in {
            "NoNewPrivileges": "yes", "CapabilityBoundingSet": "", "AmbientCapabilities": "",
            "ProtectSystem": "strict", "ProtectHome": "yes", "PrivateTmp": "yes", "PrivateDevices": "yes",
            "ProtectKernelTunables": "yes", "ProtectKernelModules": "yes", "ProtectKernelLogs": "yes",
            "ProtectControlGroups": "yes", "ProtectClock": "yes", "ProtectHostname": "yes",
            "RestrictSUIDSGID": "yes", "RestrictRealtime": "yes", "RestrictNamespaces": "yes",
            "LockPersonality": "yes", "RestrictAddressFamilies": "AF_UNIX AF_INET AF_INET6", "UMask": "0077",
        }.items()),
        "cloudflare_network_ordering": startup.get("Wants") == "network-online.target"
        and startup.get("After") == "network-online.target slimeybeats-hybrid.service",
    }
