# Native worker validation, November 16, 2026

The no-default-feature, all-target baseline passed on Windows and WSL before
implementation. Initial all-target runs passed 273 tests on Windows and 279 on
WSL, with two existing ignored tests on each platform. The worker contributes
13 Windows tests and 14 Linux tests, including the Unix symlink regression.
The subsequent stem-root parity correction passed 14 focused worker tests on
Windows and 15 on Linux, including bit-exact raw-sum PCM, guarded full-bank
preview, primary selection and full-sum export. Actual binaries passed 27 HTTP
checks on each platform after that correction. Stem responses now include
original + preview alongside the individual stem assets. Prior one-hour checks
below precede this bank-only correction.
The other agent's `hybrid_stem_setup` binary is included in the final all-target
run and has no tests. Existing CLI, library, Cargo manifest and lockfile were
not changed by this implementation.

Worker tests cover serialized settings/reference dimensions, resource limits,
path containment, output collisions, right-only waveform/channel peaks,
immutable raw imports, neutralized user EQ during mastering, shared EQ parity
without remastering, voice/Doctor/spectral parity and invalid parameters, mix
solo/mute/balance/layout, spectrogram/region edits, shared suggestions, and
original names/kind overrides for opaque stem upload paths.

Actual binary checks use `verify_worker.py`, which runs a private loopback HTTP
parent and invokes a new process for each command. The current Windows debug
worker passed 26 checks. The current WSL release worker passed 30 checks,
including all six final encodes, suggestions, edits, ZIP/loose/explicit stem
imports, finite metadata, fixed progress messages, failed commands leaving no
success response, and unchanged source bytes. FFprobe independently checked
the final sample rate and channels when available.

The Linux long fixture is **3600 seconds, 8000 Hz, mono**, not a 2 GiB fixture.
Import and analysis pass at the exact duration bound and fail at a bound 0.01
seconds shorter. Mastering also passes, retaining 28,800,000 frames and 8000 Hz,
with `safety:false` and `intensity:0`. This verifies server admission and a long
shared-engine path; it does not qualify maximum-memory jobs, every rate/channel
combination, default-intensity long mastering, or subjective sound quality.

```text
cargo test --locked --no-default-features --all-targets --manifest-path rust/Cargo.toml
cargo clippy --locked --no-default-features --bin slimeybeats_hybrid_worker --manifest-path rust/Cargo.toml -- -D warnings
rustfmt --check --edition 2024 rust/src/bin/slimeybeats_hybrid_worker.rs
python rust/src/hybrid_worker/verify_worker.py --binary ABS_WORKER
python rust/src/hybrid_worker/verify_worker.py --binary ABS_WORKER --long-seconds 3600 --long-master
```

Release builds and strict Clippy passed on both platforms. Windows artifacts use
`D:\Caches\Rust\SlimeyBeats-20261115\{debug,release}\slimeybeats_hybrid_worker.exe`.
WSL release is
`/home/logan/.cache/slimeybeats-ez-master-verification/hybrid-worker/release/slimeybeats_hybrid_worker`.
The WSL Cargo/Rustup caches are under `/tmp/slimeybeats-linux-20261112-493c`.

Release SHA-256:

| Platform | SHA-256 |
|---|---|
| Windows | `bbc5362dc623599b758b21bd3e7f8f379cc154fa60a27a05f796b00e08eed4b5` |
| Linux | `37fd55fc09201f0d4397ecd784c03bf1fbf65c2e05921c89f2f674247ec2c8ff` |

The runtime agent separately reports actual worker-to-model runs on the server
using the prior `0bc76b97...` release before stem-root additions: 24 seconds,
44.1 kHz stereo, 1,058,400 frames; both
two and four stems complete in approximately 17.36 seconds. Aggregate cgroup
peaks are 1,020,510,208 / 994,820,096 bytes, 16 tasks, zero OOM or throttling under
8 CPU / 24 GiB MemoryHigh / 32 GiB MemoryMax, 32 GiB address-space and 64-file
limits. Independent SciPy decoding checked every asset's channel_peaks against
the actual WAV; output WAV hashes match the prior debug qualification, which
measured sum RMS errors 3.35e-9 / 5.78e-9 and finite, aligned float PCM. Runtime
limits remain **512 MiB decoded input**, explicitly reported separately from
generic 2 GiB / one-hour source admission. This is a small synthetic neural
fixture, not a maximum-size or isolation/artifact-quality qualification.

Full browser/phone acceptance, backend publication, and descendant-tree kill
qualification belong to the integration agents. Shared native codec/inference
helpers create their own Unix process groups: the supervisor must terminate
the entire owned descendant tree/session, including nested groups. Killing only
the initial process group does not establish cancellation. No service deployment,
commit or push was performed by this worker implementation.
