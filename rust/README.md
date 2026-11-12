# SlimeyBeats-EZ-Master

For screenshots, example audio and the current Windows installer, start with the [project README](../README.md). This file is the native developer/operation reference. Original project code/docs/artwork are [MIT licensed](../LICENSE), subject to separately credited asset/dependency terms. The app claims no rights or royalties in your audio; see [output ownership](../OUTPUT_OWNERSHIP.md).

Native Windows desktop mastering and batch CLI. The Python application and its settings remain available alongside this version.

## Start

From the parent Master-Craft folder, open `run-slimeybeats.bat`. For batch processing, use `cli-rust.bat --help`.

Desktop flow:

1. Add or drop audio files.
2. Use **Analyze** at the top right to inspect the inputs.
3. Select a mastering profile or reference, set the loudness/peak limits, then **Master queue**. Gentler and Louder select the profile's two loudness intents; services apply their own playback normalization.
4. Compare Original/Master with loudness matching. On Manual EQ, use **Suggest EQ**, inspect the bone-colored curve, and optionally **Apply suggested EQ**. Edit any of the seven bands while playing: the live EQ updates immediately. Change mastering settings and use **Re-master queue** when needed.
5. Expand **Export settings** in the right pane to choose formats and an output folder, then **Export selected master**. Mastering and EQ previews stay in memory until this explicit action.

Exports never replace existing files. WAV 16/24/float, FLAC 16/24, and MP3 320k are supported. Integer exports use optional deterministic TPDF dither. Every encoded file is decoded and metered, with further attenuation/re-encoding if needed to respect the ceiling. The report distinguishes the in-memory master from decoded export measurements.

WAV processing needs neither Python nor FFmpeg. FFmpeg/ffprobe on PATH are required for compressed input, FLAC/MP3 export, and sample-rate conversion. Mono and stereo inputs from 8–384 kHz are accepted; multichannel audio is rejected with an explicit downmix message. Files under 400 ms receive peak protection rather than a reliable integrated-loudness master.

## Build and check

```powershell
cargo build --release --bins
cargo test --release --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
python tools/verify_exports.py
```

The final command is a development check using Python's standard library and the independent FFmpeg EBU meter; Python is not an application runtime dependency. For a CLI-only build, use `cargo build --release --no-default-features --bin slimeybeats-ez-master-cli`.

```powershell
target/release/slimeybeats-ez-master-cli.exe song.wav --profile metal --out Masters
target/release/slimeybeats-ez-master-cli.exe song.wav --profile "metal:0.6+edm:0.4" --format wav24,mp3 --out Masters
target/release/slimeybeats-ez-master-cli.exe song.wav --reference reference.wav --match-strength 0.7 --out Masters
target/release/slimeybeats-ez-master-cli.exe song.wav --analyze-only --report-json analysis.json
target/release/slimeybeats-ez-master-cli.exe song.wav --benchmark 3
target/release/slimeybeats-ez-master-cli.exe refs/*.wav --calibrate --name MyRefs --base neutral
```

CLI runs master and export as one batch command. The desktop keeps these actions separate. `--benchmark` times processing only, omits exports, and prints the complete last report plus individual run times. Batch exit codes are 0 for success, 1 for partial failure, 2 for invalid options/settings, and 3 when every input fails.

## Compatibility and sound

The sixteen built-in genre profiles are embedded from the existing `mastercraft/data/builtin_profiles.json`; custom profile JSON and weighted blends are supported. Calibration remains available in the CLI. Legacy settings import preserves common targets, formats, module switches, and the supported tuning parameters. Rust settings are saved separately to `user_data/rust-settings.json`. Original settings and presets are never migrated in place.

This is revised DSP. The output will sound different from Python. Changes include a bounded spectral analysis front end, native EBU loudness/true-peak meters, stricter stationary-hiss triggering, a revised transient detector and punch guard, ten-band reference fitting, and the corrected sub-duck hold duration. These changes contribute to the whole-pipeline speed difference; benchmark ratios do not establish algorithm equivalence or better sound.

Live EQ streams from the untouched automatic master through seven stateful filters and a lookahead peak guard. Parameter changes warm a new filter state and crossfade smoothly. A background worker prepares the full-track measured revision after editing pauses, without blocking auditioning. A/B changes crossfade and match by attenuation, with no positive comparison gain. The original app's musical note labels, EQ undo/redo and macro controls, optional FIR reference realization, and detailed pre-echo/shimmer/spectral-hole diagnostics are not reproduced. Keep the Python app for those features.

Genre correction and artifact scores remain heuristics. Automated loudness, clipping, duration, and export checks passed; subjective listening across a music library remains a separate acceptance step. Processing retains full tracks in memory, so this is not a bounded-memory streaming tool.

Version 0.4.1 addresses the real-music audit's peak failure with a longer 8× reconstruction detector shared by the limiter and final measurement, including file-boundary tails. Automatic profile EQ now scales capped proposals with intensity and limits the combined cascade; dynamic correction accounts for existing static cuts. See `docs/DSP_SAFETY.md` for ownership and limits, and `docs/CORPUS_VALIDATION.md` for the original failing cases and subsequent verification. Finite-filter cross-checks do not establish universal reconstruction or EBU conformance.

See `docs/ARCHITECTURE.md` for ownership and data flow and `docs/REVIEW.md` for the source assessment, benchmark evidence, checks, and visual-review results.

## SlimeDragon theme and beginner help

The obsidian/amethyst/lavender interface uses teal slime and bone accents from the supplied character sheets. A simplified head icon is used in the header and window; the detailed dragon portrait is reserved for larger displays. Original reference sheets are unchanged. Production assets, prompt provenance, intended sizes, and review boundaries are recorded in `assets/ASSETS.md`.

Version 0.4.0 follows the supplied SlimeDragon Audio concept: generated silver/amethyst window and panel frames, jewel crests, restrained purple ribbons, serif display headings, faceted purple controls, diamond slider handles and framed welcome steps. Native buttons, values, labels, plots and tooltips remain editable and functional. Hanging slime is confined to the header/footer, retaining its proportions and downward orientation. The previous side-slice offset is removed by a shared nine-slice grid and visible-alpha corner regression tests.

The default window is 1480×940 logical pixels and supports resizing down to 1100×720. Smaller windows use wrapping EQ fields, a compact pinned waveform and visible scrollbars. Playback and Master/Export actions stay outside scrolling settings. Export configuration defaults closed, with a summary of the selected formats; opening it preserves normal control behavior. The product/window remains SlimeyBeats-EZ-Master under the SlimeDragon Audio wordmark.

Hover a technical control name or control for plain-language guidance: LUFS, peak ceiling, intensity, frequency, gain, Q/width, filter shapes, reference matching, dither, formats, module switches, and preview/export actions all have explanations. Hints describe useful starting points and trade-offs without claiming an automatic setting is always correct.

## Deterministic suggested EQ and playback inspection

**Suggest EQ** measures the automatic master, or the original input if no master exists. The pure suggestion function uses 25% profile residual correction plus a small local spectral-shape term. Moves are rounded to 0.25 dB, capped at ±2.5 dB per band, with bass/air boosts capped at +1 dB per band. Overlapping filters combine; these are not whole-response limits. Unavailable high-frequency bands are disabled. Silence, excerpts under one second, and fewer than three valid core bands produce an explicit no-suggestion state without discarding the master. Identical decoded samples and profile produce repeatable suggestions on the same build/platform; cross-platform floating-point/decoder identity has not been qualified. These thresholds are product heuristics. This is rule-based assistance, not a learned model or a promise of better sound.

The proposal appears separately from the editable curve. **Apply suggested EQ** copies its bands to the editor and updates playback immediately. The purple EQ response and gray/teal spectrum share one graph with clearly identified axes. During playback, the teal FFT reads the actual live EQ stream before listening volume or A/B level matching. At the paused start it shows the measured full-track average; paused seeking shows the playhead window. The pinned waveform reflects the latest full-track measured revision and updates automatically after editing settles. Its playhead and click-to-seek remain available while the controls scroll.

A/B on Manual EQ isolates the live EQ against the automatic master. A/B on Overview compares the original against the final master. Playback position is preserved across these views and EQ revisions. Exporting does not restart playback. Re-analyzing an unchanged source keeps its existing master.


Live EQ uses bounded history for filter warm-up, 20–300 ms adaptive crossfades, an 80 ms gain transition, and roughly 2 ms of peak-guard lookahead. Audio-device buffering contributes additional latency. **Compensate EQ level** uses a spectral estimate during dragging, then resolves with full EBU measurements. It affects the master; listening-only loudness matching does not. Export snapshots the current controls and resolves them if background preparation is not yet current. Old worker results cannot replace a newer revision or a different selected track.

The live and fixed export paths share the same processor. Tests compare their samples and exercise parameter changes, fades, peak limits, duration, and analyzer placement before listening volume. This is not a hard real-time scheduling guarantee across every Windows audio device. Listening review and broader device/high-rate stress qualification remain separate acceptance work.

To launch directly into an editing view, use `--view eq` (or `overview` / `report`), for example `run-slimeybeats.bat song.wav --auto-master --view eq`.

For repeatable visual QA, a development-only `--capture-ui PATH.png` captures the settled native renderer and closes that diagnostic instance. The destination must be new and its parent must already exist. Combine it with `--auto-master --view eq` on a test input; `--capture-size 1100 720` checks the minimum window. `--capture-settings-scroll 520` opens only Export settings and requests a scrolled capture; `--capture-workspace-scroll 225` shows the wrapped EQ controls. Neither transient option changes saved settings. These options are for diagnostic instances, not an interactive editing session.

Primary-source research and ranked future improvements are in `docs/research/tonal-profiles-mastering-eq-2026-11-12.md`. Existing genre curves have not been validated against a reference dataset. Neutral is a processing preset, not bypass, and a spectral bump is not proof of a defect. The queue processes tracks independently; it is not an album-level normalization workflow.
