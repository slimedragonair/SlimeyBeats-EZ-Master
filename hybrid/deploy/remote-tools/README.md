# Private Linux heavy-tool runtime

This folder supports the optional server-side model runtime. It does not install
the backend, publish browser assets, or restart the backend or Funnel. The
browser uploads originals into a private v2 project; the server worker receives
generated private paths and returns WAVs for opaque asset IDs and streamed
previews. Model setup is an operator action, never an HTTP operation.

## Installed layout and provenance

The November 16, 2026 verification used this dedicated secondary-disk tree:

| Purpose | Absolute path |
|---|---|
| Dedicated operator tree | `/mnt/exparements/slimeybeats-hybrid-preview/remote-tools` |
| Core runtime base | `/mnt/exparements/slimeybeats-hybrid-preview/remote-tools/runtime` |
| Python invocation | `/mnt/exparements/slimeybeats-hybrid-preview/remote-tools/runtime/env/bin/python` |
| Model directory | `/mnt/exparements/slimeybeats-hybrid-preview/remote-tools/runtime/models/4stems` |
| Core receipt | `/mnt/exparements/slimeybeats-hybrid-preview/remote-tools/runtime/engine.json` |

Installed core versions: Python 3.11.14, uv 0.9.22, Spleeter 2.4.2,
TensorFlow 2.12.1, NumPy 1.24.3, SciPy 1.15.3. The existing core installer pins
the upstream v1.4.0 four-stem archive to SHA-256
`3adb4a50ad4eb18c7c4d65fcf4cf2367a07d48408a5eb7d03cd20067429dfaa8`.
Both modes use that checkpoint: vocals/instrumental, or vocals/drums/bass/other.
Transitive dependency versions are captured in the private audit; this does not
add a new fully locked Python dependency graph or relicense the upstream model.

The helper is automatically discovered by Cargo:

```sh
cargo build --locked --manifest-path rust/Cargo.toml --no-default-features --bin hybrid_stem_setup
hybrid_stem_setup --base /mnt/exparements/slimeybeats-hybrid-preview/remote-tools/runtime
hybrid_stem_setup --base /mnt/exparements/slimeybeats-hybrid-preview/remote-tools/runtime --probe
```

`--base` is required and must be absolute and below a filesystem root. The core
installer preserves unrelated nonempty folders, owns its installation marker,
serializes setup and verifies archive/model integrity. Setup emits bounded
progress on stderr and one operator JSON result on stdout. A missing `--probe`
runtime exits nonzero without installation. `--probe` checks readiness only;
running setup again performs the core's full installed-model integrity check.

Keep setup, sources, build artifacts, Cargo downloads, managed Python, model
downloads and temporary files on the secondary mount. The private staging
workflow uses an isolated `CARGO_HOME`/`CARGO_TARGET_DIR`, four Cargo build jobs,
and secondary `TMPDIR`. A minimal Rust source snapshot must also include
`mastercraft/data/builtin_profiles.json`, embedded by the shared core. Never
install into the system Python, root filesystem, or an existing desktop runtime.

## Worker integration and permissions

The backend operator fields are `studio_cli`, `stem_python`, and `stem_models`.
Their values must come from operator configuration; reject browser overrides,
arbitrary install operations and caller-selected executable/download paths.
Public capabilities expose readiness, modes and per-tool limits, never these
private filesystem paths. Preserve the `env/bin/python` spelling when validating
the executable: resolving its symlink for invocation bypasses `pyvenv.cfg` and
can select a base interpreter without Spleeter. Hashing/validating the resolved
ELF target is fine; execute the original virtual-environment path.

Native integration uses the existing API:

```rust
StemRuntime::configured(python: PathBuf, models: PathBuf)
    .separate(&Audio, SeparationMode, &Path, &dyn Fn(f32, &str), Arc<AtomicBool>)
    -> anyhow::Result<Separated>
```

`Separated` contains `stems` and `metadata`. `SeparationMode` is
`VocalInstrumental` or `FourStems`. The API 2 worker handles request validation,
decoding, exports and responses; see `rust/src/hybrid_worker/README.md`.
Use `StemRuntime::configured` for the immutable deployment: conversion from
`RuntimePaths` opens `engine.lock` for writing. The backend must serialize all
v1/v2 heavy jobs together, including model inference, rather than relying on
that optional lock or starting a second worker pool.
For cancellation, account for neural children that start a separate process
group. Killing only the first supervisor group is insufficient. The backend
owns descendant/session termination. Its two Linux regression methods were
reported passing in WSL for nested groups, detached sessions and helpers left
after native success. Actual deployed HTTP neural cancellation still requires
acceptance; these fixture results do not establish that live-service boundary.

The installed runtime has recursive named-user read/traverse/execute ACLs for
`slimeybeats-hybrid` (UID 997); it has no model/runtime write permission. The
operator tree grants traversal only to other users. Sources, logs and tooling
remain private. The venv's managed interpreter and libraries are on the same
secondary mount; its interpreter target also has the service ACL. Actual
service-UID execution is a separate deployment acceptance check. The private
`sudo_readonly_probe.py` is ready for that check with hidden `getpass`, pinned
SSH and encrypted stdin; it was not run in this task. No sudo password was
requested, printed, saved or placed in an argument.

Retain `ProtectSystem=strict`, `ProtectHome=yes`, network restrictions and the
existing writable project/runtime-state paths. Add only the read-only runtime
mount from `heavy-tools.conf.example` when the parent deploys the new backend.
Run setup as the operator outside the backend sandbox; inference needs no model
downloads and must not gain arbitrary outbound access.

## Resources and admission limits

`inventory.py` reads CPU, available RAM, GPU, root/secondary disk, pressure,
Python and selected systemd unit properties. It excludes environment files,
tokens and service command environments. `resource_plan.py INVENTORY_JSON`
produces a proposal and never applies it. See `MEASUREMENTS.md` for host and
actual model measurements. `heavy-tools.conf.example` proposes eight CPUs of
quota, 24 GiB memory high, 32 GiB memory max, no swap, 256 tasks, four Rayon/
TensorFlow intra-op threads, one TensorFlow inter-op thread and one OpenBLAS
thread. It is not installed by these tools. Re-inventory before applying it.

The current Python separation worker uses 12-second windows, two-second
overlap, memmapped input/four outputs and chunked normalization. The native
adapter still owns full-length source/stem/projection buffers. Neural input is
**512 MiB decoded Float32 PCM per tool**, not encoded file size. The v2 worker's
2 GiB generic import/request admission and 4 GiB aggregate bank bounds do not
expand this neural limit. Do not silently resample/downsample or alter originals
to fit. Larger neural inference remains unverified and needs an explicitly
scoped streaming adapter, request bounds, disk/memory accounting and actual
regressions before changing advertised limits.

Aggregate cgroup RAM limits and the supervisor's per-process `RLIMIT_AS` are
different. Configure the v2 child address-space budget together with the unit;
a 32 GiB service cgroup does not remove an inherited 4 GiB virtual-memory limit.
Keep the v1 defaults separate and verify the real supervisor path. The proposed
32 GiB profile provides host headroom; small synthetic tests do not qualify
maximum-size 2 GiB mastering, imported banks or long-session retention.
Choose and verify v2 `cpu_seconds` and `worker_timeout_seconds` together with its
audio admission limits. The installed v1 supervisor's 900 CPU-second / 900
wall-second defaults are independently enforced; the unit's larger CPU quota
does not extend them. The staged backend now separates v2 defaults: 16 GiB
per-process address space, 14,400 CPU seconds, 3,600 worker wall seconds and
300 decode seconds. These source defaults have not been applied to the active
service by this runtime task and do not qualify maximum-size processing.

Budget private project storage across retained originals, every immutable edit,
four stem outputs, normalization/scratch, previews and final encodes. With about
122 GiB secondary space remaining after installation, the planner proposes about
61 GiB of global active project storage; enforce a free-space floor and
combined retention/admission across v1/v2. Per-project 16 GiB alone is not a
global quota. An application watchdog is not an ext4 filesystem quota.

## Reproduce the checks

```sh
python3 hybrid/deploy/remote-tools/inventory.py
python3 hybrid/deploy/remote-tools/runtime_audit.py --base ABS_RUNTIME
python3 hybrid/deploy/remote-tools/benchmark.py --worker --base ABS_RUNTIME --probe ABS_WORKER --output NEW_ABS_PRIVATE_PROJECT --seconds 24 --cpu-quota 800 --memory-max 32G
python3 -m unittest discover -s hybrid/deploy/remote-tools/tests -p 'test_*.py'
```

Benchmarking requires cgroup v2 and the operator's user systemd manager. It
creates uniquely named transient scopes, verifies the actual CPU quota, records
aggregate `memory.peak`, task samples, CPU throttling and memory/OOM events, and
stops only its own scope on timeout. It never edits or restarts the live unit.
Without `--worker`, `--probe` selects the existing `stem_probe` example. The
independent output checker uses the installed environment's NumPy/SciPy to decode
every worker Float32 WAV, check finite samples, frame/rate/channel preservation,
stem IDs, hashes, reconstruction RMS and any supplied per-channel peak metadata.
Synthetic signals prove execution and
these contracts; they do not establish vocal-isolation quality or hardware audio.

The private Windows SSH tooling uses only the pinned Ed25519 key, a known-hosts
file and reject-unknown policy; agent/key discovery is disabled. Noninteractive
commands cannot obtain sudo credentials. Root-only deployment is a later parent
action, with any password passed only from hidden `getpass` over encrypted SSH
stdin. Do not use SSH passwords, accept-new host keys, public artifact URLs,
command-line secrets or environment-file dumps. No backend/Funnel stop, restart,
daemon reload, service environment change, commit or push occurred here.
