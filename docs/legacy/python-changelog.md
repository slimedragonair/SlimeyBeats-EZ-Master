# Changelog

## Unreleased (after 0.2.2): low-end punch, tilt-first EQ, 16 profiles + blends, reference matching, multi-format export

Measured review and the prioritised "what next" list: `docs/improvement_plan.md`.

- **Reference-track matching** (new). `--reference song.wav` or the GUI's "Reference track" row makes a
  master sit on another track's tonal balance; `--match-loudness` takes its loudness too, `--no-match-width`
  keeps the profile's width. The reference becomes the tonal target for the analysis (mud/low-end/tone are
  judged against it), while the profile still supplies dynamics, punch caps, stereo and clipper.
  - Own implementation (`mastercraft/reference.py`): loudest 50 % of active frames, 1/6-octave smoothing on
    a 1/12-octave grid, level-aligned over 100 Hz-10 kHz, ±6 dB clamp, no sub-boost, no boost above the
    codec cutoff, width matched within ±3 dB.
  - Realised as **≤7 broad minimum-phase bells (Q ≤ 1.4)**, not a matching FIR: the FIR fit the curve
    slightly better but cost **1.7 dB of measured punch**, so the bell chain is the default
    (`modules.reference_match.realize = "biquads"`, `"fir"` remains available).
  - The report has the band distance to the reference **before and after**, and `reference_match` is a
    punch-guard back-off candidate (it halves the match rather than smearing hits).
  - Measured (RuneCore vs Wait for the Ground, metal/loud): band distance 4.8 → 0.8 dB RMS, punch score 1.3
    (limit 2.0), -10.03 LUFS / -1.20 dBTP. Matching a song to itself changes it by 0.000 dB.
  - New tests y1-y3 (null test, moves toward the reference, bounded/ceiling-safe).
- **Multi-format export.** One master can be written as several formats in one run: tick them in
  **Export → Formats** or pass a comma list (`-f wav24,mp3,flac16`).
  - Unambiguous file names: the primary format of an extension keeps the plain name, the variants carry a
    tag (`song_master.wav`, `song_master_16bit.wav`, `song_master_32f.wav`, `song_master.flac`,
    `song_master_16bit.flac`, `song_master.mp3`), so nothing can overwrite anything else.
  - Labels say what each format is for (WAV 24-bit "master", WAV 16-bit "CD, TPDF dither", WAV 32-bit float
    "editing", FLAC 24 "lossless archive", FLAC 16 "small lossless", MP3 320 "streaming / sharing"), the
    panel shows a live preview of the exact file names a run will write, and the queue row reads
    "mastered · 3 files" with every path in the tooltip.
  - One JSON report per written file (`song_master.report.json`, `song_master_mp3.report.json`, ...); the
    CLI lists channel `-> file (format label)` for each.
  - **Manual EQ → "Export with EQ"** writes every ticked format too; the button shows the file count and the
    tooltip lists them.
  - New tests z1-z2 (naming/uniqueness, CLI list + bad-format error).

- **Low end ("slap harder, stay tight").** The chain used to lift 30-80 Hz by +1.3…+2.5 dB while
  taking 2.1…2.9 dB out of 160-400 Hz: a woolly low end with no punch. Fixed in four places:
  - **New `low_punch` stage** (after saturation, before stereo/limiter), the only stage that adds
    attack: a kick-band (50-160 Hz) transient shaper (+2 dB attack on the part of the band that rises
    above its own sustain, -1.5 dB on the tail) plus a kick-triggered sub duck (22-60 Hz, 90 ms,
    2 dB). Both scale with the measured loudness headroom and mud/low-end severity and are capped per
    profile (`low_punch_attack_max_db`, `low_punch_duck_max_db`). Detector: fast (0.5/20 ms) minus slow
    (30/150 ms) follower. A centred RMS comparison or a causal gain ramp misses the kick peak and
    measured +0.1 dB instead of +2 dB (`test/test_lowpunch_synthetic.py`).
  - **The bass fix is a bell on the kick body** (`low_end.body_hz`, 105 Hz) instead of a 90 Hz shelf, and
    bass excess is cut at 78/110 Hz, so the sub is not lifted together with the body.
  - **`punch.mud_static_scale` / `punch.mud_min_hz`** on Metal, Rock, Neutral and the new punch profiles:
    the static mud bell is scaled and never sits below 200 Hz (it used to place a -4.5 dB bell at 267 Hz,
    which also took punch out of the kick).
  - **Kick-body guard** (`engine._guard_low_mid`): the planned EQ response is measured at 63-250 Hz; if it
    would take more than 1.8 dB out of 100 Hz, 2.0 out of 125 or 2.6 out of 160, all low cuts are scaled
    back together (at least 50 % of the planned depth) and the action is reported.
  - Result on the two reference songs (loud mode, 45 s excerpts): 30-80 Hz +2.5 → -0.1 dB and 80-160 Hz
    crest -0.54 → +0.15 dB (Wait for the Ground); 160-400 Hz hollowing -2.9 → -1.8 dB; kick TSR -0.18 →
    +0.13 dB, full-mix TSR -0.34 → +0.38 dB; sub-vs-bass tightness -1.6 dB.
- **Automated EQ (tilt first).** One straight line is fitted to the deviation over 125 Hz-8 kHz; above
  0.75 dB/oct the slope is corrected with a low shelf (200 Hz) + high shelf (3 kHz) *before* any bell, and
  only the residual goes to the octave bells (research2 §1.4 / §3.1 step 5). No HF boost on
  artifact-heavy material. A correction that would swing more than 6 dB peak-to-peak is scaled down and
  reported as "mix is far from the target" (§3.1 step 7).
- **Every EQ filter now carries a reason** (`"why"` in the plan/report JSON, shown in the report tab
  together with the tilt, the predicted low-mid response and any guard action).
- **Multiband timing from the crest factor** (research2 §2.4): attack/release follow the measured crest
  and can only be longer than the profile value (punch first).
- **Punch metric fixed**: the transient/sustain window now ends before the next hit (0.7 x the interval),
  so the ratio stays meaningful above ~6 hits/s. Absolute TSR values change; the 2 dB limit still means
  the same thing because like is compared with like.
- **Profiles: 11 → 16 built-ins** - Alt Rock, Metalcore/Djent, Pop-Punk, Synthwave/Retro,
  Game/Instrumental Score.
- **Blends (combos)**: `-p "metal:0.6+edm:0.4"`, `"lofi:0.5+game_ost:0.5"`, `"rock:0.6+pop:0.4"`;
  weights normalise and short aliases work (`game`, `retro`, `djent`, `alt`, `trap`, `indie`…). A blend
  mixes the tonal curve, loudness, glue/multiband timing, stereo, saturation, clipper and the punch caps
  (caps take the most conservative parent). GUI: **Blend…** button with a live target preview, saved as a
  preset. The analysis also suggests a blend (non-negative least squares over the 16 curves,
  `blend_suggestion`).
- **"Already mastered" input** (loud + dense + on the ceiling) is reported as a note instead of being
  silently re-limited.
- **New tools**: `test/diag_lows.py`, `test/sweep_lowpunch.py`, `test/test_lowpunch_synthetic.py`.
- **Tests**: 5 new regression tests (x1-x5) for the shaper, the kick-body guard, the tilt/span cap,
  blends/aliases and the stage-in-chain; 44/44 pass. Two existing tests were adjusted: the preset-dir
  fallback test now cleans its folders (it failed on a second run), and the MP3 "encode + fix cost"
  tolerance went 0.15 → 0.2 dB (the denser low-end master costs ~0.15 dB there).

## 0.2.2 (QA pass on 0.2.1: window size, low-frequency EQ jumps, OUT fader, presets folder)

- **Window size / responsive layout.** Minimum window is now 873x515 logical px (was ~1126x700), the same at 100/125/150 % scaling (device px 873x515 / 1091x644 / 1310x772), so it fits 1366x768 at 125 % (~1093x545 usable). The Manual EQ toolbar wraps (flow layout) instead of two fixed rows, the EQ graph shrinks to 220x150 and the gain/OUT faders to 80 px, the EQ tab scrolls vertically on short screens, the Spectrum/Waveform views shrink, and the transport hint moved to the Play tooltip. The queue and settings panes are collapsible (splitter or the new Queue / Settings toggles in the status bar; hiding both gives 558x365). The window opens at 1480x920 or the available screen size, whichever is smaller.
- **Settings panel never scrolls sideways.** Forms wrap long rows, combo boxes size to 10 characters, the profile buttons sit on their own row, two long check box labels were shortened (full text in the tooltip), and the panel's minimum width is fitted to its content plus the vertical scrollbar (now ~309 px; the content needs 285 px, was 477 px).
- **Low-frequency, high-Q EQ changes no longer jump.** Each filter chain computes the decay time constant of its slowest pole (tau ~ Q/(pi f), ~0.14 s for 40 Hz Q18). The new chain is warmed on 8 x tau of recent audio (170 ms to 1.5 s), the crossfade lasts 1.5 x tau (20 to 300 ms), and Play/seek prime on the same longer history. Worst case on the QA cases: +3.44 dB -> +0.11 dB (5 ms RMS above the louder steady state); see `test\eq\overshoot_results.json`.
- **OUT fader / hold gain changes are smooth.** Gain-only changes keep the filter state and fade, linearly in dB over 80 ms, from the current output to a second true-peak guard primed on the recent audio at the new gain. A plain gain ramp still overshot (+1.4 to +2.4 dB) because the guard's release lagged; with the primed guard, OUT 0 -> +12 goes +1.89 dB -> 0.00 dB.
- **Presets folder fallback.** If the presets folder cannot be written, the EQ tab says so and uses the next folder in the chain: `MASTERCRAFT_DATA_DIR`, then `F:\Apps\Master-Craft-data`, then `<app>\user_data`, and only as a last resort `%APPDATA%\Master-Craft` (system drive), which needs your OK in a dialog first. The default location is unchanged.
- Tests: new `eq_preset_dir_fallback`, low-frequency high-Q and OUT-fader cases in the overshoot regression, `test\eq\layout_test.py` (minimum sizes at QT_SCALE_FACTOR 1/1.25/1.5, offscreen, with system fonts).

## 0.2.1 (QA pass on 0.2.0: EQ ear safety, M8, silent master, memory, lows, layout)

Fixes from `test\qa\v020\QA_V020_REPORT.md`, in its priority order.

- **1. Ear safety (EQ transitions).** The live EQ no longer glides coefficients.
  - Every edit builds a new, fixed filter chain (biquads + hold gain + output fader). The chain is warmed on the last 170 ms of audio and crossfaded in over 20 ms (raised cosine).
  - Every output sample of a transition lies between the old and the new EQ's sample, so a transition can never be louder than the louder of the two steady states. Edits that arrive during a fade are queued (the newest wins).
  - While stopped, edits snap. Play, resume-from-stop and seek reset the processor at the current EQ and prime it on the audio just before the play position (5 ms fade-in), so there is no cold-filter burst.
  - Hold gain and output gain are finite and capped (+/-30 dB, +/-12 dB). NaN/inf input samples are zeroed.
  - Measured on RuneCore, worst overshoot above the louder steady state (5 ms RMS), before -> after: A/B flat<->all+18 +3.9/+6.5 -> 0.0 dB; flat<->all-18 +5.7/+5.7 -> 0.02; all+18 -> all-18 +5.4 -> 0.01; type change +5.4 (96 ms) -> 0.0; global bypass +6.6/+4.4 -> 0.0; Play after editing while stopped +8.6/+5.1 (50 ms) -> 0.0. Full table: `test\eq\overshoot_results.json` (script `test\eq\overshoot_test.py`).
  - The true-peak guard's interpolator is now 32 taps (it under-read fast transients by about 0.2 dB).
- **2. M8 (mud).** Severity now uses a score: the distance to the target plus half of how far the bump stands out from the song's own neighbouring bands (+/-1.25 octave).
  - The total cut follows the score. Three quarters go to a static bell at the detected centre; the dynamic band only takes the remainder (its depth is twice that remainder, at least 1 dB).
  - Bell sweep (`test\m8sweep\M8_SWEEP_V2.md`): removed share of the injected bell, before -> after, e.g. RuneCore +9@200 36 -> 61 %, +9@500 32 -> 63 %, WFTG +6@200 60 -> 99 %, +6@300 19 -> 41 %, +9@300 27 -> 53 %.
  - Known limit: RuneCore has a natural 4-5 dB dip at 240-370 Hz. A bell there fills the dip and ends up near the target, so +9@300 now flags (severity 26 -> 36) but only 22 % is removed (was 16 %), and +6@300 still reads as within tolerance.
  - The RuneCore and Wait for the Ground renders (Metal, Metal loud, Neutral, Rock, Rock loud) are bit-identical to 0.1.2. No new mud cut fires on the clean songs.
- **3. Silent master.** Loudness hold is off for a silent master (gain 0 dB, with a note). All gain math is finite-checked and capped. Reports are written as strict JSON: NaN/inf become `null`.
- **4. Memory.** The GUI no longer keeps each row's master in RAM. It is cached as `tmp\eqcache\master_<session>_<row>_<id>.npy` and only the selected row is loaded. Files are removed on exit, and stale ones (> 24 h) at startup.
  - Measured with the real GUI and worker on a 30-song batch of 90 s songs (`test\mem\mem_test.py`, results `test\mem\mem_*.json`). Growth of the process commit over the batch: +1033 MB before -> +65 MB after (2705 -> 1737 MB total). Working set after the batch: 1293 -> 328 MB. Peak working set: 2348 -> 1417 MB. Growth scales with song length, so full-length songs roughly double the "before" figure.
- **5. Lows.**
  - Loudness solver: up to 8 secant steps (was 3). A note appears when it does not converge. QA's E2 mono loud case is now -10.10 vs -10.07 LUFS (was -10.46).
  - CLI `--eq-preset`: the `out:` line and the report's `output.metrics` describe the post-EQ file. The auto-chain values are kept in `output.metrics_before_manual_eq`.
  - Undo: keyboard, wheel and click edits on sliders and knobs create undo steps. Changes less than 0.6 s apart are merged; a drag is one step.
  - The Export button stays disabled until the export has finished.
  - A UTF-8 BOM is accepted in presets, settings, profiles and sidecars.
  - Presets live in a per-user data folder outside the install folder. The default is `F:\Apps\Master-Craft-data\eq_presets` (a sibling of the app folder); set `MASTERCRAFT_DATA_DIR` to change it.
  - Sidecars store the SHA-1 of the master. On a different master (e.g. re-mastered), the saved EQ is not applied. An "Apply saved EQ" button offers it, and opening a song never overwrites it.
  - Preset booleans: `"false"`, `"0"`, `"off"`, `"no"` are false.
  - Macros are absolute settings, so applying one twice equals applying it once (no stacking).
- **6. Layout (closer to FL Parametric EQ 2).**
  - Narrower band strips (32 px) with icon type buttons (drawn curves, menu) and numbered on/off buttons.
  - A master **OUT** fader (+/-12 dB, with the guard's gain reduction) and a C1-C9 note row above the region bar.
  - The graph now takes 69 % of the tab width at a 1480 px window and 72 % at 1600 px (was 56 %). (Correction, v0.2.2: the real window minimum in 0.2.1 was about 1126x700 logical px, not the 1304 px stated here; the 1304 figure came from an offscreen run without system fonts.)

## 0.2.0 (Manual EQ)

- **Manual EQ tab.** This is a post-master 7-band parametric EQ in the style of FL Studio's Parametric EQ 2.
  - Graph: a log-frequency graph (20 Hz-20 kHz, +/-18 dB) with SUB / BASS / LOW MID / MID / HIGH MID / PRS / TREBLE labels.
  - Nodes: drag a node to set frequency and gain, use the mouse wheel to set Q, and double-click a node to reset it.
  - Per band: a gain slider, FREQ and BW knobs, a type (bell, low/high shelf, low/high cut) and an on/off switch.
  - Analyzer: a real-time spectrum behind the curve shows input (grey) and output (cyan). When playback is stopped, it shows the song average with the EQ applied.
- **Live audition.** QtMultimedia `QAudioSink` in pull mode plays the master through the EQ.
  - Edits are smoothed (10 ms, coefficients recomputed every 16 samples, wet/dry for bypass and type changes), so there are no clicks.
  - Measured control-to-audio latency is about 72 ms median and about 100 ms worst case (see `test\eq\`).
- **Editing.** Global bypass, A/B compare slots (with copy), undo/redo (Ctrl+Z / Ctrl+Y), and macros: Brightness / Air, Low-end weight, Vocal presence.
- **Presets and sidecar.** Presets are saved and loaded as JSON (`eq_presets\`). The per-song state is kept as `<master>.eq.json` next to the master. Nothing goes into `settings.json`.
- **Signal order.** Auto chain -> manual EQ -> loudness hold -> true-peak guard (2 ms lookahead, 4x interpolated peaks).
  - Loudness hold keeps the auto master's integrated loudness: an instant spectral estimate while you drag, then an exact value from an offline render about 0.5 s after you stop.
- **Export with EQ.** Writes `<master>_eq.<ext>` with the same processor the preview uses. The null test gives a max difference of 0.0 against the preview in float, and the LUFS difference is 0.000.
- **CLI.** `--eq-preset FILE` applies a saved preset after the auto chain.

## 0.1.2 (QA retest of 0.1.1: N1-N9, M8, L6, L8)

Fixes from `test\qa\v011\QA_RETEST_REPORT.md`. The RuneCore (Metal, Metal loud) and Wait for the Ground (Neutral, Rock, Rock loud) renders are bit-identical to 0.1.1. Regression tests: `test\test_regressions.py` (29 tests). M8 sweep: `test\m8sweep\M8_SWEEP.md`.

- **N9** An unreadable `settings.json` is never overwritten. It is copied to `settings.json.bad` (or `settings.json.<date-time>.bad` if that exists), the app runs on defaults in memory, and autosave is off until you save explicitly (Ctrl+S, the warning dialog's "Save current settings now", or Reset to defaults). The GUI shows a warning dialog and a status-bar banner.
- **M8** Mud detection scans 150-500 Hz. The long-term spectrum is compared with the profile target on a fine grid and averaged over 2/3 octave, so narrow natural peaks don't count, but a build-up anywhere in the range does. The cut follows the detected frequency and excess: a static bell (bounded, at most 6 dB) plus the dynamic band centred on the detected frequency. Clean songs get no new mud cuts. Known limit: a bell that fills a natural dip relative to the target (RuneCore +9 dB at 300 Hz) is close to the target and is not flagged.
- **N2** Make-up gain is capped (`modules.limiter.max_makeup_db`, default +18 dB). Near-silent or hiss-only inputs are no longer pushed up by 40-60 dB, and a note says when the cap was hit.
- **N7** When the MP3 encoder overshoots, the master is re-limited to a lower ceiling at the same loudness and re-encoded, instead of being turned down. The decoded MP3 must sit 0.1 dB under the ceiling. If the decoded loudness is more than 0.2 LU from the target, a note says so.
- **N4 / L6** Multichannel inputs are downmixed by layout. The WAVE_FORMAT_EXTENSIBLE speaker mask is used when present, otherwise the standard order for 3/4/5/6/7/8 channels. Centre and LFE go to both sides at -3 dB, surrounds go to their own side at -3 dB, and fronts are at unity. Unknown layouts are folded with equal-power gains and a note.
- **N1** A missing file in a batch is reported as `FAILED: File not found` and counts as a failure (exit 1 for a partial batch). If every input is missing, the names are listed (exit 2).
- **N3** Report names follow the unique output names. WAV keeps `<name>.report.json`; other formats use `<name>_<ext>.report.json`. `--no-overwrite` checks the report path too.
- **N5** For a file that exists but has no audio header, the error now says "unrecognised file header" instead of libsndfile's "does not exist". mpg123/libsndfile console noise is suppressed while loading.
- **N6** No `All-NaN slice` RuntimeWarning on hiss-only inputs.
- **N8** The punch WARNING names the stages that caused the loss, with advice for each, and needs at least 16 hits, so very short files no longer get it.
- **L8** Tab moves focus normally inside the settings panel and the queue (slider, combos, table) and in text fields. Elsewhere it is still the A/B toggle.
- **L10** Not changed. Denoise strength stays as documented; the QA retest marked it as passing.

## 0.1.1 (QA fixes on top of 519b6b1)

Fixes from `test\qa\QA_REPORT.md`. RuneCore (Metal, Metal loud) and Wait for the Ground (Neutral, Rock, Rock loud) renders are bit-identical to 519b6b1. Regression tests: `test\test_regressions.py`.

### High
- **H1** `loudness.true_peak()` no longer counts the resampling filter's ringing at the padded chunk edges. It now matches unchunked 4x resampling, so there are no more phantom overs: the QA case read -1.25 dBTP where the real value is -2.20. The limiter's TP trim, the final guard and the post-SRC check all use the corrected meter. The QA "strict loud" repro now reaches -9.54 LUFS (target -9.5); before it ended at -9.93. Any output more than 0.5 LU from its target gets a note saying why.
- **H2** The punch detector no longer crashes at 8 / 11.025 kHz. Bands that do not fit below Nyquist are skipped.

### Medium
- **M1** `settings.json` now has a `settings_version`. On load, values that still equal an old default migrate to the new one; values you changed are left alone. The new version is written on the next save, and saves are atomic. Version 2: limiter `lookahead_ms` 3 -> 2 and `release_ms` 100 -> 30. CLI `--settings` files are migrated in memory only.
- **M2** Batch output names are unique (`_2`, `_3` ..) in both the CLI and the GUI. New CLI flag `--no-overwrite` also avoids existing files.
- **M3** A corrupt, empty or non-audio input gives one readable line that keeps the libsndfile reason, and the batch continues. There is no ffmpeg command dump, and the ffmpeg decode temp file has a unique name.
- **M4** CLI exit codes: 0 all OK, 1 partial failure, 2 usage error, 3 all failed. `cli.bat` now passes the code on.
- **M5** With the limiter disabled, a note says the target may not be reached, and by how much the output was pulled back for the ceiling.
- **M6** MP3: output rates other than 32/44.1/48 kHz are resampled before the TP check, and the report shows the real rate. After encoding, the MP3 is decoded and its LUFS and TP are measured again. If the encoder pushed TP over the ceiling it is re-encoded quieter (up to 3 passes).
- **M7** Punch guard: each stage is charged only with its own change, including stereo, cleanup and the other non-back-off stages. When a back-off does not help, the guard tries the next culprit. If it ends over the limit, the notes carry a WARNING.
- **M8** Mud: the centre is now the power-weighted centroid of the bands within 2 dB of the worst one, so the QA case A reads 416 Hz instead of 500 Hz and the dynamic band (260-665 Hz) covers the 300 Hz build-up. The dynamic band is wider (/1.6..*1.6) and deeper when severe. At severity 70 and above a static share is added. QA case B goes 96.5 -> 72.0, where 519b6b1 left it at 86.4.
- **GUI previews** Preview files are named per process and run (`preview_<pid>_<id>_..`), so two instances never collide. They are deleted on exit (and at interpreter exit); stale ones older than 24 h are purged at startup.

### Low
- **L1** An unknown profile, or an unknown `--base` for `--calibrate`, is now a usage error (exit 2) that lists the known profiles, not a traceback.
- **L2** `--spotify-strict` without loud mode prints a warning, and the help text and README are clarified. In the GUI the checkbox is disabled in streaming mode.
- **L3** The CLI warns about out-of-range intensity, ceiling and target. The GUI no longer writes clamped spin-box values back over stored values it could not display.
- **L4** Tiny files no longer crash. Under 1024 samples the file is passed through with a note. Hits at the edges (NaN rows) are dropped before the medians.
- **L5** Silent input (below -70 LUFS) skips the chain and gets a note.
- **L6** Inputs with more than 2 channels are folded to stereo (odd channels to L, even to R) with a warning, instead of being truncated.
- **L7** The "not reachable" and loud-cap notes quote the GR cap that was actually used.
- **L8** Tab moves focus while you are typing in a field and still toggles A/B elsewhere. Play resets at end of media. The suffix is trimmed. Invalid JSON in an Advanced field keeps the previous value and is outlined in red. A/B switching continues from the position of the player you were hearing.
- **L9** A truncated WAV (data chunk shorter than the header says) gets a warning.
- **L10** A note explains when the hiss score rises because of the loudness gain.
- **L11** README: per-profile streaming LUFS, the MP3 encoder settings (CBR, about 320 kbps), exit codes, the settings migration, known gaps.