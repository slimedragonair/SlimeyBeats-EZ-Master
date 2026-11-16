# Funnel connector operator notes

The dedicated unprivileged `slimeybeats-funnel` daemon uses userspace networking;
the backend stays on `127.0.0.1:8787`. The operator manages login, node authorization,
Funnel activation, rollout and live checks. No VPS, Cloudflare, R2 or DNS change
is required by this selected setup. This source unit is not activation evidence.

Preserve **both** arguments during updates:

- `--state=/var/lib/slimeybeats-funnel/tailscaled.state` preserves the node identity.
- `--statedir=/var/lib/slimeybeats-funnel` explicitly establishes writable persistent
  config/certificate storage. HTTPS has failed with `no TailscaleVarRoot` in this
  configuration when only the state-file argument was supplied.

`StateDirectoryMode=0700`, `RuntimeDirectoryMode=0700` and `UMask=0077` protect
identity/certificate files and `/run/slimeybeats-funnel/tailscaled.sock`. The
service account needs the statedir; nobody else needs its contents. Never print,
commit, move or delete the node state to fix a connector error. Keep authorization
keys and account credentials out of the unit, argv, logs and public artifacts.
Use operator-approved verified static binaries at the paths in the unit. There is
no download, install, login or restart helper here.

After the dedicated daemon is installed and running, the operator can authorize
the node and configure its Funnel target with these manual commands:

```sh
sudo -u slimeybeats-funnel /usr/local/libexec/slimeybeats-funnel/tailscale \
  --socket=/run/slimeybeats-funnel/tailscaled.sock up \
  --accept-dns=false --accept-routes=false --ssh=false

sudo -u slimeybeats-funnel /usr/local/libexec/slimeybeats-funnel/tailscale \
  --socket=/run/slimeybeats-funnel/tailscaled.sock funnel \
  --bg --https=443 http://127.0.0.1:8787
```

Complete any required account authorization and Funnel policy approval in the
operator's trusted session. These settings decline Tailscale DNS/subnet-route
configuration and keep Tailscale SSH disabled. Publish only the backend's
loopback HTTP listener; do not substitute a shell, MCP endpoint or unrelated
service. Review existing configuration before changing an already authorized
node rather than resetting its state.

Record the device's node-key expiry policy in the admin console and arrange
reauthentication before expiry. Persistent state does not prevent authorization
expiry; expired node keys interrupt connectivity. If expiry is disabled for
unattended operation, record that explicit security decision. Reauthentication
can interrupt connectivity, so retain an independent administrative access path.

From the repository root, offline source checks are:

```sh
python3 hybrid/deploy/tailscale/funnel_scaffold.py
python3 -m unittest discover -s hybrid/deploy/tailscale/tests -v
systemd-analyze verify --man=no hybrid/deploy/tailscale/slimeybeats-funnel.service
```

Run the last check on the target with its installed executable. A source-only
machine can check a temporary unit copy with just ExecStart's executable path
mapped to the staged identical binary; label that scope explicitly. These checks
do not start the daemon, inspect private state or establish HTTPS availability.

After an operator-controlled restart, verify the node identity stayed the same,
the daemon runs as its dedicated account, persistent/runtime permissions and
effective CPU/memory limits match the unit, and HTTPS reaches the loopback API.
Confirm no public MCP or unrelated services, no owner key in the public browser,
exact CORS, guest isolation and rejection after expiry. The kernel/resource and
live forwarding checks are separate from source validation. Acceptance must also
cover these distinct states in a controlled maintenance window:

- Backend running with public guests disabled: `/v1/status` reports `disabled`
  with `guest_access:false`; session creation returns 403. The browser displays
  the disabled state and does not upload audio.
- Backend stopped: the browser displays server unavailability while retaining
  local work. After one controlled restart and readiness check, connection
  recovery must not automatically resubmit audio. Avoid rapid restart cycles
  that exhaust the unit's systemd start limit.
- Expired or deleted guest job: the browser offers explicit forget/reconnect
  recovery without exposing another guest's job or replaying the source upload.

Keep both backend proxy-header trust flags **off** for this Funnel deployment.
The current loopback peer supplies one conservative shared guest rate bucket;
do not claim visitor-specific IP limiting or trust arbitrary forwarding headers.
Connector startup does not itself enable public guest processing. The operator manages
the backend public flag and the explicit browser-processing switch separately.

Official references: [userspace networking](https://tailscale.com/kb/1112/userspace-networking),
[Funnel](https://tailscale.com/kb/1223/funnel),
[tailscaled flags](https://tailscale.com/kb/1278/tailscaled),
[CLI network preferences](https://tailscale.com/kb/1080/cli),
and [node-key expiry](https://tailscale.com/kb/1028/key-expiry).
The staged `tailscaled` 1.104.7 `--help` describes `--statedir` as storage for
configuration state and TLS certificates; the source checker requires it explicitly.
