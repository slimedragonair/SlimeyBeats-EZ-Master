# Changelog

## Unreleased

- Raise the private browser preview to 15 minutes per track, with coordinated 384 MiB source/decoded bounds and visible mobile import errors. Reduce redundant input/reference/preview/convolution allocations and reserve export buffers. Keep one active mix rather than a growing queue.
- Version connected browser runtime assets together to prevent cached JS/WASM API mismatches. Add duration/byte boundary regressions, real long WAV/MP3 imports and repeat-use/master/export memory checks.
- Honor declared MP3 gapless timing when a browser leaves encoder padding in the decoded audio, fixing WebKit's rejection at the 15-minute boundary without widening the WAV limit or shortening overlong recordings. Cross-check decoded timing with independent FFmpeg PCM.
- Add a private static browser preview using the shared Rust engine in WebAssembly: local decoding/analysis/mastering, AudioWorklet live EQ, overlaid spectrum, deterministic EQ suggestions, matched A/B audition, reference profiles, three verified WAV formats and touch-friendly help.
- Discard late browser decode/fetch completions after cancellation and preserve control ownership across overlapping job completions.
- Invalidate both the stored master and its export when reference audio is replaced, even under the same filename.
- Add responsive SlimeDragon styling, browser qualification, static-site build/license inventory and GitHub Pages deployment. Native v0.4.2 release artifacts remain unchanged.

- Add Ubuntu Linux build/export/GUI validation, XDG settings/data paths, Linux system-font lookup and platform-native output-folder opening; preserve the Windows portable layout and DSP.
- Fix installer packaging on fresh runners by allowing locked Cargo metadata to fetch platform packages needed for the license inventory; include Cargo's error details on failure.
- Make hosted installer verification explicitly command-line-only, retain full GUI verification by default locally, and include capture error output when it fails.

## 0.4.2 — 2026-11-12

- Add MIT licensing for original project code/docs/artwork and an explicit no-ownership/no-royalty audio-output policy.
- Prepare the GitHub README with actual native screenshots, a credited CC0 music example and loudness-matched previews.
- Add installation, user, build, release, contribution and security documentation; preserve the separate Python guides as historical material.
- Add a per-user Windows installer, optional desktop shortcut and data-preserving uninstall.
- Add a pinned/checksummed portable NSIS build tool, clean public-source export and Windows CI/packaging workflows.
- Preserve the 0.4.1 DSP; this release concerns distribution, licensing, examples and documentation.

## 0.4.1 — 2026-11-12

- Correct the reproduced true-peak limit failures with longer reconstruction, boundary-tail checks and a shared limiter/final detector.
- Bound combined automatic EQ before intensity and coordinate static/dynamic cut budgets.
- Determine EQ compensation status from the final measured render.
- Pass 73 automated tests, 24 real-song render checks, six Cipher formats in both modes, independent reconstruction and additional mono/rate/EQ cases.

## 0.4.0 — 2026-11-12

- Implement the SlimeDragon Audio silver/amethyst/teal visual direction with native controls and generated supporting artwork.
- Fix frame joins, compact layouts, scrolling, waveform labels and native slider sizing.

## Earlier native work

- Add the native Rust GUI/CLI, separate Analyze/Master/Export workflow, beginner help, deterministic suggested EQ and live layered spectrum/EQ editing.
- Keep the original Python implementation and its data separate. Its [historical changelog](docs/legacy/python-changelog.md) is retained for reference.
