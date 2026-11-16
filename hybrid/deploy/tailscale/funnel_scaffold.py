"""Read-only validation of the public source unit, never its private node state."""

import configparser
import json
from pathlib import Path
import shlex


def check_unit(path):
    unit = configparser.ConfigParser(interpolation=None)
    unit.optionxform = str
    try:
        unit.read_string(path.read_text(encoding="utf-8"))
        service, startup = unit["Service"], unit["Unit"]
        arguments = shlex.split(service["ExecStart"])
        if not arguments:
            raise ValueError("Missing daemon command")
        flags = {}
        for argument in arguments[1:]:
            key, separator, value = argument.partition("=")
            if key in flags:
                raise ValueError("Duplicate daemon flag")
            flags[key] = value if separator else True
    except (OSError, ValueError, KeyError, configparser.Error):
        return {"source_unit_parse": False}
    return {
        "source_unit_parse": True,
        "dedicated_unprivileged_account": service.get("User") == service.get("Group") == "slimeybeats-funnel",
        "userspace_daemon_only": arguments[0] == "/usr/local/libexec/slimeybeats-funnel/tailscaled"
        and flags.get("--tun") == "userspace-networking" and flags.get("--port") == "0",
        # --state alone does not establish the certificate/config VarRoot contract.
        "explicit_persistent_statedir_for_https": flags.get("--statedir") == "/var/lib/slimeybeats-funnel",
        "stable_node_identity_file": flags.get("--state") == "/var/lib/slimeybeats-funnel/tailscaled.state",
        "private_local_control_socket": flags.get("--socket") == "/run/slimeybeats-funnel/tailscaled.sock"
        and service.get("RuntimeDirectory") == "slimeybeats-funnel" and service.get("RuntimeDirectoryMode") == "0700",
        "private_persistent_state": service.get("StateDirectory") == "slimeybeats-funnel"
        and service.get("StateDirectoryMode") == "0700" and service.get("UMask") == "0077"
        and service.get("ReadWritePaths") == "/var/lib/slimeybeats-funnel"
        and startup.get("RequiresMountsFor") == "/var/lib/slimeybeats-funnel",
        "no_auth_key_in_source_arguments_or_environment": set(flags) == {
            "--tun", "--statedir", "--state", "--socket", "--port", "--no-logs-no-support"
        } and service.get("Environment") == "HOME=/var/lib/slimeybeats-funnel"
        and "EnvironmentFile" not in service and flags.get("--no-logs-no-support") is True,
        "privilege_and_filesystem_sandbox": all(service.get(key) == value for key, value in {
            "NoNewPrivileges": "yes", "CapabilityBoundingSet": "", "AmbientCapabilities": "",
            "ProtectSystem": "strict", "ProtectHome": "yes", "PrivateDevices": "yes", "PrivateTmp": "yes",
        }.items()),
        "resource_and_restart_budgets": all(service.get(key) == value for key, value in {
            "CPUQuota": "100%", "MemoryHigh": "192M", "MemoryMax": "384M", "MemorySwapMax": "0",
            "TasksMax": "64", "LimitNOFILE": "2048", "LimitCORE": "0", "Restart": "on-failure",
            "RestartSec": "5", "KillMode": "control-group", "TimeoutStopSec": "30",
        }.items()) and startup.get("StartLimitIntervalSec") == "300" and startup.get("StartLimitBurst") == "5",
    }


if __name__ == "__main__":
    checks = check_unit(Path(__file__).with_name("slimeybeats-funnel.service"))
    passed = all(checks.values())
    print(json.dumps({"passed": passed, "checks": checks, "live_activation": "not_checked"}, indent=2))
    raise SystemExit(0 if passed else 1)
