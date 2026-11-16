"""Run only offline Caddy adaptation and OpenSSH effective-config inspection."""

import json
import os
from pathlib import Path
import subprocess


def _run(executable, arguments):
    # Do not pass owner tokens, provider credentials or proxy settings to tools.
    # Windows OpenSSH needs the non-secret account/profile fields even for -G.
    allowed = {"PATH", "SYSTEMROOT", "WINDIR", "TEMP", "TMP", "LANG", "USERPROFILE",
               "USERNAME", "USERDOMAIN", "SYSTEMDRIVE", "PROGRAMDATA"}
    environment = {key: value for key, value in os.environ.items() if key.upper() in allowed}
    try:
        result = subprocess.run([str(executable), *arguments], env=environment, capture_output=True,
                                timeout=10, check=False, shell=False)
        if result.returncode == 0 and len(result.stdout) <= 256 * 1024:
            return result.stdout.decode("utf-8")
    except (OSError, UnicodeError, subprocess.TimeoutExpired):
        pass
    return None


def _objects(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _objects(child)
    elif isinstance(value, list):
        for child in value:
            yield from _objects(child)


def caddy_policy(document):
    objects = list(_objects(document))
    proxies = [item for item in objects if item.get("handler") == "reverse_proxy"]
    if len(proxies) != 1:
        return False
    proxy = proxies[0]
    request = proxy.get("headers", {}).get("request", {})
    headers = {key.lower(): value for key, value in request.get("set", {}).items()}
    deletes = {key.lower() for key in request.get("delete", [])}
    host_matches = [item["host"] for item in objects if "host" in item]
    return (
        proxy.get("upstreams") == [{"dial": "127.0.0.1:18787"}]
        and headers == {"host": ["api.slimedragon.net"],
                        "x-slimeybeats-client-ip": ["{http.request.remote.host}"]}
        and deletes == {"cf-*", "x-forwarded-*", "forwarded", "x-real-ip"}
        and host_matches == [["api.slimedragon.net"]]
        and document.get("admin", {}).get("listen") == "127.0.0.1:2019"
        and not any("trusted_proxies" in item for item in objects)
    )


def check_caddy(executable: Path, directory: Path):
    output = _run(executable, ["adapt", "--adapter", "caddyfile", "--config", str(directory / "Caddyfile.example"), "--pretty"])
    try:
        valid = output is not None and caddy_policy(json.loads(output))
    except (ValueError, TypeError, AttributeError, RecursionError):
        valid = False
    return {"status": "passed" if valid else "failed", "parsed_and_checked_header_policy": bool(valid)}


def check_ssh(executable: Path, directory: Path):
    output = _run(executable, ["-G", "-F", str(directory / "home-ssh.conf.example"), "slimeybeats-relay"])
    try:
        settings = {}
        for line in (output or "").splitlines():
            key, value = line.split(None, 1)
            settings.setdefault(key, []).append(value)
        expected = {
            "hostname": ["relay_public_host"], "user": ["slimeybeats-tunnel"], "port": ["22"],
            "remoteforward": ["[127.0.0.1]:18787 [127.0.0.1]:8787"], "exitonforwardfailure": ["yes"],
            "serveraliveinterval": ["15"], "serveralivecountmax": ["3"], "identityagent": ["none"],
            "hostkeyalias": ["slimeybeats-oracle-relay"], "hostkeyalgorithms": ["ssh-ed25519"],
            "userknownhostsfile": ["/etc/slimeybeats-relay/known_hosts"], "globalknownhostsfile": ["/dev/null"],
        }
        valid = all(settings.get(key) == value for key, value in expected.items())
        valid = valid and settings.get("stricthostkeychecking") in (["yes"], ["true"])
    except ValueError:
        valid = False
    return {"status": "passed" if valid else "failed", "effective_pinned_forward_config": bool(valid)}
