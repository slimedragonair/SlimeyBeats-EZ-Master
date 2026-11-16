# Hybrid backend

Run from the repository root with Python 3.10 or later:

```text
python -m hybrid.server --cli ABSOLUTE_NATIVE_CLI --data-dir ABSOLUTE_PRIVATE_STORAGE --host 127.0.0.1 --port 8787
```

Set `SLIMEYBEATS_HYBRID_TOKEN` to a secret of at least 32 printable ASCII
characters. `SLIMEYBEATS_HYBRID_ORIGINS` is an optional comma-separated list of
exact HTTP(S) web origins, without trailing slashes. Public guest processing is
disabled by default. The owner bearer keeps its existing protocol and can manage
all jobs, including guest jobs. Never place that owner secret in a public website,
browser build, URL, or browser storage. Browser preflight is unauthenticated and
restricted to configured origins, methods and headers. CORS never enables cookie
credentials. `GET /health` returns only `{"status":"up"}`.

The explicit `--cli` must name the built native executable. `--ffmpeg` can select
a native codec executable; otherwise startup discovers it beside the CLI or on
the operator's PATH. Keep CLI/codec packages under operator control. MP3 output
also needs the native CLI's own FFmpeg discovery to work. Native work uses
`--threads 4`; one processing worker runs at a time.

## Public browser sessions and availability

The operator must explicitly set `SLIMEYBEATS_HYBRID_PUBLIC=1` to enable guests;
all other values leave access private. Configure the website's exact origin in
`SLIMEYBEATS_HYBRID_ORIGINS`. Public ingress, TLS and encrypted relay transport are
managed outside this server. Keep the public flag off until the relay is ready;
configuring IP trust does not enable guest processing. Enabling guests still
requires the private owner secret at startup.

`GET /v1/status` is the exact unauthenticated availability route. It accepts no
query string and requires neither a cookie nor bearer, even with public access
disabled. Its entire response is:

```json
{"api_version":1,"status":"online","guest_access":true}
```

`status` is `disabled` with `guest_access:false` when public access is off;
otherwise it is `busy` when all global retained-job slots are occupied, or
`online`. `guest_access` remains true while busy. It exposes no job IDs, owner
configuration, capacity counts, paths, or session data. Configured CORS applies,
and requests without an Origin remain readable. Other status methods or route
variants do not bypass authentication.

With public access enabled, `POST /v1/session` requires a configured `Origin` and
an empty JSON object with `Content-Type: application/json`. No bearer is needed.
It returns HTTP 201 and `{"token":"OPAQUE_GUEST_TOKEN","expires_at":"ISO8601_UTC"}`.
Send that token only as `Authorization: Bearer OPAQUE_GUEST_TOKEN` on the existing
capabilities/job/upload/run/artifact/delete API. Tokens in queries are never
accepted. Keep guest tokens in browser memory; the backend retains only their
SHA-256 digests. Sessions have a fixed one-hour maximum lifetime, are not renewed
by use, and vanish on restart. Missing, unknown and expired bearers return 401.
Session creation returns 403 while public access is disabled.

Each guest may retain one job, and every job operation checks its creating
principal. A known foreign job or artifact ID grants no access and returns 404.
Guests cannot list jobs. Owner requests can manage all jobs. Idempotency keys are
namespaced by principal; retries never claim a job from another session. Guest
audio/job expiry is clamped to session expiry, including queued/running jobs and
downloads; the existing expiry sweeper cancels work and removes owned files.

Guest limits are deliberately small: 64 live sessions; four session creations
per IP and 64 globally per 10-minute window; three starts per session, eight per
IP and 32 globally per hour. Only the first successful run transition counts;
idempotent run retries do not. Sessions and jobs share the existing four retained
jobs and one worker. Owner processing bypasses guest rate limits, but still obeys
the global job/storage bounds. Rate and capacity failures return 429 with an
integer-seconds `Retry-After`, exposed through CORS. Deleting a job frees its slot
but does not reset a guest's rate budget. Session expiry also leaves rate windows
intact. There are at most 256 watched IP/operation windows plus two global
counters; live windows are never evicted to accept rotated IPs. IP metadata is
HMAC-hashed in memory with a process-secret nonce, never persisted or logged.

Client identity uses the socket peer by default, ignoring `X-Forwarded-For`,
`CF-Connecting-IP`, and `X-SlimeyBeats-Client-IP`. For an operator-controlled Caddy
relay connected through an authenticated, encrypted reverse SSH tunnel, use:

```text
--host 127.0.0.1 --port 8787 --expected-host api.slimedragon.net --trust-relay-client-ip
```

The relay must overwrite `X-SlimeyBeats-Client-IP` from its observed `remote_host`,
discarding any client-supplied value. Its private loopback forward must carry the
request to backend `127.0.0.1:8787` while preserving Host `api.slimedragon.net`.
Protect both ends of the forward and its SSH credentials; the backend cannot
authenticate the original internet peer through the loopback socket. Relay mode
uses only `X-SlimeyBeats-Client-IP`; Cloudflare and forwarded-for headers are
ignored. This mode is provider-neutral and does not set up the relay or tunnel.

The existing Cloudflare mode is a separate, mutually exclusive opt-in:

```text
--host 127.0.0.1 --expected-host api.slimedragon.net --trust-cloudflare-client-ip
```

Either mode is refused unless the listener is loopback and the expected host is exactly
`api.slimedragon.net`. The expected-host option pins Host on every route, including
health/preflight; local probes must send that Host. Guest session creation and
job starts additionally require a loopback peer and exactly one valid IP in
the selected mode's header. Invalid, duplicate, comma-separated, missing or
zone-scoped addresses fail closed. Cloudflare mode uses only `CF-Connecting-IP`,
which Cloudflared must overwrite, and ignores the relay header. Trusted local
processes share this boundary. Do not expose the origin listener or enable a
trust mode behind an arbitrary forwarder. Both flags together are rejected by
CLI parsing and configuration validation. Forwarded-for headers are never used.
All `SLIMEYBEATS_HYBRID_*` environment variables, including
the owner secret and public feature flag, are stripped before any child runs.

## Responsibilities and flow

- `config.py` owns defaults, operator limits and the 16 built-in profile IDs.
- `http_server.py`, `http_auth.py`, and `http_deadline.py` implement routing,
  constant-time bearer comparison, exact CORS, bounded connections, and an
  absolute deadline across header/body reads, including trickling clients.
- `principals.py`, `guest_sessions.py`, and `client_identity.py` own owner/guest
  identities, token digests, bounded rate windows and the explicit proxy boundary.
- `requests.py` accepts only the public mastering fields. Filenames select
  WAV/MP3 upload extensions and are discarded; they never become local paths.
- `jobs.py` serializes state, exact append offsets, retained-job/upload
  reservations, queueing, cancellation, download leases, and expiry.
- `storage.py` exclusively locks a private marked root. Restart removes only
  marked job directories belonging to this backend and never resumes jobs.
- `audio_guard.py` checks complete RIFF PCM/float WAV layout, finite float
  samples, sample rate/channels, frames, duration and decoded size.
- `decoder.py` rebuilds metadata-free WAV input or forces MP3 demuxing with
  `file,pipe` protocols into bounded stereo 48 kHz PCM. The CLI sees WAV only.
- `worker.py`, `processes.py`, and `child.py` run and reap owned subprocess
  trees, drain bounded logs, enforce watchdog/disk/resource limits, and publish
  only the fixed audio output plus `reports.py`'s numeric report projection.

Snapshots keep the v1 contract: `id`, `state`, `uploaded_bytes`, `total_bytes`,
`progress` (0–1), `message`, `expires_at` (ISO 8601 UTC), and `artifacts`.
Artifact names are generic ASCII and their IDs are opaque. Optional creation
header `Idempotency-Key` accepts 16–128 ASCII letters, digits, `_` or `-`.
An identical retry returns the same retained job; changed fields return 409.
Keys expire with their jobs. Run requests are also idempotent.

Defaults are 512 MiB uploads, 1 MiB chunks, four retained jobs of any state,
one-hour retention, a 512 MiB decoded f32 bound, and 1,800 seconds of audio.
At stereo 48 kHz the decoded bound limits MP3 to about 1,398 seconds; WAV's
duration also depends on rate/channels. Capability limits expose both bounds.
All upload reservations count until deletion/expiry. A new job reserves its
configured disk budget (default 2 GiB), with a 64 MiB free-space margin.
Output directory size is monitored while processing and file writes fail
explicitly; this monitoring is not an operating-system filesystem quota.

Linux/Unix children inherit memory, CPU, file-size, core and descriptor limits.
Linux also protects the isolated process group on parent death. Windows uses a
private kill-on-close, memory-bounded Job Object for each supervisor and its
descendants. Non-loopback binding is refused outside Linux. DELETE waits for
the owned child and active downloads before removing audio. Streaming checks
cancellation/expiry between blocks, and blocked writes have a socket timeout.

## Validation

```text
python -m hybrid.tests --cli ABSOLUTE_NATIVE_CLI [--ffmpeg ABSOLUTE_FFMPEG]
```

This requires a real native executable and runs HTTP, hostile-input, lifecycle,
storage, process and audio smoke regressions. Discovery also works with
`python -m unittest discover -s hybrid/tests`; set `HYBRID_TEST_CLI` and optionally
`HYBRID_TEST_FFMPEG`. CI fails rather than silently skipping a missing native CLI
or missing FFmpeg. Unix resource assertions are skipped on Windows. Native
smoke covers WAV24/MP3 output, MP3 input, float WAV, report/download/deletion,
and oversized/disguised MP3 rejection. Public regressions mint two guest tokens,
run actual native jobs, verify isolated status/exports/cancellation and source
privacy, and exercise expiry, flood/capacity limits, proxy trust, absolute session
body deadlines and slow downloads. Browser, ingress, physical listening,
and deployment acceptance are separate checks owned by the integrating caller.
