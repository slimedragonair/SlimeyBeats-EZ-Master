"""Read-only deployment checks; no installation, uploads or secret output."""

from __future__ import annotations

import argparse
import getpass
import json
import os
from pathlib import Path

from deployment_health import check_health
from deployment_inventory import check_inventory
from deployment_scaffold import check_scaffold


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("offline", help="Check only repository source examples")
    inventory = commands.add_parser("inventory", help="Read local server prerequisites")
    inventory.add_argument("--cli", type=Path)
    inventory.add_argument("--storage", type=Path)
    inventory.add_argument("--require-tunnel", action="store_true", help="Also require cloudflared")
    health = commands.add_parser("health", help="GET/OPTIONS only; never submit a job")
    health.add_argument("--endpoint", default="http://127.0.0.1:8787")
    health.add_argument("--prompt-token", action="store_true", help="Read without echo")
    args = parser.parse_args()
    if args.command == "offline":
        checks = check_scaffold(Path(__file__).resolve().parent)
    elif args.command == "inventory":
        checks = check_inventory(args.cli, args.storage, args.require_tunnel)
    else:
        token = (
            getpass.getpass("Hybrid bearer token (hidden): ")
            if args.prompt_token
            else os.environ.get("SLIMEYBEATS_HYBRID_TOKEN", "")
        )
        checks = check_health(args.endpoint, token)
    passed = all(checks.values())
    print(json.dumps({"check": args.command, "passed": passed, "checks": checks}, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
