# Changelog

## Unreleased

- Update the Studio development source to desktop **0.5.0-beta.3**, optional MCP companion **0.2.0-beta.3** and browser Studio **0.3.0-beta.3**. Beta.3 binaries have not been publicly released; stable v0.4.2 and earlier beta packages remain unchanged.
- Remove the fixed count cap for aligned imported stems/mix tracks. Loose-file and Suno/stem ZIP imports select all distinct parts by default, with optional selection and unique `part_N` identities for additional parts. At least two parts with matching sample rate, channels and frame count are still required; neural separation remains two/four stems.
- Keep separate import budgets: native defaults to 512 MiB decoded PCM per input and a 4 GiB aggregate decoded bank; browser Studio retains 128 MiB combined decoded input and 15 minutes per track. Available RAM, archive/metadata budgets and working buffers still constrain capacity.
- Expand the optional local MCP companion to 38 tools, including read-only stem ZIP inspection and job-based import. Preview preparation and explicit audio/project saving remain separate. See the [current Studio guide](docs/STUDIO.md) for workflows and qualification scope.
- Preserve accented/CJK paths in Windows MCP setup, require pending faders to be applied before further Studio edits/saving, and keep saved MCP projects tied to the current preview's actual source and stem bank. Add native, Windows PowerShell and three-engine browser regressions; see [merge validation and availability](docs/MERGE_VALIDATION.md).
- Remember desktop mastering settings and independent Studio export preferences, master/Studio listening controls and the two/four-stem choice across normal restarts. Save changes atomically after 750 ms of quiet and flush on normal close; **Save settings** saves both files immediately. Preserve corrupt files and block their autosave until an explicit save, with visible errors. This does not recover audio, queues or projects; Studio work still needs **Save project**. Browser and MCP workflows and older released binaries are unchanged. [Details](docs/USER_GUIDE.md#remembering-desktop-settings).

## 0.5.0-beta.1 — Studio Preview

- Add desktop vocal/instrumental and four-stem separation with optional isolated CPU engine setup, verification/repair and preserved model provenance. Import aligned user stems without a model; rebalance, compare, undo and export separately.
- Add voice cleanup, learned stationary-noise reduction, de-essing, an optional quiet-gap gate and selectable time/frequency repair. Keep original audio, prepared previews and saved files distinct.
- Add optional isolated VST3 effect rendering with generic controls, mono/stereo handling, reported-latency compensation, bounded process supervision and separate plugin logs. Vendor editors/MIDI and universal plugin compatibility are outside this initial host.
- Expand MCP to 36 self-describing tools, including editing analysis, stem operations, projects, native effects and mastering promotion. Preserve immutable Studio origins/IDs across promotion and exact direct-stem lineage after project reload.
- Add private browser Studio for imported stems, rebalancing, cleanup, repair, A/B and WAV/ZIP saving. Enforce a separate 128 MiB combined decoded-input budget; desktop remains the flagship for neural extraction, plugins and longer projects.
- Show right-only stereo audio correctly in Studio waveforms and cache peak envelopes for playback redraws. Keep capture-verification artifacts distinct for each report and reject collisions before installing.
- Bundle the MCP companion, VST3 worker and connection helper in the Studio Preview installer. Use separate preview folders/registration/shortcuts, retaining stable v0.4.2 and preserving user audio/projects on upgrade/uninstall.
- Validate the beta source with 94 Windows / 96 Linux native tests, MCP regressions and real stdio/model/plugin workflows. Browser Studio passes Chromium, Firefox and Linux WebKit; physical mobile playback and broad third-party effect qualification remain separate.

## Earlier source development after 0.4.2

- Add an optional local stdio MCP companion preview with 16 tools for scoped audio import, analysis, deterministic EQ suggestions, mastering previews, EQ revisions, explicit non-clobber exports, progress/cancellation and desktop session handoff. Keep the desktop as the flagship; no models, stems or cloud audio service are bundled.
- Share native preview/EQ preparation between desktop and companion, preserve the existing DSP sample path, and verify handoff source files with SHA-256 fingerprints. Add job, access, input-budget, cancellation and wire-protocol regressions plus MCP setup/privacy documentation. The published v0.4.2 installer is unchanged.

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
