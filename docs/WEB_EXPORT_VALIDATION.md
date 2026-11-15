# Browser parity validation — November 15, 2026

This records development work on `feature/stem-editor-workspace`, based on desktop editor commit `4c0b2f7`. It is not a hosted deployment, installer acceptance or physical-phone qualification. All new fixtures are synthetic; no private recording was published or uploaded.

These are the original export-candidate results. Development is now integrated
on `current-work` with a separately checked [live preview](MOBILE_PREVIEW.md).
Later browser timeline/time-edit results are in [WEB_STEM_EDITOR.md](WEB_STEM_EDITOR.md);
neither later publication nor this earlier record establishes physical-phone acceptance.

## Baseline

A fresh release WASM/static build passed the existing Chromium 143 mastering and 25-group Studio suites before implementation. The original worklet/direct-Rust comparison differed by at most approximately `1.15e-38`; matching settled PCM did not establish glitch-free parameter changes. The host binding baseline passed 12 tests.

## Export coverage

| Check | Result and scope |
| --- | --- |
| App export workflows | Chromium 143.0.7499.4, Firefox 144.0.2 and Linux WebKit 26 passed mastering/Studio MP3, unchanged bytes after listener-volume/match edits, EQ preset JSON, WAV/MP3 session packages, CRC extraction, current settings, pending-mix save guards and 320/390px layout checks. |
| Stale/cancelled packaging | Delayed actual ZIP worker results could not download old EQ after an edit or publish after cancellation; the source remained loaded. |
| Packaging lifecycle | 17 Node cases with a fake Worker and real structured cloning ended with zero workers, timeouts and abort listeners. Checks cover malformed payloads, clone failures, success, cancellation, runtime/message errors, timeout and duplicate/late results. Real archive contents are checked separately. |
| Startup failures | Chromium, Firefox and Windows WebKit passed successful load, blocked required dependency, private-path exception and an actual 30-second startup deadline. Reporting stayed reachable, downloaded text matched its preview and private-path canaries stayed out. CSP and the versioned runtime graph were preserved. |
| MP3 codec | Each of Chromium, Firefox and Windows WebKit passed 11 independent FFmpeg roundtrips, four bitrates, input validation, progress, cancellation/retry and identical encoder bytes. Windows WebKit has no AudioContext in this test build, so browser decoding there was not run; Linux WebKit app tests separately cover decoding. |
| Lossy peak overshoot | A peak-safe synthetic WAV encoded above its −1 dBTP ceiling on the first MP3 attempt. Actual mastering/Studio verification retried from the unchanged WAV: approximately **−0.9477 → −1.1620 dBTP** in Chromium/Firefox and **−0.9476 → −1.1623 dBTP** in Linux WebKit. Final bytes matched a fresh encode at the recorded gain; independent FFmpeg 8× reconstruction measured approximately **−1.1690 dBTP**. |
| ZIP builder | Three browser engines independently extracted 13 archives each. Checks include real Rust settings/WAV16/24/32f, odd mono frame counts, extensible WAV, aligned banks, exact entry/JSON/archive bounds, flat unique names, CRC corruption, metadata redaction and unchanged legacy raw ZIP bytes. |
| Encoder provenance | 17 recorded source/notice/build-input hashes verified. The binary is a replaceable local WASM asset; its source and separate licenses are shipped in the site. Rebuilding it with Emscripten was not run. |

## Preview and regression coverage

The shared Rust processor now limits historical replay to 16 frames per incoming frame while preparing a replacement filter. Tests verify the old audible path remains intact until crossfade, history catch-up includes incoming samples, and the final target converges. Worklet settings coalesce from 61 incoming messages to 22 applications at 22 edited blocks.

The focused Chromium comparison measured a maximum edited direct-WASM 128-frame block around **0.11–0.12 ms**, versus **1.99 ms** in its baseline. The final probe separately measured settings bursts, with a maximum around **0.39 ms** in that run. These are synthetic desktop diagnostics, not whole-system scheduling guarantees or phone performance claims. Actual worklet output matched the direct shared processor exactly in tested 48 kHz stereo and 44.1 kHz mono cases; settled target errors were at most approximately `4.7e-10`. Running-source identity, A/B isolation and live attenuation also passed in Chromium/Firefox.

The native release all-targets suite with schemas and strict Clippy passed after the shared processor change; two existing opt-in archive tests remained ignored. The web binding passed **14 tests**, WASM Clippy passed, and the MCP suite passed **42 Rust tests**. The development MCP tool count remains **43**; browser-only downloads do not add a new desktop DSP operation.

Existing Chromium mastering/Studio suites passed on the integrated export candidate. Recovery passed 14 groups, including process termination, both restored workspaces, pending controls and report privacy. Learning/layout passed all 20 groups at desktop and 320/390/430px widths, including 200% text and persisted tutorial choices. An earlier concurrent learning run encountered a localhost connection failure while loading modules; an isolated run passed without weakening checks. Independent investigation confirmed that corrupt preferences alone were not the cause. The failed-load path is addressed at the bootstrap boundary and checked separately.

## Limits retained

- No actual iPhone/Android device, hardware audition, Bluetooth/background interruption, or thermal test was performed.
- The tester's original audio/browser was not available. Reduced processing bursts are a measured improvement, not a reproduced diagnosis of that recording's sound.
- An exact 900-second stereo encoder probe succeeded in Chromium/Firefox, but decoded MP3 contained 1,152 extra frames (**900.024 seconds**) and the unchanged import guard rejected it. WAV remains the sample-accurate option. That probe excluded full-app mastering/verification memory and did not qualify mobile capacity.
- Full-capacity 512 MiB ZIP memory stress and encoder source rebuild/relink were not run. Existing input, Studio and recovery budgets are unchanged.
- Portable ZIPs do not restore an editing workspace, provide neural separation or create missing stems. Native VST3 and the desktop timeline/time edits remain desktop features.

The checks above describe the original development qualification and do not by themselves claim a deployment. Later on November 15, the separate [current-work mobile preview](MOBILE_PREVIEW.md#first-live-validation--november-15-2026) was published and tested against HTTPS in Chromium, Firefox and Linux WebKit. The main app and installer were not promoted. See [export architecture/workflow](WEB_EXPORTS.md) and the [remaining parity roadmap](BROWSER_PARITY_ROADMAP.md).
