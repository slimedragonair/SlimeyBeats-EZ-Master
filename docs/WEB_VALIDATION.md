# Browser preview validation

Validation date: November 12, 2026 (America/New_York). GitHub Actions may show November 13 in UTC.

This record separates measured behavior, automated integration coverage, appearance review, and physical-device work that remains open. The maintained [browser workflow](../.github/workflows/web.yml) runs against each pull request and main; [its current results and artifacts](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/actions/workflows/web.yml) are the source of truth for the latest commit. Windows, Linux and browser checks are all required before merging.

## Tested environments

| Environment | Evidence and scope |
| --- | --- |
| Windows native source | 77 regression tests, formatting, Clippy with warnings denied, and release builds passed in CI. |
| Ubuntu 24.04 native source | 79 regression tests plus actual exports, XDG paths, native GUI captures and a CLI build without desktop features. Virtual display/audio backends do not establish hardware playback. |
| Additional maintainer-reported Linux-server run | All 79 tests and lint passed; mastering, all six export formats and GUI launch/import/mastering/EQ were verified. Desktop/menu launchers were installed, with no application source changes needed. The host reported Dummy Output with no headphones/speakers; audible hardware playback and the file-picker portal remain unverified. [Server-check scope and local report location](LINUX.md#additional-linux-server-check). |
| Chrome 156.0.8078.80 on the development PC | Actual application flows, worker DSP, AudioWorklet rendering, exports, recovery and responsive-layout checks. |
| Chromium 143.0.7499.4, Firefox 144.0.2, WebKit 26.0 on Ubuntu CI | Three-engine integration matrix using a synthetic audio sink. Initial complete three-engine qualification: [run 43354121252](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/actions/runs/43354121252). Later state regressions are enforced by the maintained workflow. |
| iPhone / Android hardware | **Not qualified yet.** Responsive desktop-engine captures are not physical-device acceptance. |

The 77/79 counts describe current native source, including platform-specific tests. They do not replace the **73-test historical record for the published v0.4.2 installer**. The release tag, installer and portable archive were not rebuilt or overwritten for the browser preview.

## Audio and export checks

- The included 45-second CC0 recording is loaded, analyzed, mastered and exported through the actual interface. No mock DSP or server renderer stands in for the Rust engine.
- A deterministic stereo fixture is processed in the actual AudioWorklet and compared with the same Rust streaming processor called directly. Output length, sample alignment and finite samples are checked. Observed differences were zero in local Chrome and approximately 1.2 × 10⁻³⁸ or less in the other recorded engines; this is numerical agreement for the tested fixture, not proof of subjective sound quality.
- Suggested EQ repeats identically for identical measurements/settings, stays within its bounded moves, and rejects silence rather than inventing a correction.
- WAV 16-bit PCM, 24-bit PCM and 32-bit float are decoded and peak-checked before download. Exported fixtures are independently measured with FFmpeg. Mono, stereo, silence and duration preservation are covered.
- Listening-volume changes produce byte-identical exports. Rapid EQ edits do not invoke full-track mastering/export or interrupt the audition. The full-track export resolves loudness compensation rather than assuming the live estimate is exact.

The browser uses the native Rust analysis/mastering and streaming EQ code. A target-specific monotonic clock and a shared memory/file WAV writer adapt it for WebAssembly; the native DSP was not retuned.

## Session, cancellation and privacy checks

Chrome integration tests verify that replacing a reference invalidates both UI export readiness and the worker's stored master, even when different audio has the same filename. Cancel restores the original, obsolete track/EQ results cannot replace current data, invalid files report errors without destroying the existing session, and processing continues with the network offline after application files are loaded.

A controlled delayed-decode regression uses the real browser decoder, holds its completion, cancels the import, starts a newer mastering job, and then releases the old result. The cancelled result must not replace the restored track or unlock controls owned by the newer job. Short synthetic sources are used for these state tests; the real recording and numerical audio tests remain separate.

Request logs are checked for same-origin static GETs with no audio request body. There are no accounts, analytics, upload API, persisted user audio, or server DSP. The static host still receives ordinary requests for its application files and optional example. Closing/reloading the tab discards the session; there is no automatic project recovery.

Playwright 1.57's WebKit screenshot preparation inserts an empty `body {}` style. The strict production CSP blocks it. The harness records that known automation diagnostic only when a matching insertion is observed during capture; the policy is not relaxed and other errors remain fatal. An isolated test verified that a deliberate application stylesheet violation is still detected. Windows Playwright WebKit did not expose Web Audio, so the Linux engine was used for audio qualification.

## Appearance and a longer fixture

Actual 1480 × 1000 desktop captures and 390 × 844 responsive viewport captures received independent review. The five principal welcome/master/EQ states scored at least **9/10 for visual quality, polish and readability** after revisions. Scroll captures verified complete result cards, readable graph scales, an opaque transport and active-band editing. This is a scoped appearance review, not accessibility certification or a review of every possible state.

A three-minute synthetic stereo fixture at 48 kHz (65.92 MiB decoded) preserved **8,640,000 frames** and exported finite, peak-safe audio in Chrome. On that development-PC run, mastering including worker loading took about 25.6 seconds; total mastering plus WAV export took about 32.1 seconds. File decoding and UI interaction were not part of that measurement. One synthetic desktop case is not a cross-device speed promise or a competitor benchmark.

## Limits and reproduction

The preview accepts one mono/stereo track at a time, up to five minutes **and** 96 MiB of decoded PCM, with a 100 MiB source-file limit. The decoded-PCM limit can be reached before five minutes. Browser decoding uses a 48 kHz context; input codec support varies. Export is WAV only. Full files are held in memory, so actual device capacity may be lower and decoding very long compressed files can fail before the decoded-size check.

Physical iPhone/Android audio, headphones/Bluetooth routing, background/lock-screen interruptions, thermal behavior and full-song memory use remain open. The additional Linux-server check confirms the native desktop workflow and exports; audible hardware playback still needs a connected output device, and the file-picker portal remains unverified. Automated tests do not establish commercial-mastering equivalence or replace listening.

Build, serve and reproduce the checks using [WEB.md](WEB.md). Test entry points are [verify_browser.py](../web/tools/verify_browser.py), [browser_audit.js](../web/tools/browser_audit.js), and the [Linux verifier](../rust/tools/verify_linux.py). Runtime browser use does not require FFmpeg or Python; those are development/verification tools.
