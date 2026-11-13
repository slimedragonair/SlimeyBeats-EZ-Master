# Studio editing preview

Studio extends the desktop mastering workflow with separate editing stages. It is development source, undergoing qualification; the published v0.4.2 installer does not contain these tools. Existing mastering and real-time Manual EQ remain available. Processing a preview and exporting a delivery file remain separate actions.

## Desktop workflow

1. Open **Studio**, choose a local recording, or use the selected mastering-queue track. **Analyze** at the top right measures the selected Studio input.
2. In **Stem Studio**, choose **Vocal + instrumental** or **Four stems** (vocals, drums, bass and other). Install the optional local engine once, then **Separate source**. Setup downloads a private CPU runtime and verified upstream model; separation uses local files. The optional environment does not alter global Python.
3. Alternatively, expand **Import your own stems** and assign aligned files. Matching sample rate, channels and frame count are required. Imported stems need no neural model.
4. Adjust gain, mute/solo and stereo balance; **Update mix preview** renders from the bank. A muted part stays muted even if soloed. Repeated fader changes do not accumulate processing.
5. Use **Voice cleanup** on a voice recording or vocal stem. Start gently with rumble removal, noise reduction and de-essing. A noise-only region can teach stationary noise; the optional quiet-gap gate reduces low-level gaps. Cleanup does not itself isolate speech from music.
6. In **Spectral repair**, choose the source and build its spectrogram. Drag a rectangle or enter exact seconds/Hz, then preview attenuation or healing. Bright energy is not automatically a defect. Healing estimates surrounding spectral magnitudes; it cannot reconstruct missing words or instruments.
7. A direct stem edit can be **applied to the selected stem**, rebuilding the full mix while retaining the other stems. Apply each direct edit before chaining another. Undo/redo retains a bounded set of recent revisions.
8. Compare **A · Before** and **B · Preview**. Listening volume and level matching do not alter exports. **Use for mastering** creates a private working source for the existing mastering queue. It does not save a final master.
9. **Export current preview** saves the prepared revision and report. Individual stem export saves aligned 32-bit float WAVs at their relative bank levels; fader/solo changes affect the mix, rather than those raw files. Existing output files/folders are preserved.

**Save project** writes a new self-contained folder with source, stem, before/preview WAVs, mix controls and hashes. Open its `session.studio.json` to continue without the original library files or model runtime. Projects contain private audio: share them only intentionally. They retain the prepared audio and mix, not an entire undo history or vendor-plugin state.

The engine maintenance disclosure offers **Verify installed model** and **Repair stem engine**. Verification checks the installed model files. Repair reinstalls the isolated dependencies and model; unknown/user-added cache files are preserved in a backup. Setup and inference have cancellation and process supervision. Readiness is a quick status check, rather than continuous hashing of model files.

## Optional VST3 effects

Build/install the `plugins/host` helper beside the desktop/MCP binaries. Choose a trusted VST3 file or x64 bundle, probe its audio-effect classes, adjust discovered generic parameters and **Preview plugin effect**. Values use each plugin's normalized 0–1 mapping. Compare before exporting.

The initial host supports compatible mono/stereo float32 audio effects and offline preview rendering. It compensates reported latency and trims the result to the source duration. Native vendor editors, MIDI/instrument sequencing, arbitrary plugin compatibility and persistent vendor state are outside this initial scope. Plugins run native code in a separate process for crash/cancellation containment; this is not an OS security sandbox. Plugins may have their own network/licensing behavior.

## Browser Studio

The browser **Studio** tab supports a recording or imported vocal/instrumental/four-stem set, rebalancing, voice cleanup, a selectable spectrogram, repair, A/B playback, undo/redo, WAV export and a raw stem ZIP. **Use preview for mastering** loads a working copy into the existing browser mastering workflow while retaining Studio. **Close Studio** releases its workspace. Reloading/closing the page discards unsaved audio.

Neural separation and native VST3 hosting remain desktop features. Browser processing stays in local WebAssembly workers; user audio is not uploaded. Studio cancellation waits for its current bounded processing step, then discards the proposed revision, preserving the previous preview. This differs from the mastering worker's terminate-and-restore cancellation behavior.

Browser Studio has a **128 MiB combined decoded-input budget**, in addition to a 15-minute per-file ceiling and 128 MiB source-file bound. This is distinct from the original mastering page's 384 MiB limits. Decoding uses a 48 kHz context. At stereo 48 kHz the input budget represents approximately 5:49 for one recording, 2:54 per file for two equally long stems, or 1:27 per file for four. Mono files use half that PCM memory. The stricter applicable bound wins. Processing, history, A/B and export allocate additional memory; the input budget is not a total RAM cap or a phone guarantee. Desktop is recommended for longer stem projects.

WAV downloads use the shared export peak checks and may apply protective attenuation. Raw float stem ZIPs retain bank samples and relative levels. Pending tool settings are identified; **Export current preview** saves the already prepared audio. Preview again to hear new settings. Listening volume never changes saved samples.

## MCP workflows

The companion currently exposes 36 tools. Begin with `get_capabilities`, then `get_help` topics `stems`, `voice`, `repair`, `plugins` or `studio_export`. Discover current audio, stem-bank and preview IDs with `get_studio_session`; poll every returned job ID. The editing workspace is separate from the mastering preview.

Imported stems use `import_stems`; neural extraction uses `separate_stems`. `mix_stems`, `clean_voice`, `repair_audio` and `render_plugin` create temporary previews. `apply_preview_to_stem` applies a direct current-stem edit. Explicit saving uses `export_studio_audio`, `export_stems` or `save_studio_project`. `open_studio_in_desktop` creates a self-contained project for listening/refinement. `use_studio_for_mastering` promotes the prepared mix to the original mastering flow. Desktop and AI sessions evolve independently after handoff.

Plugin execution additionally requires owner-configured `--plugin-root` folders; audio-library access alone does not authorize native plugins. The AI client controls its own handling of filenames, measurements and tool results. No audio bytes are returned through MCP. See [MCP setup/privacy](MCP.md).

## Validation status — November 13, 2026

| Scope | Observed result |
| --- | --- |
| Shared audio DSP | Eight focused regressions passed for stem reconstruction/mixing, cleanup, spectrogram behavior and localized repair. Samples outside the selected repair time remain unchanged before later export stages. |
| Local model | Real CPU separation passed in two/four modes, including a multi-chunk fixture and a 45-second source-rate adapter test. Aligned finite stems and neutral reconstruction were checked. This is not a listening-quality benchmark. |
| Native/MCP | Real stdio tests passed editing, stale-ID rejection, undo/redo, explicit exports, imported stems, direct stem replacement, mastering promotion, self-contained projects and desktop handoff. Source hashes were retained. |
| Current native regression | 93 Windows native tests and eight MCP regressions passed; Windows/Linux shared-source Clippy passed with warnings denied. The original stdio workflow passed all six export formats. New project regressions preserve direct stem-edit lineage after reopening. |
| Runtime recovery | A real installed test cache passed verification, detected same-size model-index corruption, then completed explicit repair, reinstallation and inference self-test with the original index hash restored. Four extraction/cache regressions passed, including preserving malformed provenance in a backup. |
| VST3 | External gain fixtures passed mono/stereo processing, generic gain changes and a fixed 137-frame latency case, with sample-exact expected gain after compensation and source duration retained. Existing files were preserved. A native fault module passed null-factory failure, plugin-initiated worker exit and cancellation of a stalled factory; the MCP parent retained its source and could analyze again. This covers compatible fixture effects, not arbitrary third-party plugins. |
| Browser Studio | Chrome 156, Firefox 144.0.2 and Linux WebKit 26.0 passed real UI/worker workflows: two/four imports, gain/mix, cleanup applied to one stem while preserving others exactly, repair, A/B, undo/redo, WAV/ZIP export and mastering handoff. Failed/cancelled edits and misaligned imports retained the previous preview. 390px layout/reachable export checks passed. |
| Browser regression | The original Chrome mastering/EQ suite passed, including AudioWorklet samples matching shared Rust exactly, three WAV formats, reference invalidation, cancellation/late decode safety, offline analysis and unchanged exports at different listening volumes. |
| Fresh-agent MCP | An agent with no project/chat history imported four stems, cleaned and applied the vocal edit, retained other stems, rebalanced and prepared a preview using only live instructions/schemas/help. It did not export until a later explicit save instruction, then saved WAV/report, raw stems and a self-contained project and closed cleanly. A missing returned bank ID it reported was fixed. This qualifies the tested protocol harness, not every AI client/model. |
| Browser visual review | Independent inspection of actual desktop/390px repair captures scored theme 9.2, polish 9.0 and readability 9.1 after fixing overflow, annotation size, selection contrast and active-workspace status. Native Studio screen review remains separate. |
| Import bounds | Actual WASM staging accepted 128 MiB combined and rejected one frame over. A 900-second 8 kHz mono binding input was accepted and one frame over duration rejected; a rejected load preserved the prior source. This tests binding guards, not full-length physical-phone processing. |
| Studio replacement soak | Chrome completed 60 × 10-second source replacement/cleanup/undo/redo/download cycles without Close or forced GC. Studio WASM capacity remained 12.5 MiB; isolated browser private memory peaked at about 745 MiB. Private memory grew during the run, including retained download/engine allocations, so this is a bounded successful soak, not proof of a total-memory plateau or a phone maximum. |
| Remaining qualification | New native screen review, additional plugin/process fault cases, final Windows/Linux/package checks, unfamiliar-agent MCP review, release packaging and physical mobile playback remain open. Windows WebKit stalled; the maintained Linux WebKit environment completed the suite. |

Reproduce browser checks with `python web/tools/verify_studio_browser.py --out PATH`, selecting `--browser firefox` or `--browser webkit` as needed. `verify_studio_memory.py` exercises repeated replacement, cleanup, undo/redo and downloaded export without forcing garbage collection; `--check-limits` additionally stages the actual boundary buffers. Existing `verify_browser.py` remains the mastering/live-EQ check. CI runs the Studio suites separately. A desktop browser engine or a synthetic audio sink does not qualify a physical iPhone/Android device.

## Implementation

Shared DSP lives in `rust/src/studio`; native runtime, project and plugin adapters in `rust/src/studio_native`; desktop state/jobs/controls/plots in `rust/src/gui/studio`. The VST3 worker is a separate crate in `plugins/host`. MCP Studio routes are separate from the original mastering routes.

The browser's `web/dsp/src/studio.rs` owns immutable PCM, revisions and shared DSP. `studio-worker.js` prepares proposed revisions on a fork sharing immutable audio; the controller commits only a successful, still-current proposal. Worker errors and cancellation discard the proposal. `studio-client.js` owns request IDs, `studio-player.js` owns independent audition buffers, `studio-controls.js` and `studio-selection.js` own forms/selection, and `studio-plots.js` draws actual audio/spectral data. `studio-zip.js` produces bounded local ZIPs without a new dependency. The existing versioned JS/WASM asset graph includes these modules.
