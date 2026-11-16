# Hybrid Studio v2 validation — November 16, 2026

The v2 backend is deployed behind the existing Funnel endpoint. The ordinary
editor uses server projects for original-file import, analysis, mastering,
supported Studio processing and full-resolution export. Local processing remains
the default. This record separates native tests, owned HTTP fixtures, the public
service and physical-device acceptance.

## Implemented and checked

| Check | Result and scope |
|---|---|
| Backend on Linux | **114 tests passed** in GitHub Actions at `14aaf57`; shared queue, ownership, range downloads, limits, expiry/cancellation, immutable banks, reference selection, typed analysis, history and actual native codec integration |
| Windows backend | 107 of 111 tests passed, four platform-specific checks skipped; five focused report/new-bank regression tests subsequently passed with the actual worker |
| Native worker | Windows/Linux release builds, strict Clippy and native/HTTP fixtures passed; [worker qualification](../rust/src/hybrid_worker/VALIDATION.md) retains exact scope and hashes |
| Full browser workflow | Chromium 143 and Linux WebKit 26: **18 checks each**; Firefox 144: **17 checks**, plus **13 targeted stream checks** |
| Preview audio and live controls | Captured output verified live EQ level change, stopped silence and live stem mutes; original/full-bank A/B, region edits, exact edited-bank PCM, Undo/Redo and full-mix exports were checked |
| No full browser decode in server mode | Tests reject full-song `decodeAudioData`, media Blob/object-URL paths and heavy browser audio jobs; playback streams native media through authenticated Range requests |
| Downloads | WAV, MP3 and session ZIP passed actual browser downloads and artifact hash checks; full-resolution server audio is distinct from the compressed listening preview |
| MCP | Seven focused help tests and actual 45-tool stdio protocol verification passed; public remote processing is not exposed through the local companion |
| Published local mode | Final functional build **`1b580ca` / runtime `3882a4a1d0d5a7cf`** passed all eight hosted smoke groups: touch import, Master, live EQ, MP3/report/ZIP, 320/390-pixel layouts, matching runtime graph and only same-origin static GETs |
| Engine guidance | Source candidate after that build passed native-backed import/Master, local/server label changes, retained streamed-source export guidance and 320/390-pixel layout without page errors |
| Existing local work → server | Five actual-UI migration checks passed in each of Chromium 143, Firefox 144 and Linux WebKit 26: original/edit PCM, pending faders, required mix commit, native tools and original-based main Re-Master; old local worker and playback references were released |
| Long public song | **Passed:** 150-second preflight and **18-minute, 48 kHz stereo MP3**, through actual published Pages/Funnel: import, Analyze, Master, captured stream/Stop and complete WAV/MP3/session ZIP downloads; originals retained exact geometry and all owned projects were removed |
| Browser control/state regressions | **129 Node checks** passed; classic-worker build and bounded audio-startup checks are included in **27 Python publication/build checks** |
| Documentation bundles | **15 payload + 3 link tests** passed on Windows and WSL after adding the v2 guides/screenshots to the reviewed public allowlists; unreviewed private files remain excluded |
| Existing local reference/export bug | Reproduced on baseline `be3fcde` and fixed in the WASM Session: changed references refuse stale exports while preserving recovery PCM; unchanged reference reloads stay valid. Chromium/Firefox/WebKit passed **14 DSP checks each**; Chromium recovery passed **17 checks**, including A→B refusal and retained-PCM adoption. **31 binding tests**, formatting and strict Clippy passed |

The browser suites use real native DSP in an owned loopback service. Their
fixture changes only the copied API-origin pin and supplies a streaming HTTP
proxy; it does not simulate DSP replies. The generated classic Service Worker
preserves shared policy sources and supports Firefox 144, whose module Service
Worker evaluation failed. Credentials remain in tab memory, including across
native attachment downloads; two workspace listeners do not close each other's
message ports.

The first final hosted smoke encountered HTTP 503 for one decorative crest image
and was recorded as failed. A subsequent direct asset request returned 200, and
a fresh complete hosted smoke passed without ignoring errors or changing source.

Linux CI later exposed a probe startup assumption: media could be buffered while
the headless PulseAudio clock had not produced PCM within the fixed 400 ms wait.
One reproduced start needed 1,007 ms. The verifiers now wait for measured PCM and
audio-clock settling within a strict deadline, retain the same gain/restoration
and silence thresholds, and fail permanently silent or nonfinite output. All
13 media checks passed in each engine with PulseAudio/two-core constraints; the
18 full UI Chromium checks also passed. Production app code was unchanged.

## Actual deployed service

The deployed Linux worker SHA-256 is
`37fd55fc09201f0d4397ecd784c03bf1fbf65c2e05921c89f2f674247ec2c8ff`.
Backend source was checkpointed at `24e5b3d`. The service uses eight worker
threads, CPU quota 800%, memory high 24 GiB, memory max 32 GiB and 256 tasks.
One heavy worker is shared between v1 and v2. A quota is a bound, not a promise
that every input admitted by duration/byte limits will finish.

**14 actual public-service checks passed** on one deterministic 24-second,
44.1 kHz stereo fixture. These covered original decode/identity, authenticated
preview Range, two/four neural stems, voice cleanup, issue scan, spectral repair,
region editing, current bank controls, WAV/MP3/ZIP downloads and cancellation.
Every output retained its geometry and finite PCM; complete mixes were compared
with their independently decoded stem sums. The original stayed byte-identical.

Native/model processes were observed under the dedicated service identity and
configured limits. Cancellation during real neural work preserved the previous
revision and reaped the owned model/native child processes. The service remained
active. The one owned guest project was deleted; a subsequent GET returned 404.
No unrelated project, desktop installation or Funnel configuration was changed.

The observed two-stem command took 78.5 seconds including queueing, with its
first running observation at 56 seconds; four-stem took 26.4 seconds. These are
qualification observations, not isolated inference benchmarks or a latency SLA.

## Speed and admission

### Published browser and public transport

One installed **Google Chrome 156.0.8078.80** page loaded actual published build
`276a8c8` and kept one guest
for the 150-second preflight and 18-minute case. Neither static assets nor Funnel
responses were simulated. The 18-minute MP3 was 8,640,428 bytes and decoded on
the server to **51,840,000 stereo frames / 414,720,000 Float32 bytes**: beyond
both the local 15-minute duration ceiling and 384 MiB decoded-PCM limit.
The decoded source PCM hash and WAV/MP3 output frame counts were independently
checked. Browser full-PCM decode, whole-media reads and heavy worker counters
remained zero; native streamed playback produced nonzero output and Stop silence.

| Actual UI action | 150 seconds of audio | 18 minutes of audio |
|---|---:|---:|
| Import, including upload/decode/preview preparation | 20.84 s | 78.42 s |
| Analyze | 3.65 s | 19.04 s |
| Master, including preview/poll overhead | 22.88 s | 150.59 s |
| Reported Master engine time, included above | 14.47 s | 105.33 s |
| WAV24 render plus full streamed download | 59.34 s | 281.89 s |
| MP3 render plus full streamed download | 32.48 s | 124.00 s |
| WAV session ZIP render plus full streamed download | 48.06 s | 292.99 s |

These are individual network-inclusive qualification observations, not medians,
phone benchmarks, a speedup ratio or a latency guarantee. The 18-minute WAV and
ZIP downloads were approximately **311 MB each**; network time matters. The
entire two-phase qualification, including three complete downloads per phase,
took 1,148.93 seconds. Both phases passed, sources were unchanged, and the owned
server projects subsequently returned 404 after cleanup. Earlier failed harness
attempts are retained privately: they involved observation/dialog assumptions,
not a claimed successful full run.

### Matched processing benchmark

See the [matched benchmark](../hybrid/deploy/remote-tools/BENCHMARK.md) for all
warmed samples, hardware, inputs, settings and independent PCM comparisons.
On a 60-second fixture, median Master worker wall time was **9.873 seconds in
the Ryzen 9 5950X desktop browser** versus **6.499 seconds on the eight-thread
EPYC server**: about **1.52x throughput**. Analyze was 1.239 versus 1.061 seconds.
The 30-second Master comparison was about 1.32x; short Analyze was effectively
tied. These timings exclude internet upload, queue, preview creation and download.
They do not establish a physical-phone multiplier or a whole-workflow speedup.

Source admission defaults to one hour / 2 GiB encoded and decoded per input;
neural separation separately retains a 512 MiB decoded limit. The worker's
one-hour qualification used 8 kHz mono with `safety:false` and `intensity:0`;
it is not a maximum-memory, default-settings, 48 kHz stereo qualification.

## Limits still requiring device/workload acceptance

- Physical iPhone/Android audio, storage behavior and sustained memory use.
- Subjective isolation/mastering quality, headphones/speakers and GPU inference.
- Maximum 2 GiB jobs, large simultaneous live stream banks, concurrent-user
  endurance, network interruption/resume and long-session stability.
- A temporary server project is not local crash recovery. Export/download a
  session ZIP before leaving; credentials and remote undo history are not saved.

No source limits were increased by downsampling or truncating originals. The
stable main web app and Hotfix4 desktop installer remain separate releases.
