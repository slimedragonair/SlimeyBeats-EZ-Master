# Changelog

## 0.5.0-beta.3 Hotfix4 — 2026-11-15

- Prepare Windows Studio Preview build revision **2026111501**, keeping desktop **0.5.0-beta.3**, MCP **0.2.0-beta.3**, browser Studio **0.3.0-beta.3** and release tag **v0.5.0-beta.3**. Use immutable `-Hotfix4-Setup.exe`, `-Hotfix4.zip` and `-Hotfix4-Source.zip` assets; preserve earlier packages and the original tag.
- Include the shared live-EQ preparation improvements in desktop: warm filter history in bounded work while retaining current playback, then crossfade to the prepared settings. Browser-only worklet revision coalescing remains in the browser adapter.
- Include the desktop stem timeline, precise selection, fit/zoom, loop auditioning and undoable silence/fade/gain/reverse edits. Preserve aligned parts and separate project, preview and export actions.
- Include Stem Doctor's timestamped issue scans and explicitly requested conservative repair previews. Scans do not trim, repair or export automatically.
- Include the matching **45-tool** MCP companion with `edit_stem_region`, `scan_audio_issues` and `preview_stem_doctor`. Historical Hotfix3 binaries retain 42 tools.
- Document the main web app's already shipped shared DSP, time edits, Doctor, local MP3, EQ preset and session ZIP exports. A desktop refresh does not imply a new browser deployment or browser-to-desktop bridge.
- Keep release-specific test results, artifact identities and private-identity Hotfix3 → Hotfix4 installer QA in the external [HOTFIX4-VALIDATION.json](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.5.0-beta.3/HOTFIX4-VALIDATION.json) report. Source-feature checks and earlier Hotfix3 installer results remain historical; see [Hotfix4 scope and evidence](docs/HOTFIX4.md).

## 0.5.0-beta.3 — 2026-11-14

### Hotfix3 — local recovery and private reporting

- Add local native autosave and three transactional restore points per session, with an 8 GiB shared budget, exact audio/metadata checks, active-window locks and explicit inspection/restore/deletion. Keep original inputs, projects and exports separate.
- Preserve loaded queue sources, prepared masters, pending EQ and Studio controls; re-master recovered sources after the original moves. Back up replaced work as a separate session, keep live mute/solo active, and protect close during save/restore/clear. Do not resume playback or running jobs.
- Add opt-in browser audio recovery, atomic bounded IndexedDB storage and explicit private recovery files. Preserve editable Studio banks, reject stale imports/restore races, and keep recovery/storage limits separate from editing limits.
- Add locally generated support previews with typed minimal diagnostics, best-effort manual-text filtering and reviewed exact Copy/Save. No automatic audio/log/path collection, uploads or issue posting. Keep public issues and private security reporting distinct.
- Add MCP report preparation and reviewed saving, for 42 tools. Report compiled companion/linked-engine identity accurately without inferring installed GUI state or accessing private desktop recovery storage.
- Preserve exact floating-point editing values and checksum the stored metadata bytes before parsing. Document privacy choices, storage boundaries, matching-build constraints and remaining device/clipboard qualification.

### Hotfix2 — easier updates and optional setup

- Add **Help → Updates & tools**, once-daily optional checks, verified background downloads and an explicit saved-work/Setup flow. Build revision distinguishes same-version hotfixes; immutable per-revision catalogs preserve previous releases. Portable/Linux manual routes stay explicit.
- Prepare private FFmpeg/ffprobe for the action that needs compressed input/export or resampling, without global PATH changes. Reuse capable system tools; verify FLAC/MP3 encode/probe/decode and share the prepared private cache across processes.
- Chain missing stem-engine setup directly into source separation with cancellation and monotonic progress. Keep existing-stem imports, voice cleanup and spectral repair independent of the separation model.
- Recover missing application-owned VST3/MCP helpers on use, with bounded/hash-checked bundles and original notices. Do not download third-party plugins or edit AI-client settings.
- Expose optional-tool discovery/preparation through the MCP companion, preserving explicit owner download policy and custom profile roots. Correct the native live-mix footer description.
- Keep desktop 0.5.0-beta.3, MCP 0.2.0-beta.3 and browser Studio 0.3.0-beta.3. See [workflow and candidate evidence](docs/UPDATES_AND_OPTIONAL_TOOLS.md).

### Guided interface and Help

Studio Preview beta.3 includes the guided desktop interface, bulk stem imports and remembered preferences. Its installer remains separate from stable v0.4.2; the MCP companion is 0.2.0-beta.3 and browser Studio is 0.3.0-beta.3. Earlier releases retain their original binaries and qualification records.

- Start in Basic, reveal detailed controls with Advanced, and flag active hidden custom settings without resetting audio settings. Pair native Studio stem waveforms and faders in the workspace while retaining explicit preview/apply/export actions.
- Add bottom-right Help, native F1 access and an optional eight-step tour with replay, terms and troubleshooting. Save completed, declined and dismissed choices immediately, with separate native interface preferences and a tutorial-history marker that protects against stale windows.
- Add explicit desktop AI setup: choose audio/output folders, generate the real companion's `--print-config` JSON, then copy it into the client's supported setup. Do not edit client settings or start a server automatically. Browser MCP Help remains desktop-only guidance, without a bridge.
- Share tour/glossary/MCP copy across native and web. Browser storage retains only small interface choices, with a visible session-only fallback and automatic guidance disabled when storage fails. [Guide and implementation ownership](docs/LEARNING_AND_HELP.md).
- Put selected-band EQ controls in a sidebar inspector, reserve measured playback space and keep compact waveform traces readable. Move Studio voice/repair editors into the workspace, pin Preview before Export, and offer dense named stem rows with optional full-size plots. Use readable export-format labels and part-specific accessible gain names.
- Preserve precise slider values during untouched rendering; retain native clipboard, links and accessibility integration. Measure platform-font tabs/disclosures, prevent fractional-DPI pane growth, and paint popup/modal content above decorative artwork.
- Improve enlarged phone navigation, first-import and tour choices, compact playback, pending-mix feedback and Help answer/search focus. Keep terminal browser tutorial choices safe from interleaved stale-tab writes, with explicit storage errors.
- Keep modified Space chords out of transport handling, prevent held-key playback toggles, and reserve Alt+Space for the native window menu without activating a focused tutorial choice.
- Reveal completed spectrograms once, retain independent Studio page positions, and put plugin choices/controls in the workspace. Add current illustrated guidance and [candidate-specific UI validation](docs/UX_VALIDATION.md).

### Studio, imports and remembered preferences

- Release desktop **0.5.0-beta.3**, optional MCP companion **0.2.0-beta.3** and browser Studio **0.3.0-beta.3**. Stable v0.4.2 and earlier beta packages remain unchanged.
- Remove the fixed count cap for aligned imported stems/mix tracks. Loose-file and Suno/stem ZIP imports select all distinct parts by default, with optional selection and unique `part_N` identities for additional parts. At least two parts with matching sample rate, channels and frame count are still required; neural separation remains two/four stems.
- Keep separate import budgets: native defaults to 512 MiB decoded PCM per input and a 4 GiB aggregate decoded bank; browser Studio retains 128 MiB combined decoded input and 15 minutes per track. Available RAM, archive/metadata budgets and working buffers still constrain capacity.
- Expand the optional local MCP companion to 38 tools, including read-only stem ZIP inspection and job-based import. Preview preparation and explicit audio/project saving remain separate. See the [current Studio guide](docs/STUDIO.md) for workflows and qualification scope.
- Preserve accented/CJK paths in Windows MCP setup, require pending faders to be applied before further Studio edits/saving, and keep saved MCP projects tied to the current preview's actual source and stem bank. Add native, Windows PowerShell and three-engine browser regressions; see [merge validation and availability](docs/MERGE_VALIDATION.md).
- Remember desktop mastering settings and independent Studio export preferences, master/Studio listening controls and the two/four-stem choice across normal restarts. Save changes atomically after 750 ms of quiet and flush on normal close; **Save settings** saves both files immediately. Preserve corrupt files and block their autosave until an explicit save, with visible errors. This does not recover audio, queues or projects; Studio work still needs **Save project**. Browser and MCP workflows and older released binaries are unchanged. [Details](docs/USER_GUIDE.md#remembering-desktop-settings).

### Live stem monitoring hotfix — same version

- Make stem gain, mute, solo and stereo balance audible during native and browser playback without rendering a preview first. Changing a stem control selects Preview/B automatically; settled B contains only the preview/live mix, with no hidden Before layer. Native gain changes use a 5 ms ramp and streaming peak guard; browser monitoring uses smoothed gains and conservative sample headroom, with final true-peak protection on materialized mixes/exports.
- Keep **Update mix preview** explicit to render and measure the current balance before preview exports, project saving, mastering handoff or further voice/repair/plugin processing. Disable native level matching for the unmeasured live mix; label the comparison waveform as the saved preview while live faders are not drawn.
- Centralize native playback ownership across workspace tabs, ZIP-drop navigation and tutorial navigation so hidden mastering and Studio players cannot overlap. Preserve aligned-bank requirements, input/memory budgets, unrestricted imported-part counts and two/four-stem model scope. [Behavior and implementation](docs/STEM_LIVE_MONITORING.md).
- Retain desktop **0.5.0-beta.3**, browser Studio **0.3.0-beta.3** and companion **0.2.0-beta.3**. This source hotfix does not establish new test results, installer qualification or publication; earlier beta.3 validation applies only to its recorded candidates.

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
