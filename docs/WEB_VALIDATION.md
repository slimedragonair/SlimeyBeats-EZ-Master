# Browser preview validation

Historical local validation: November 13, 2026 (America/New_York). Earlier three-engine and appearance qualification was recorded November 12; GitHub Actions may use the following UTC date.

**Current beta.3 results:** Chrome 156, Firefox 144 and Linux WebKit 26 each passed **20 complete larger-bank workflow groups**, including 24 manual rows, 64-part import/export, twelve-part all-selected ZIPs, folder-aware names/manifests and 390px controls. Separately, **11 native-host web-DSP binding tests passed**, including 24/64 parts, exact float exports/history, the 128 MiB combined-input boundary and rejection of the next sample over it. Browser Studio retains that budget and its 15-minute per-track guard. The final WebKit rerun completed with `passed: true`, 20 groups and zero application errors. These desktop-engine checks do not qualify physical mobile devices. See the [beta.3 validation record](STUDIO_BETA3_VALIDATION.md); the tables and measurements below predate this follow-up.

This record separates measured behavior, automated integration coverage, appearance review, and physical-device work that remains open. The maintained [browser workflow](../.github/workflows/web.yml) runs against each pull request and main; [its current results and artifacts](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/actions/workflows/web.yml) are the source of truth for the latest commit. Windows, Linux and browser checks are all required before merging.

## Tested environments

| Environment | Evidence and scope |
| --- | --- |
| Windows native source | 78 regression tests, formatting, Clippy with warnings denied, and release builds are enforced in CI. |
| Ubuntu 24.04 native source | 80 regression tests plus actual exports, XDG paths, native GUI captures and a CLI build without desktop features. Virtual display/audio backends do not establish hardware playback. |
| Additional maintainer-reported Linux-server run | All 79 tests and lint passed; mastering, all six export formats and GUI launch/import/mastering/EQ were verified. Desktop/menu launchers were installed, with no application source changes needed. The host reported Dummy Output with no headphones/speakers; audible hardware playback and the file-picker portal remain unverified. [Server-check scope and local report location](LINUX.md#additional-linux-server-check). |
| Chrome 156.0.8078.80 on the development PC | Actual application flows, worker DSP, AudioWorklet rendering, exports, recovery and responsive-layout checks. |
| Chromium 143.0.7499.4, Firefox 144.0.2, WebKit 26.0 on Ubuntu CI | Three-engine integration matrix using a synthetic audio sink. Initial complete three-engine qualification: [run 43354121252](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/actions/runs/43354121252). Later state regressions are enforced by the maintained workflow. |
| iPhone / Android hardware | **Not qualified yet.** Responsive desktop-engine captures are not physical-device acceptance. |

The 78/80 counts describe the native source at that historical checkpoint, including platform-specific tests and a new exact-output convolution regression. Three binding-crate tests cover duration/decoded-size boundaries and deterministic export allocation changes. Two Python tests cover connected runtime cache versions. These do not replace the **73-test historical record for the published v0.4.2 installer** or the separate maintainer-reported 79-test server run. The release tag, installer and portable archive were not rebuilt or overwritten for the browser preview.

## Audio and export checks

- The included 45-second CC0 recording is loaded, analyzed, mastered and exported through the actual interface. No mock DSP or server renderer stands in for the Rust engine.
- A deterministic stereo fixture is processed in the actual AudioWorklet and compared with the same Rust streaming processor called directly. Output length, sample alignment and finite samples are checked. Observed differences were zero in local Chrome and approximately 1.2 × 10⁻³⁸ or less in the other recorded engines; this is numerical agreement for the tested fixture, not proof of subjective sound quality.
- Suggested EQ repeats identically for identical measurements/settings, stays within its bounded moves, and rejects silence rather than inventing a correction.
- WAV 16-bit PCM, 24-bit PCM and 32-bit float are decoded and peak-checked before download. Exported fixtures are independently measured with FFmpeg. Mono, stereo, silence and duration preservation are covered.
- Listening-volume changes produce byte-identical exports. Rapid EQ edits do not invoke full-track mastering/export or interrupt the audition. The full-track export resolves loudness compensation rather than assuming the live estimate is exact.

The browser uses the native Rust analysis/mastering and streaming EQ code. A target-specific monotonic clock, a shared memory/file WAV writer and direct interleaved convolution output adapt it for WebAssembly; the native DSP was not retuned. The browser convolution path is required to match native channel-plane output exactly for mono/stereo and partial FFT blocks.

## Session, cancellation and privacy checks

Chrome integration tests verify that replacing a reference invalidates both UI export readiness and the worker's stored master, even when different audio has the same filename. Cancel restores the original, obsolete track/EQ results cannot replace current data, invalid files report errors without destroying the existing session, and processing continues with the network offline after application files are loaded.

A controlled delayed-decode regression uses the real browser decoder, holds its completion, cancels the import, starts a newer mastering job, and then releases the old result. The cancelled result must not replace the restored track or unlock controls owned by the newer job. Short synthetic sources are used for these state tests; the real recording and numerical audio tests remain separate.

Request logs are checked for same-origin static GETs with no audio request body. There are no accounts, analytics, upload API, persisted user audio, or server DSP. The static host still receives ordinary requests for its application files and optional example. Closing/reloading the tab discards the session; there is no automatic project recovery.

Playwright 1.57's WebKit screenshot preparation inserts an empty `body {}` style. The strict production CSP blocks it. The harness records that known automation diagnostic only when a matching insertion is observed during capture; the policy is not relaxed and other errors remain fatal. An isolated test verified that a deliberate application stylesheet violation is still detected. Windows Playwright WebKit did not expose Web Audio, so the Linux engine was used for audio qualification.

## Appearance and a longer fixture

Actual 1480 × 1000 desktop captures and 390 × 844 responsive viewport captures received independent review. The five principal welcome/master/EQ states scored at least **9/10 for visual quality, polish and readability** after revisions. Scroll captures verified complete result cards, readable graph scales, an opaque transport and active-band editing. This is a scoped appearance review, not accessibility certification or a review of every possible state.

A three-minute synthetic stereo fixture at 48 kHz (65.92 MiB decoded) preserved **8,640,000 frames** and exported finite, peak-safe audio in Chrome. On that development-PC run, mastering including worker loading took about 25.6 seconds; total mastering plus WAV export took about 32.1 seconds. File decoding and UI interaction were not part of that measurement. One synthetic desktop case is not a cross-device speed promise or a competitor benchmark.

## Long tracks and repeated use

The reported iPhone rejection was an explicit **over-duration error**, not evidence that WAV/MP3 decoding or the file chooser failed. Browser preview v0.2 raises the old five-minute/96 MiB guards together, to **15 minutes and 384 MiB decoded**, with a **384 MiB source-file limit**. Bounds come from the Rust binding and are displayed near Add audio file. A real 15:01 file is rejected with a visible, focused explanation above the phone playback dock. Unit tests accept exactly 15:00 at 48 kHz mono/stereo and reject one extra frame; the independent decoded-byte bound is also tested.

The following development-PC runs used **Chrome 156.0.8078.80**, actual Rust WASM, synthetic stereo sources, no audio uploads and no forced garbage collection:

| Test | Observed result |
| --- | --- |
| Real file chooser and decoding | 6-minute and 15-minute WAV **and** MP3 files accepted; 15-minute MP3 reference accepted. Sources were 44.1 kHz, decoded at 48 kHz. |
| Full 15-minute UI workflow | Imported a stereo WAV, mastered, played through the real AudioWorklet, adjusted EQ, stopped, and downloaded 24-bit WAV. All passed. 43,200,000 output frames; FFmpeg independently decoded 15:00 and measured −18.1 LUFS / −6.9 dBTP for this synthetic case. |
| 15-minute UI memory | Peak sampled private committed memory for the isolated browser process group: **3,731.25 MiB (3.64 GiB)**. WASM heap capacity: **2,146.25 MiB**. These are different measurements; PCM size alone is not the total memory cost. |
| Repeat-use soak | **40 consecutive 30-second tracks**, 20 minutes cumulatively, each replacing the previous mix without Clear. Every cycle mastered, played, changed EQ and exported. No failure or memory-limit rejection occurred. |
| Repeat-use memory | WASM heap stayed at **69.75 MiB** for all 40 cycles. Sampled browser private-commit peak: **689.59 MiB**; post-cycle values settled around 635–664 MiB after warm-up. No continuing growth was observed over the tested range. |

The app has **one active mix plus an optional reference**, so there is no queue whose combined duration reaches a fixed memory threshold. These 40 completed operations establish a tested lower bound on repeat use, **not a universal maximum track count**. Private committed bytes are sampled every 250 ms across the test browser's discovered processes; brief peaks can be missed. Audio buffers, JS, WASM and browser overhead are included, and freed allocation capacity may remain reserved until GC or tab/worker teardown. The UI run fell to about 1,623 MiB after Clear; that is not a promise of immediate memory return on phones.

An engine-only 15-minute run also preserved 43,200,000 frames and finite output, then rendered final EQ and exported 259,200,068 bytes of 24-bit WAV. Its WASM high-water was 2,146.25 MiB; this excludes the main page's A/B AudioBuffers. Timing was about 125 seconds for worker loading/mastering and 157 seconds including export on that run. Timing is illustrative, not a cross-device benchmark.

The reduced-copy input/preview/reference paths, browser convolution destination and reserved export buffers avoid unnecessary full-track allocations. This does **not** make a 15-minute job safe on every phone: the measured full workflow still used multiple GiB. The iPhone tester must retry the deployed fix, and actual mobile memory/playback qualification remains open. Supporting longer browser tracks reliably would require incremental decoding and processing/export with bounded memory.

A 15-minute constant-sine reference exceeded the initial **180-second measurement deadline in Firefox**, after all four WAV/MP3 mix imports had passed. The isolated reproduction remained CPU-active until that test deadline, without a reported application error. Sustained signals can force the offline reconstructed-peak detector through its expensive path across the track; this is a performance boundary, not a codec or duration rejection. The maintained import integration uses varying-level synthetic audio, still checks the complete 15-minute reference analysis, records its elapsed time and allows a longer measurement deadline. It does not skip peak verification or claim worst-case processing speed. The interface now identifies reference decoding and measurement rather than leaving the previous “Track ready” status visible.

An independent review opened the actual new 390 × 844 limit/error screenshots and scored look, polish and readability **9/10 each**. This covers those two states only.

## MP3 padding at the duration boundary

The expanded matrix found WebKit 26.0 counting MP3 encoder/decoder padding against the limit: a valid 15-minute WAV passed, while its MP3 was reported just over 15:00. A focused adapter now reads recognized Xing/Info + LAME-compatible declared timing and removes padding only when decoded frame counts match a known layout. It leaves already gapless buffers unchanged, rejects unknown/truncated layouts for correction, and never shortens a genuinely overlong recording to fit. The strict WAV/Rust frame boundary is unchanged.

Local Linux WebKit then accepted all four real 6-/15-minute WAV/MP3 imports and the complete 15-minute MP3 reference measurement. An initial three-second stereo-noise check at 44.1/48 kHz compared the result with independent FFmpeg decoding: normalized output was exactly **144,000 frames at 48 kHz**, with best alignment at **zero samples** in both cases. Observed stereo correlation was about 0.99545 after different 44.1→48 kHz resamplers, and 0.99999994 at 48 kHz. The maintained mono/stereo alignment fixture is band-limited noise so it checks timing without treating expected high-frequency resampler-response differences as a delay failure. This is a timing/alignment check, not bit-identical decoder certification. Chrome's already gapless buffers were returned unchanged.

Maintained checks cover mono/stereo at both source rates, malformed/missing timing, MPEG variants, ID3/CRC/optional tag layouts, no double trimming, unknown lengths, genuinely overlong sources and the strict extra-frame WAV rejection. The CI runs actual independent MP3 timing and long imports before the broader audio suite so a codec-boundary regression fails earlier. Physical iPhone testing remains outstanding.

## Limits and reproduction

The preview accepts one mono/stereo mix plus an optional reference, **up to 15:00 per file**, within **384 MiB decoded PCM and 384 MiB source size**. All three bounds apply independently. Browser decoding uses a 48 kHz context; input codec support varies. Export is WAV only. Full files are held in memory, so actual device capacity may be lower and decoding very long compressed files can fail before the decoded-size check. There is no combined-duration/song-count quota.

Physical iPhone/Android audio, headphones/Bluetooth routing, background/lock-screen interruptions, thermal behavior and full-song memory use remain open. The additional Linux-server check confirms the native desktop workflow and exports; audible hardware playback still needs a connected output device, and the file-picker portal remains unverified. Automated tests do not establish commercial-mastering equivalence or replace listening.

Build, serve and reproduce the checks using [WEB.md](WEB.md). Test entry points are [verify_browser.py](../web/tools/verify_browser.py), [browser_audit.js](../web/tools/browser_audit.js), [verify_imports.py](../web/tools/verify_imports.py), [verify_memory.py](../web/tools/verify_memory.py), and the [Linux verifier](../rust/tools/verify_linux.py). The maintained browser workflow runs actual long imports in Chromium/Firefox/Linux WebKit and a short replacement smoke; the 40-cycle and full 15-minute workflow measurements above are separate local Chrome runs. Runtime browser use does not require FFmpeg or Python; those are development/verification tools.
