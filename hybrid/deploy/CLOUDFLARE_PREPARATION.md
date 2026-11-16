# Cloudflare connector preparation

These are reviewable source examples, not an installed or connected tunnel.
They publish only JSON control endpoints at `api.slimedragon.net` through
`cloudflared` to `http://127.0.0.1:8787`. `/health`, `/v1/status`, and
`/v1/capabilities` are the entire anchored path allowlist. All other paths on
that hostname return 404, followed by a global 404 catchall. Audio upload,
artifact download, session creation, job execution, MCP, apex, and www are not
routed. The origin's existing authentication/CORS and private interface remain
in force. This is path routing, not method filtering or a content inspection
firewall; the origin must continue rejecting unexpected methods and bodies.

## Operator review gates

- Review `cloudflare-source-findings.json` for dated official facts, URLs and
  exact section names. Public Tunnel large-file use requires an appropriate
  service under Cloudflare's published guidance. Keep audio paths blocked until
  R2 or another approved transfer architecture is reviewed and integrated.
  File splitting/chunking or cache changes do not settle permission. R2 billing,
  activation and free allowances are separate parent-owned decisions.
- Free full DNS setup requires Cloudflare authoritative nameservers. Keeping
  Squarespace authoritative DNS with a Cloudflare partial setup is not a Free
  option. Parent reports an existing DS and Google mail MX records: export and
  reconcile the full provider zone, including MX, SPF, DKIM, DMARC, verification,
  apex/www and other records before any nameserver switch. Audit DNSSEC and old
  DS TTL expiry using Cloudflare's documented migration path; preserve rollback.
  Do not change existing website/apex proxy behavior for this connector.
- Confirm target Ubuntu architecture, systemd credentials support (247+),
  cgroup resource controls, and the existing loopback server. No new inbound
  origin ports are required. Parent owns outbound network/firewall review.
- Review effective remote ingress before starting: remote configuration is
  authoritative in token mode. It must exactly match the allowed paths and both
  404 rules; a dashboard hostname route without a path would expose audio.

## Pinned binary staging

Run as an unprivileged operator in a disposable writable staging directory.
The helper requires explicit version, digest, architecture and destination.
It creates no directories, refuses root and existing destinations, follows
only HTTPS GitHub release asset redirects, uses no inherited proxy/auth/cookies,
checks SHA-256 before publishing, and never installs or executes the result.
The 128 MiB download bound is separate from the connector's runtime budget.

From the repository root, for the reviewed Linux amd64 release:

```sh
python3 hybrid/deploy/download_cloudflared.py \
  --version 2026.11.0 \
  --sha256 51f7c68b57ed7269164075d7e789efce5b59355982f06392deeb037d982a51fd \
  --arch amd64 --dest ./cloudflared-2026.11.0
```

The official GitHub release/API was checked on November 15, 2026: version
`2026.11.0`, published November 12, 2026; amd64 asset 40,283,099 bytes.
The findings file also records the arm64 digest. Match the target architecture
and independently review the release before future version/hash updates.

## Preferred remotely managed connector

`slimeybeats-cloudflared.service` runs as the dedicated no-login
`slimeybeats-cloudflared` account described by
`slimeybeats-cloudflared.sysusers.conf`. The operator must review/create this
account without modifying the hybrid worker account. It needs no home or
writable application storage. No setup script here mutates the host.

After operator approval, install the verified binary at
`/usr/local/libexec/cloudflared` (root-owned, mode 0755, parent not writable by
the service); install the unit under `/etc/systemd/system/`. Put only the
named tunnel token in `/etc/slimeybeats-cloudflared/tunnel.token` (root:root,
0600, directory root:root 0700). Never commit it or paste it in a shell command,
unit argument, environment variable, log, screenshot or review artifact.
systemd `LoadCredential` supplies a private runtime copy; ExecStart references
`%d/tunnel-token` using the officially supported `--token-file` flag.

Parent creates/selects a remotely managed named tunnel and reviews/applies
`cloudflared-remote.json.example` as the configuration request body
(`PUT /accounts/{account_id}/cfd_tunnel/{tunnel_id}/configurations`). This file
contains no credentials and is not read locally by cloudflared. The account
token used for that administrative API call is distinct from the tunnel token
and must never reach the connector host. Parent alone handles zone activation
and the `api` DNS route to the named tunnel. Review the resulting configuration
again if dashboard routing actions add or reorder ingress rules.

The unit disables self-update, binds metrics only to `127.0.0.1:20241`, sets
no capabilities and `NoNewPrivileges`, protects the filesystem/home/devices,
and bounds CPU, memory, swap, tasks and restarts. Its initial 100% CPU,
128 MiB MemoryHigh and 256 MiB MemoryMax are preparation budgets, not measured
capacity. Info logs avoid the documented debug header logging risk. No token
or `cert.pem` belongs in Environment/ExecStart. Retain an operator-owned update
process for security releases because auto-update is disabled.

## Locally managed alternative

Use this mode only with a locally managed named tunnel and its tunnel-scoped
credentials JSON. Install `cloudflared.yml.example` as
`/etc/cloudflared/config.yml` after replacing the UUID/path. Make the directory
root:slimeybeats-cloudflared 0750 and config/credentials root:slimeybeats-cloudflared
0640. Do not place account-wide `cert.pem` on the service host. Install
`slimeybeats-cloudflared-local.conf.example` as
`/etc/systemd/system/slimeybeats-cloudflared.service.d/local.conf`; it clears
remote token loading and the old startup command, then validates local ingress
before running. The local YAML preserves the same control-only policy.

## Preparation and acceptance evidence

From the repository root, offline checks require no owner credentials/network:

```sh
python3 hybrid/deploy/validate_deployment.py offline
python3 -m unittest discover -s hybrid/deploy/tests -v
```

The checks validate the shipped fixed formats; they do not prove effective
remote ingress, systemd semantics or a live deployment. On Ubuntu the operator
must separately review `systemd-analyze verify` for the prepared unit/effective
drop-in, `systemd-analyze security` and effective resource limits. Confirm the
installed binary version, credential file permissions, service identity,
loopback-only origin/metrics, and secret-free argv/logs without publishing
credentials. In local mode also run `cloudflared tunnel --config ... ingress
validate` and `ingress rule` for the three allowed URLs and blocked upload,
artifact, job, MCP and foreign-host URLs. Token mode requires reviewing actual
remote ingress; validating local YAML says nothing about remote configuration.

After parent-controlled activation, check allowed endpoints and confirm 404
for `/v1/jobs`, `/v1/jobs/<id>/upload`, `/v1/jobs/<id>/artifacts/<id>`, `/mcp`,
and unknown paths, without submitting audio. Do not send a bearer token to a
redirect or unapproved endpoint. The existing `health` validator checks the
full private engine (including upload preflight); a restricted public route
intentionally cannot satisfy that full-audio check. Keep running that validator
against loopback for private-engine acceptance. Account/DNS/server operations,
systemd runtime verification, public acceptance and audio-storage integration
are not established by the offline tests.
