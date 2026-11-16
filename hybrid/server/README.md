# Private hybrid backend

Run from the repository root with Python 3.10 or later:

```text
python -m hybrid.server --cli ABSOLUTE_NATIVE_CLI --data-dir ABSOLUTE_PRIVATE_STORAGE --host 127.0.0.1 --port 8787
```

Set `SLIMEYBEATS_HYBRID_TOKEN` to a secret of at least 32 printable ASCII
characters. `SLIMEYBEATS_HYBRID_ORIGINS` is an optional comma-separated list of
exact HTTP(S) web origins, without trailing slashes. Every actual `/v1` request
requires the bearer token; browser preflight is unauthenticated and restricted to
configured origins, methods and headers. CORS never enables cookie credentials.
`GET /health` returns only `{"status":"up"}`. This is an owner/invite preview.
Remote access needs the operator's private ingress and TLS configuration.

The explicit `--cli` must name the built native executable. `--ffmpeg` can select
a native codec executable; otherwise startup discovers it beside the CLI or on
the operator's PATH. Keep CLI/codec packages under operator control. MP3 output
also needs the native CLI's own FFmpeg discovery to work. Native work uses
`--threads 4`; one processing worker runs at a time.

## Responsibilities and flow

- `config.py` owns defaults, operator limits and the 16 built-in profile IDs.
- `http_server.py`, `http_auth.py`, and `http_deadline.py` implement routing,
  constant-time bearer comparison, exact CORS, bounded connections, and an
  absolute deadline across header/body reads, including trickling clients.
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
and oversized/disguised MP3 rejection. Browser, ingress, physical listening,
and deployment acceptance are separate checks owned by the integrating caller.
