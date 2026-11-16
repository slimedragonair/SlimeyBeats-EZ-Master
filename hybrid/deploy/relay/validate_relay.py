"""Check repository relay examples without connecting, installing or reading keys."""

import argparse
import json
from pathlib import Path

from relay_scaffold import check_scaffold
from relay_tool_checks import check_caddy, check_ssh


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--caddy", type=Path, help="Optional trusted Caddy executable: adapt only, never run")
    parser.add_argument("--ssh", type=Path, help="Optional trusted OpenSSH executable: -G only, never connect")
    args = parser.parse_args()
    directory = Path(__file__).resolve().parent
    checks = check_scaffold(directory)
    tools = {
        "caddy": check_caddy(args.caddy, directory) if args.caddy else {"status": "not_run"},
        "openssh_client": check_ssh(args.ssh, directory) if args.ssh else {"status": "not_run"},
        "sshd_effective_config": {"status": "not_run", "reason": "Operator must check the full VPS configuration"},
        "systemd_runtime_and_live_relay": {"status": "not_run"},
    }
    passed = all(checks.values()) and not any(tool["status"] == "failed" for tool in tools.values())
    print(json.dumps({"passed": passed, "source_checks": checks, "tools": tools}, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
