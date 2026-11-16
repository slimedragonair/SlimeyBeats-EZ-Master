# Matched Analyze / Master benchmark — November 16, 2026

On these synthetic fixtures, the eight-thread server completed Master in
**3.326 / 6.499 seconds** for 30 / 60 seconds of audio, compared with the browser's
**4.403 / 9.873 seconds**. This is **1.32x / 1.52x browser throughput** using median
worker wall times. Four threads were
slightly slower than the browser for the 30-second Master. Analyze was effectively
tied at 30 seconds with eight server threads; four server threads were slower
for both Analyze clips. This is not a general “super-server speedup” claim.

## Machines and execution paths

- Browser PC: AMD Ryzen 9 5950X, 16 physical / 32 logical CPUs. Actual
  `navigator.hardwareConcurrency` was 32. Windows Chromium 143.0.7499.4 ran
  headlessly with no mobile-device emulation. The real packaged `DspClient` /
  module worker / WASM Session were used; WASM processing was single-threaded.
- Server: AMD EPYC 7551P, 32 physical / 64 logical CPUs, approximately 189 GiB RAM.
  The actual Linux release worker was invoked over pinned SSH from an isolated
  operator benchmark project, with four and eight Rayon threads tested separately.
- Server profiles each had their own transient user cgroup: CPU quota 800%,
  memory high 24 GiB, max 32 GiB, swap max zero and tasks max 256. The active
  backend and its service configuration were not altered.

This compares a desktop PC browser with the native operator worker. It is
**not physical iPhone/Safari speed qualification, nor deployed HTTP/backend or
Funnel end-to-end latency**. A faster offloaded result also depends on real upload,
queue, preview and download behavior, none of which is inferred from core timings.
Eight threads are a tested configuration, not a claim that the current backend
uses eight. The parent still owns worker-thread and backend deployment choices.

## Matched workload and validation

One deterministic 60-second stereo signal supplied fixed 30- and 60-second
prefixes: 48,000 Hz, two channels, PCM16, seed 20261116. Both paths decoded the
same file bytes to interleaved Float32 using the PCM16 divisor 32768; independent
Python decoding, browser decoding and server decoding produced identical input
PCM hashes. No resampling, truncation, reduced rate or processed-source reuse
was allowed. Every Master used the same immutable original.

Settings were neutral / streaming-safe, explicit target **−14 LUFS**, ceiling
**−1 dBTP**, intensity **1**, safety **true**, no reference and no effective manual
EQ. Report policy, effective target, input analysis and safety attempts were
checked for every warm-up and measured run. Every Master performed **one safety
attempt**; these fixtures do not exercise maximum retry cost or intense limiting.
Output integrated loudness was within 0.000001 LU of −14; the ceiling was not an
active bottleneck for this signal.

Input-analysis numeric differences were at most `6.47e-07`. Independently
decoded first measured master outputs differed by at most `1.79e-07` per
Float32 sample (maximum RMS difference `1.98e-08`). Four- and eight-thread native
PCM hashes matched each other. Browser/native output bytes were not bit-identical.
The private validator required input-analysis difference <=1e-5, output-metric
difference <=1e-4 and maximum sample difference <=1e-4. All validation checks
passed; observed differences are reported above rather than only a pass/fail.

| Fixture | WAV SHA-256 | Decoded Float32 SHA-256 |
|---|---|---|
| 30 s | `214c7d84744abdc14278d90bf883d5147dc097442733a916848bc4b52deb0e93` | `a3424afca5f136e63303031c9eea67faa27b4c7e289b7151eaa5581de80422be` |
| 60 s | `c47628408f73f190ef19e77e76f06d751596b5f57ed5c34a789bd45ae36e8c9e` | `e7115d1ec2f43b416470538419706ac6c4e68b01974f3cb095ad53e822d5e562` |

## Median worker wall times

Seconds; all three measured runs follow one warm-up per operation. Browser wall
is `DspClient.call` dispatch through worker completion, including report parsing
and Master PCM copy/transfer back. Browser WAV fetch/decode/import is separate.
Native wall includes fresh process startup, source file read/hash/decode, DSP,
waveform/analysis, response JSON and Master Float32 WAV output. It includes up to
10 ms of wrapper polling granularity. Neither path includes playback setup or
audio hardware; native wall excludes SSH/SFTP and undeployed backend overhead.

| Clip | Operation | Browser PC | Server, 4 threads | Server, 8 threads |
|---|---|---:|---:|---:|
| 30 s | Analyze | 0.559 | 0.786 | 0.562 |
| 30 s | Master | 4.403 | 4.494 | 3.326 |
| 60 s | Analyze | 1.239 | 1.497 | 1.061 |
| 60 s | Master | 9.873 | 8.757 | 6.499 |

At 30 seconds the 3 ms Analyze median difference between browser and eight-thread
native is below the native wrapper's polling granularity: treat it as a tie.
The browser's 60-second Master times decreased across the three measured runs;
one warm-up is not proof of fully stabilized JIT or thermal behavior. No p95/p99
or long-session claims can be made from three samples.

## Master core time versus adapter wall time

Core seconds are the actual `Report.processing_seconds`, not subtraction of
estimated transport costs. Analyze exposes no corresponding core timer, so no
Analyze core time is invented. Both adapters skip effective manual EQ; the
browser's zero-gain default EQ flags and the native adapter's disabled flags
differ in metadata but do not activate EQ processing.

| Clip | Runtime | Median core seconds | Median worker wall seconds |
|---|---|---:|---:|
| 30 s | browser | 4.399 | 4.403 |
| 30 s | native-t4 | 4.229 | 4.494 |
| 30 s | native-t8 | 3.066 | 3.326 |
| 60 s | browser | 9.866 | 9.873 |
| 60 s | native-t4 | 8.321 | 8.757 |
| 60 s | native-t8 | 6.059 | 6.499 |

## All warmed samples and retained warm-ups

Times below are in execution order. All runs are reported, including the slower
four-thread setting. The medians above do not select each runtime's fastest run.
A separate two-second preflight validated the harness before these cases and
is retained privately, excluded from the 30/60-second comparison.

| Clip | Runtime | Operation | Warm-up wall | Warm-up core | Warmed wall runs 1 / 2 / 3 | Warmed core runs 1 / 2 / 3 |
|---|---|---|---:|---:|---|---|
| 30 s | browser | analyze | 1.214 | not exposed | 0.550 / 0.559 / 0.567 | not exposed |
| 30 s | browser | master | 4.874 | 4.867 | 4.403 / 4.415 / 4.388 | 4.399 / 4.412 / 4.385 |
| 30 s | native-t4 | analyze | 0.859 | not exposed | 0.786 / 0.775 / 0.786 | not exposed |
| 30 s | native-t4 | master | 4.491 | 4.233 | 4.501 / 4.469 / 4.494 | 4.245 / 4.211 / 4.229 |
| 30 s | native-t8 | analyze | 0.553 | not exposed | 0.572 / 0.562 / 0.561 | not exposed |
| 30 s | native-t8 | master | 3.340 | 3.078 | 3.326 / 3.306 / 3.382 | 3.066 / 3.051 / 3.117 |
| 60 s | browser | analyze | 2.476 | not exposed | 1.252 / 1.239 / 1.149 | not exposed |
| 60 s | browser | master | 10.594 | 10.587 | 10.202 / 9.873 / 9.484 | 10.197 / 9.866 / 9.479 |
| 60 s | native-t4 | analyze | 1.497 | not exposed | 1.496 / 1.498 / 1.497 | not exposed |
| 60 s | native-t4 | master | 8.772 | 8.336 | 8.751 / 8.810 / 8.757 | 8.313 / 8.341 / 8.321 |
| 60 s | native-t8 | analyze | 1.060 | not exposed | 1.062 / 1.038 / 1.061 | not exposed |
| 60 s | native-t8 | master | 6.596 | 6.155 | 6.499 / 6.504 / 6.459 | 6.060 / 6.059 / 6.026 |

## Initialization and import, measured separately

The browser init column measures the `init` call after synchronous client
construction: asynchronous worker startup and WASM initialization are included,
but synchronous constructor time was not recorded. Later workers may reuse
compiled browser cache. PCM16 parsing was an explicit fixture decoder,
not Web Audio import/resampling. Native startup below is a separate `--help`
process probe, not an equivalent WASM initialization operation. Native `import`
also writes a Float32 original and computes analysis/waveform; browser `load`
only installs the source Session, so these import columns are not speed ratios.

| Clip | Browser init call | Local HTTP fixture fetch | PCM16 decode | Browser load/transfer |
|---|---:|---:|---:|---:|
| 30 s | 0.022 | 0.018 | 0.006 | 0.005 |
| 60 s | 0.015 | 0.041 | 0.014 | 0.010 |

| Clip | Native threads | Startup help probe | Native import / analyze / write |
|---|---:|---:|---:|
| 30 s | 4 | 0.004 | 0.849 |
| 30 s | 8 | 0.004 | 0.596 |
| 60 s | 4 | 0.004 | 1.579 |
| 60 s | 8 | 0.004 | 1.120 |

## Transfers and resource evidence

One initial pinned SSH connection took about 0.072 s. SFTP uploads/downloads below
were separate LAN observations, not Funnel measurements and not general network
performance. Inputs were uploaded once, then reused by all timed native runs.
First measured master WAVs were downloaded afterward for independent decoding.
Each download opened a separate SSH connection (about 0.072–0.079 s), excluded
from its SFTP body time. These times are not part of the worker wall table.

| Transfer | Bytes | SFTP seconds |
|---|---:|---:|
| Upload fixture-30s.wav | 5,760,044 | 0.059 |
| Upload fixture-60s.wav | 11,520,044 | 0.113 |
| Download 30 s master, 4 threads | 11,520,068 | 0.102 |
| Download 30 s master, 8 threads | 11,520,068 | 0.147 |
| Download 60 s master, 4 threads | 23,040,068 | 0.237 |
| Download 60 s master, 8 threads | 23,040,068 | 0.247 |

| Native profile, after both clips | Aggregate cgroup peak bytes | Sampled peak tasks | Throttled periods | Throttled microseconds |
|---|---:|---:|---:|---:|
| 4 threads | 412,667,904 | 6 | 0 | 0 |
| 8 threads | 416,473,088 | 10 | 60 | 67730 |

Both profiles had zero memory-high/max/OOM events. Aggregate cgroup memory includes
charged file cache as well as process allocations; it is not individual-process
RSS. The eight-thread profile had small but nonzero CPU throttling under its
800% quota, retained in these results. Small fixtures do not qualify 2 GiB jobs,
model inference, large banks, concurrent sessions or long-duration retention.

## Artifact identity and private evidence

Browser WASM SHA-256:
`a087dd3f42772b1ef2d86a538577b321a4fd2e797dc246d0f6106a41c45aacf3`.
This is the frozen `be3fcde` browser DSP build; its WASM hash was independently
matched before timing. Native release-worker SHA-256:
`0bc76b972a46fc6550e7428cb617b75df58ffaa4b3e22f61eb38895f91ac3a75`.
The native binary was snapshotted into the private benchmark tree before timing;
no integration source or active backend executable was changed.

Private evidence retains fixture generation, full settings and reports, all warm-up
and measured rows, independent hash/PCM comparison, exact browser artifact hashes,
host snapshots, transfer records and reproducible scripts. Those operator paths,
account/process identities and source inventories are intentionally not published.
