"""Offline checks for the deliberately fixed relay examples, not installed hosts."""

import configparser
from pathlib import Path


def source_lines(directory, name):
    return [line.strip() for line in (directory / name).read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith("#")]


def directives(lines):
    result = {}
    for line in lines:
        key, value = line.split(None, 1)
        if key in result:
            raise ValueError("Duplicate source directive")
        result[key] = value
    return result


def unit_service(directory, name):
    unit = configparser.ConfigParser(interpolation=None)
    unit.optionxform = str
    unit.read_string((directory / name).read_text(encoding="utf-8"))
    return unit


def check_scaffold(directory: Path) -> dict[str, bool]:
    try:
        ssh = directives(source_lines(directory, "home-ssh.conf.example"))
        daemon = source_lines(directory, "vps-sshd.conf.example")
        if len(daemon) < 2:
            raise ValueError("Incomplete sshd Match block")
        sshd = directives(daemon[1:-1])
        unit = unit_service(directory, "slimeybeats-relay.service")
        service, startup = unit["Service"], unit["Unit"]
        backend = unit_service(directory, "home-backend-localonly.conf.example")["Service"]
        caddy_limits = unit_service(directory, "vps-caddy-limits.conf.example")["Service"]
        caddy = source_lines(directory, "Caddyfile.example")
        keys = source_lines(directory, "vps-authorized_keys.example")
        hosts = source_lines(directory, "home-known_hosts.example")
        accounts = [source_lines(directory, name) for name in ("home-relay.sysusers.conf", "vps-tunnel.sysusers.conf")]
    except (OSError, ValueError, KeyError, configparser.Error):
        return {"relay_source_examples_parse": False}

    expected_ssh = {
        "Host": "slimeybeats-relay", "HostName": "RELAY_PUBLIC_HOST", "User": "slimeybeats-tunnel", "Port": "22",
        "RemoteForward": "127.0.0.1:18787 127.0.0.1:8787", "BatchMode": "yes", "IdentitiesOnly": "yes",
        "IdentityAgent": "none", "PubkeyAuthentication": "yes", "PreferredAuthentications": "publickey",
        "PasswordAuthentication": "no", "KbdInteractiveAuthentication": "no", "StrictHostKeyChecking": "yes",
        "UserKnownHostsFile": "/etc/slimeybeats-relay/known_hosts", "GlobalKnownHostsFile": "/dev/null",
        "HostKeyAlias": "slimeybeats-oracle-relay", "HostKeyAlgorithms": "ssh-ed25519", "CheckHostIP": "no",
        "UpdateHostKeys": "no", "VerifyHostKeyDNS": "no", "CanonicalizeHostname": "no",
        "ServerAliveInterval": "15", "ServerAliveCountMax": "3", "ExitOnForwardFailure": "yes",
        "ConnectTimeout": "10", "ConnectionAttempts": "1", "TCPKeepAlive": "no", "Compression": "no",
        "RequestTTY": "no", "ForwardAgent": "no", "ForwardX11": "no", "PermitLocalCommand": "no",
        "ControlMaster": "no", "ControlPath": "none",
    }
    expected_sshd = {
        "AuthenticationMethods": "publickey", "PubkeyAuthentication": "yes", "PasswordAuthentication": "no",
        "KbdInteractiveAuthentication": "no", "AuthorizedKeysFile": "/etc/ssh/authorized_keys/slimeybeats-tunnel",
        "AllowTcpForwarding": "remote", "AllowStreamLocalForwarding": "no", "PermitListen": "127.0.0.1:18787",
        "PermitOpen": "none", "GatewayPorts": "no", "PermitTTY": "no", "X11Forwarding": "no",
        "AllowAgentForwarding": "no", "PermitTunnel": "no", "PermitUserRC": "no", "MaxSessions": "0",
        "ClientAliveInterval": "15", "ClientAliveCountMax": "3", "ForceCommand": "/bin/false",
    }
    # A source-format check, not a Caddy parser. --caddy additionally checks adapted JSON.
    expected_caddy = [
        "{", "admin 127.0.0.1:2019", "servers {", "protocols h1 h2", "timeouts {", "read_header 10s",
        "idle 30s", "}", "max_header_size 32KB", "}", "}", "api.slimedragon.net {",
        "@api path /health /v1/status /v1/session /v1/capabilities /v1/jobs /v1/jobs/*", "handle @api {",
        "reverse_proxy 127.0.0.1:18787 {", "header_up Host api.slimedragon.net",
        "header_up X-SlimeyBeats-Client-IP {remote_host}", "header_up -CF-*", "header_up -X-Forwarded-*",
        "header_up -Forwarded", "header_up -X-Real-IP", "transport http {", "dial_timeout 5s",
        "response_header_timeout 30s", "}", "}", "}", "handle {", "respond 404", "}", "}",
    ]
    return {
        "relay_source_examples_parse": True,
        "ssh_pinned_host_and_loopback_forward": ssh == expected_ssh,
        "sshd_remote_only_dedicated_match": daemon[0] == "Match User slimeybeats-tunnel"
        and daemon[-1] == "Match all" and sshd == expected_sshd,
        "authorized_key_restricted_listener": keys == [
            'restrict,port-forwarding,permitlisten="127.0.0.1:18787" ssh-ed25519 '
            'REPLACE_WITH_HOME_PUBLIC_KEY_BASE64 slimeybeats-home-relay'
        ],
        "host_pin_public_placeholder_only": hosts == [
            "slimeybeats-oracle-relay ssh-ed25519 REPLACE_WITH_RELAY_HOST_PUBLIC_KEY_BASE64"
        ],
        "separate_no_login_accounts": accounts == [
            ['u slimeybeats-relay - "SlimeyBeats outbound SSH connector" /nonexistent /usr/sbin/nologin'],
            ['u slimeybeats-tunnel - "SlimeyBeats restricted reverse SSH account" /nonexistent /usr/sbin/nologin'],
        ],
        "caddy_fixed_api_and_peer_header_policy": caddy == expected_caddy,
        "ssh_systemd_private_credential": service.get("User") == service.get("Group") == "slimeybeats-relay"
        and service.get("Environment") == "HOME=/nonexistent" and "EnvironmentFile" not in service
        and service.get("LoadCredential") == "relay-key:/etc/slimeybeats-relay/id_ed25519"
        and service.get("ExecStartPre") == "/usr/bin/test -s %d/relay-key"
        and service.get("ExecStart") == "/usr/bin/ssh -NT -F /etc/slimeybeats-relay/ssh_config -i %d/relay-key slimeybeats-relay",
        "ssh_bounded_resources_and_reconnect": all(service.get(key) == value for key, value in {
            "Restart": "always", "RestartSec": "15", "TimeoutStopSec": "10", "KillMode": "control-group",
            "CPUQuota": "50%", "MemoryHigh": "32M", "MemoryMax": "64M", "MemorySwapMax": "0",
            "TasksMax": "16", "LimitNOFILE": "128", "LimitCORE": "0", "IPAccounting": "yes",
        }.items()) and startup.get("StartLimitIntervalSec") == "60" and startup.get("StartLimitBurst") == "5",
        "ssh_privilege_sandbox": all(service.get(key) == value for key, value in {
            "NoNewPrivileges": "yes", "CapabilityBoundingSet": "", "AmbientCapabilities": "", "UMask": "0077",
            "ProtectSystem": "strict", "ProtectHome": "yes", "PrivateTmp": "yes", "PrivateDevices": "yes",
            "ProtectKernelTunables": "yes", "ProtectKernelModules": "yes", "ProtectKernelLogs": "yes",
            "ProtectControlGroups": "yes", "RestrictSUIDSGID": "yes", "RestrictRealtime": "yes",
            "RestrictNamespaces": "yes", "LockPersonality": "yes", "RestrictAddressFamilies": "AF_UNIX AF_INET AF_INET6",
        }.items()),
        "backend_loopback_and_private_default": dict(backend) == {
            "IPAddressDeny": "any", "IPAddressAllow": "localhost", "IPAccounting": "yes",
            "UnsetEnvironment": "SLIMEYBEATS_HYBRID_PUBLIC",
        },
        "caddy_relay_only_resource_budget": dict(caddy_limits) == {
            "CPUQuota": "100%", "MemoryHigh": "128M", "MemoryMax": "256M", "MemorySwapMax": "0",
            "TasksMax": "64", "LimitNOFILE": "1024", "LimitCORE": "0", "IPAccounting": "yes",
        },
    }
