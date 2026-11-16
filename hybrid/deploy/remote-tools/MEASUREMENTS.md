# CPU neural runtime verification — November 16, 2026

This public summary contains relevant hardware, artifact provenance and measured
results. Full operator paths, account/process identities and filesystem inventory
remain in private evidence outside the repository. These are synthetic execution
checks, not vocal-isolation quality scores or hardware-audio acceptance.

## Hardware and limits

The Linux test host used an AMD EPYC 7551P with 32 physical cores / 64 logical
CPUs and approximately 189 GiB RAM. Inference explicitly ran on CPU. GPU use was
not qualified. Each measurement used a separate transient user cgroup, rather
than changing the active backend: 32 GiB maximum memory, 256 tasks and the CPU
quota shown below. API 2 tests additionally set memory high to 24 GiB. A proposed
deployment profile is CPU quota 800%, memory high 24 GiB / max 32 GiB, no swap and
one globally serialized heavy job. It is a proposal, not evidence of deployment
or maximum-size workload qualification.

## Installed engine and provenance

The optional environment used Python 3.11.14, uv 0.9.22, Spleeter 2.4.2,
TensorFlow 2.12.1, NumPy 1.24.3 and SciPy 1.15.3. Both two- and four-stem modes
used the same upstream v1.4.0 checkpoint, with archive SHA-256
`3adb4a50ad4eb18c7c4d65fcf4cf2367a07d48408a5eb7d03cd20067429dfaa8`.

The existing core installer completed in 52.08 seconds, including downloads and
real neural smoke inference. Its maximum individual-process resident size was
1,113,452 KiB; this is not aggregate cgroup memory. A repeated setup invocation
verified installed-model provenance and file hashes without another download.

The measured API 2 debug worker hash was
`fa80d86c16ab712abc6ffa489f33a6b8bfe59fbf398480e650bd4c16b6bae14e`.
The separately measured release worker hash was
`0bc76b972a46fc6550e7428cb617b75df58ffaa4b3e22f61eb38895f91ac3a75`.
These identify measured artifacts, not a claim about the currently deployed
backend. The shared core/helper source snapshot was based on `be3fcde`.

Generic operator layout examples are `/srv/slimeybeats/remote-tools/runtime`,
`runtime/env/bin/python` and `runtime/models/4stems`. Preserve the virtual
environment executable's invocation path after validating its resolved target;
executing the resolved base interpreter can bypass installed packages.

## Actual neural measurements

All runs used one deterministic 24-second, 44.1 kHz stereo PCM16 fixture:
1,058,400 frames / 8,467,200 decoded Float32 bytes. No source downsampling or
truncation occurred. Each run crossed three 12-second windows with two-second
overlap. The 200% runs retained the same 32 GiB cap to isolate CPU quota; they do
not establish a failure or pass at the old backend's 4 GiB aggregate cap.

| Execution path / CPU quota | Mode | Wall seconds | Aggregate peak bytes | Sampled peak tasks | Reconstruction RMS |
|---|---|---:|---:|---:|---:|
| Core example / 800% | two stems | 18.447 | 968,265,728 | 12 | 4.88e-11 |
| Core example / 800% | four stems | 19.359 | 1,025,548,288 | 12 | 6.57e-9 |
| Core example / 200% | two stems | 21.797 | 987,119,616 | 12 | 4.88e-11 |
| Core example / 200% | four stems | 22.585 | 1,018,523,648 | 12 | 6.57e-9 |
| API 2 debug / 800% | two stems | 18.561 | 996,040,704 | 16 | 3.35e-9 |
| API 2 debug / 800% | four stems | 19.150 | 1,032,552,448 | 16 | 5.78e-9 |
| API 2 debug / 800%, 4 GiB AS | two stems | 18.349 | 1,001,541,632 | 16 | 3.35e-9 |
| API 2 debug / 800%, 4 GiB AS | four stems | 18.851 | 1,068,371,968 | 16 | 5.78e-9 |
| API 2 release / 800%, 32 GiB AS | two stems | 17.363 | 1,020,510,208 | 16 | 3.35e-9 |
| API 2 release / 800%, 32 GiB AS | four stems | 17.363 | 994,820,096 | 16 | 5.78e-9 |

AS denotes the separate per-process address-space limit. Additional constrained
runs used 64 open files and 900 CPU seconds. All ten runs had zero OOM or memory
max events. The 200% runs recorded 90 / 89 throttled periods; 800% recorded zero.
These are individual observations, not p95/p99 or a general throughput ratio.

Independent SciPy decoding checked finite Float32 output, exact geometry, stem
roles/identities, resummation and hashes. Release per-channel peak metadata matched
decoded samples; corresponding output WAV hashes matched the earlier debug runs.
The fixture produced little estimated vocal energy, so it does not qualify
singing or speech isolation quality.

## Scope and remaining acceptance

Neural admission remains **512 MiB decoded PCM per tool**. Generic 2 GiB request
admission does not expand that cap. Do not downsample originals to fit it.
Maximum-size mastering, 2 GiB neural jobs, retained multi-project disk usage,
concurrent sessions, long-session stability and GPU inference are unqualified.

The staged backend separates v2 defaults: 16 GiB address space, 14,400 CPU seconds,
3,600 worker wall seconds and 300 decode seconds. These source settings are not
an applied service override or a maximum-size benchmark. Keep global concurrency,
combined disk admission and retention bounded, and verify the final deployed
worker, service-identity permissions and complete HTTP workflow.

Two Linux supervisor regression methods were reported passing in WSL for nested
process groups, detached sessions and cleanup after native success. Actual
deployed HTTP neural cancellation remains a separate acceptance check.

For matched browser/native Analyze and Master timings, see [BENCHMARK.md](BENCHMARK.md).
