# Optional HTTPS and reverse SSH relay

Source preparation only. The user selected Tailscale Funnel for the current
connector; this VPS pattern is an unused future option. No VPS account signup,
provisioning or DNS changes have occurred. Nothing here installs, connects or
deploys a host. This is one possible public-browser relay using Caddy, HTTPS and ordinary
encrypted OpenSSH. Existing Cloudflare staging remains an unused alternative.
No R2 activation or payment is required by these examples.

```text
browser -- HTTPS --> relay Caddy :443
                    --> relay 127.0.0.1:18787
                        -- existing outbound SSH connection --> home 127.0.0.1:8787
```

Home initiates `ssh -NT -R 127.0.0.1:18787:127.0.0.1:8787`; no inbound home port
is needed. Visitors connect to the relay IP. The relay/provider still sees the
home SSH source IP, and the relay terminates HTTPS and can see proxied requests.
This does not erase old DNS records or other application/network disclosures.

## Parent-owned preparation

If this option is selected, provision an eligible small relay VM; an Oracle
Always Free eligible `VM.Standard.E2.1.Micro` has 1 GB RAM. Run only Caddy/SSH and
the OS there, not audio DSP. Caddy's proposed 256 MiB hard process budget is an
initial setting, not a measured throughput guarantee. Oracle currently documents
10 TB monthly outbound data in Always Free; eligibility, capacity, quotas and
other billable services still need account review. These templates impose no
global byte-transfer cap or provider bill cap. `IPAccounting` supplies counters
that can reset, not billing enforcement. Any desired 20 GB usage alert needs an
explicit window/unit and operator monitoring; no such cap is claimed here.

Parent alone creates/changes the `api.slimedragon.net` DNS-only A/AAAA records
to the selected relay IP, if approved. Do not publish an AAAA record without a
working relay IPv6 path. Squarespace nameservers, mail records, DNSSEC/DS,
apex/www and unrelated DNS stay untouched. Review both host and cloud firewalls:
public TCP 80/443 for Caddy; SSH only as needed by the home/admin peers. Never
open relay ports 18787 or 2019 publicly or expose home port 8787.

Review/create the two separate no-login accounts with the supplied sysusers
files on their respective machines. Parent can use `systemd-sysusers` on a
reviewed copy; no script invokes it here. Check distro account/PAM policy permits
key-only forwarding for the tunnel account without enabling password or shell
access. Use packaged Caddy's existing dedicated account and service.

| Example | Operator destination and permissions |
| --- | --- |
| `home-ssh.conf.example` | Home `/etc/slimeybeats-relay/ssh_config`, root:root 0644 |
| `home-known_hosts.example` | Home `/etc/slimeybeats-relay/known_hosts`, root:root 0644 |
| `slimeybeats-relay.service` | Home `/etc/systemd/system/slimeybeats-relay.service`, root:root 0644 |
| `home-backend-localonly.conf.example` | Home `slimeybeats-hybrid.service.d/relay-localonly.conf`, root:root 0644 |
| `vps-sshd.conf.example` | Relay `/etc/ssh/sshd_config.d/60-slimeybeats-relay.conf`, root:root 0644 |
| `vps-authorized_keys.example` | Relay `/etc/ssh/authorized_keys/slimeybeats-tunnel`, root:root 0644 |
| `Caddyfile.example` | Relay `/etc/caddy/Caddyfile`, root:caddy 0640 |
| `vps-caddy-limits.conf.example` | Relay `caddy.service.d/relay-limits.conf`, root:root 0644 |

Use root-owned 0755 parent directories for public SSH configuration/key files.
The central authorized_keys contains only public material and must be readable
by the target user while remaining unwritable by it. Parent generates a dedicated
Ed25519 home key; only its public half replaces `REPLACE_WITH_HOME_PUBLIC_KEY_BASE64`.
Put the private half at home `/etc/slimeybeats-relay/id_ed25519`, root:root 0600.
Never commit, print or reuse an admin/owner private key. `LoadCredential` (systemd
247+) exposes a private runtime copy to the connector; argv contains only its
path. Replace `RELAY_PUBLIC_HOST` with the selected SSH endpoint. Verify the
relay's Ed25519 host public key through a trusted console/channel before filling
the known-hosts placeholder; `ssh-keyscan` alone is not initial authentication.

The SSH account Match block permits remote TCP forwarding only at loopback
18787, denies session channels (`MaxSessions 0`), forces `/bin/false`, and denies
PTY, agent, X11, stream-local and tunnel-device forwarding. `-N` opens no session
channel, so it remains usable. Authorized-key `restrict,port-forwarding` enables
only the subsequently narrowed forwarding permission. A key holder controls the
home-side forwarding target: sshd's PermitListen restricts the relay listener,
not that target. Protect and revoke the dedicated key accordingly.
`ExitOnForwardFailure` checks listener setup, not home backend health; verify the
HTTP health endpoint separately.

## Backend trust and rollout gate

Caddy overwrites Host with `api.slimedragon.net` and overwrites the entire
`X-SlimeyBeats-Client-IP` value with the actual socket peer `{remote_host}`.
It removes CF-*, X-Forwarded-*, Forwarded and X-Real-IP upstream headers and
does not trust inbound proxy headers. Keep Caddy directly internet-facing for
this contract. Only the named version-one API/health routes are forwarded;
MCP and unknown routes return 404. This API includes upload/run/download routes
for a later approved deployment; no Cloudflare transport is used in this path.
The existing API's upload chunking is a protocol behavior, not a policy bypass.

**Guest integration prerequisite:** the backend must explicitly support trusting
`X-SlimeyBeats-Client-IP` only from the selected loopback relay boundary, require
the pinned Host, reject duplicate/malformed addresses, and ignore competing
forwarding headers. The backend owner has added explicit `--trust-relay-client-ip`
support with `--expected-host api.slimedragon.net`; independently checked malformed,
duplicate and competing-header cases fail closed. Do not use the Cloudflare trust
flag for this relay. The operator still needs an approved effective service
configuration and live header tests before claiming public relay rate limits work.

The home drop-in keeps network access local with `IPAddressDeny=any` and
`IPAddressAllow=localhost`; retain the existing 127.0.0.1:8787 binding. It also
unsets the public guest flag after environment-file merging, preserving private
owner operation. Parent must explicitly remove that unset only after approving
the public rollout. This staged drop-in changes no running service or local UI.
Verify kernel/cgroup BPF enforcement; do not assume the IP filter works merely
because systemd parsed it. Local processes share the trust boundary.

## Offline checks and remaining acceptance

From the repository root:

```sh
python3 hybrid/deploy/relay/validate_relay.py
python3 hybrid/deploy/relay/validate_relay.py --caddy /trusted/path/caddy --ssh /usr/bin/ssh
python3 -m unittest discover -s hybrid/deploy/relay/tests -v
python3 -m unittest discover -s hybrid/deploy/tests -v
```

Optional tools run only `caddy adapt` and `ssh -G`; they never start listeners,
resolve/connect to the relay, read private keys, install files or contact OCI.
Unavailable/unrequested checks remain `not_run`. Tool output/errors are not
echoed, and tool subprocesses receive only selected non-secret environment fields.
Windows offline parsing was verified with a temporary SHA-256-checked Caddy
v2.11.9 official binary (Windows amd64 ZIP SHA-256
`ad93112d335ee687100c4c9f11bced16ca590377976b7fe7883882b9e797e472`);
the binary was removed. OpenSSH effective-config checks
also run without a connection. These prove syntax/config policy, not live routing.

Parent must separately run `sshd -t` and inspect effective `sshd -T -C` for the
tunnel user against the full relay config (includes/order matter), validate the
prepared systemd units/drop-ins and cgroup limits, verify authorized-key options,
and preserve an administrative recovery session before any SSH reload. Confirm
host/key authentication, loopback-only listeners, header replacement under
duplicate spoofed headers, no public MCP, reconnect/restart behavior, HTTPS,
own/foreign guest isolation, upload/download and resource/traffic behavior before
deployment approval. No live acceptance, Oracle availability or billing outcome
is established by the offline checks.

Official sources checked November 15, 2026: [Caddy proxy headers](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy#headers),
[Caddy adapt](https://caddyserver.com/docs/command-line#caddy-adapt),
[Caddy v2.11.9](https://github.com/caddyserver/caddy/releases/tag/v2.11.9),
[OpenSSH client options](https://man.openbsd.org/ssh_config),
[sshd Match/PermitListen/MaxSessions](https://man.openbsd.org/sshd_config),
[authorized_keys restrict/port-forwarding](https://man.openbsd.org/sshd),
[systemd credentials/environment](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html),
[systemd IP filtering/accounting](https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html),
and [Oracle Always Free resources](https://docs.oracle.com/en-us/iaas/Content/FreeTier/freetier_topic-Always_Free_Resources.htm).
