# Historical Python implementation guide

This is the preserved guide for the older, separate Python implementation. The current native Rust release is documented in the project README. Feature, path and dependency statements below describe that historical implementation, not the Windows native installer.

Master-Craft is a Windows desktop app that analyses and **auto-masters Suno AI songs**, with light mix-repair on top.
It runs locally in its own `.venv`, writes all temp files inside the app folder, and needs no cloud service.

## Features
- **Analysis engine.** Each issue gets a 0–100 severity, a plain-language explanation and a suggested fix:
  - muddiness (150–500 Hz, wherever the build-up sits), harshness (2–5 kHz), sibilance (5–10 kHz bursts), fizz/sizzle (8–16 kHz)
  - recurring metallic resonances, Suno-style HF shimmer and codec artifacts (lowpass cutoff, spectral holes, pre-echo, HF swirl)
  - broadband noise/hiss, clipping (flat-top runs and histogram spike), inter-sample true-peak overs
  - stereo and phase problems (correlation, side/mid ratio, per-band correlation, mono-fold loss)
  - weak, boomy or missing low end
  - loudness metrics: integrated LUFS, LRA, PLR, short-term max, crest factor
  - spectral balance against the selected genre's target curve, plus a "nearest genre" suggestion
  - The detector thresholds come from `docs/research.md`.
- **Analysis-driven chain.** Each stage runs only when the analysis calls for it, and every move is scaled and capped:
  1. headroom
  2. declip
  3. DC/rumble cleanup (zero-phase 24 dB/oct HPF, so the kick fundamental keeps its phase)
  4. HF-only denoise (only if real noise is measured in true gaps of the song; breaths are not hiss)
  5. corrective static EQ: **tilt first**, then the residual (see below) plus resonance notches (boosts ≤ 3 dB, cuts ≤ 4 dB)
  6. dynamic EQ (mud, harsh, fizz), with drum hits protected
  7. split-band de-esser, only for narrowband 5–10 kHz bursts that are not drum hits
  8. 3-band multiband compressor (zero-phase complementary FIR crossovers that sum back exactly). It is level-preserving, so it evens out dynamics without acting as an EQ. Attack/release follow the measured crest factor and are never shorter than the profile asks
  9. glue compressor
  10. 2× oversampled parallel saturation
  11. **low-end punch** (new): a kick-band (50–160 Hz) transient shaper (+attack, −sustain) plus a kick-triggered sub duck (22–60 Hz, 90 ms). This is the only stage that *adds* attack; every other stage can only take it away
  12. M/S stereo with mono-safe bass and swirl control
  13. optional **4× oversampled soft clipper** (loud mode, 1–1.5 dB on Metal/Rock/Neutral), which shaves the shortest peaks before the limiter
  14. **4× oversampled true-peak limiter**: 2 ms lookahead, two-stage program-dependent release (30 ms fast / 250 ms slow, the lower gain wins). It iterates to the target LUFS and never exceeds the ceiling
- **Corrective EQ in detail.** The deviation is fitted with one straight line over 125 Hz–8 kHz. Above 0.75 dB/oct the slope is corrected with a low shelf (200 Hz) and a high shelf (3 kHz) *before* any bell, and only the residual goes to the octave bells (research2 §1.4). A correction that would swing more than 6 dB peak-to-peak is scaled back and reported. Every filter carries a reason (`"why"` in the report JSON and in the report tab), and the report shows the predicted low-mid response.
- **The low end.** A bass deficit is fixed with a bell on the kick body (105 Hz), never a broad shelf that lifts the sub with it; bass excess is cut at 78/110 Hz. Punch-preserving profiles also scale the static mud cut (`punch.mud_static_scale`) and keep its centre above 200 Hz (`punch.mud_min_hz`). A **kick-body guard** then measures the planned EQ's response at 100/125/160 Hz and scales the low cuts back if they would take more than 1.8/2.0/2.6 dB out of them; the guard action is reported.
- **Punch protection.** Kick and snare hits are detected on the input (kick band 40–120 Hz; snare body 180–350 Hz plus 1.5–5 kHz crack). Dynamic EQ, de-esser, saturation and denoise leave the first ~40 ms of every hit alone. Compressors use slower attacks and program-dependent release.
  - After rendering, a **punch check** compares each hit's transient-to-sustain ratio (peak 0–15 ms vs RMS 30–120 ms) with the input, across the kick, snare, crack (5–12 kHz) and full-band views.
  - If the worst 10 % of hits lose more than 2 dB, or more than 15 % of hits are smeared, a probe pass finds the stage responsible and backs it off. The result is kept only if it measures better. At most 2 rounds.
  - The report shows a "Punch / transients" table, the time ranges with smeared hits, and any guard actions. The CLI prints a one-line summary.
  - In loud mode, punch wins over loudness. Metal and Rock stop at 4 dB of limiter gain reduction, Neutral at 5 dB, and the notes say so.
- **Safety re-analysis.** After mastering, the output is analysed again. If distortion or artifact scores got worse, the chain is re-run at 60% intensity, up to 2 retries. When backing off cannot help (for example, tonal shifts caused by heavy limiting in loud mode), the app says so in the notes.
- **Two modes.** *Streaming-safe* targets -14 LUFS / -1 dBTP (Acoustic/Folk -15, Orchestral/Cinematic -16 LUFS). *Loud* uses per-genre targets, with an optional Spotify-strict -2 dBTP ceiling (loud mode only; it does not switch loud mode on). If a target cannot be reached without more than 6 dB of limiting (less on some profiles in loud mode), the app stops early and reports it. Any output that ends more than 0.5 LU from its target gets a note saying so and why.
- **Genre profiles (16):**
  - Neutral, Metal, Rock, Alt Rock, Metalcore/Djent, Pop, Pop-Punk
  - Hip-Hop/Trap, EDM, Synthwave/Retro, Acoustic/Folk
  - Country, R&B/Soul, Lo-fi, Game/Instrumental Score, Orchestral/Cinematic

  You can save and load custom JSON profiles in `profiles/`, and use `--calibrate` to build a profile from reference tracks.
- **Blends / combos.** Any two profiles can be mixed by weight, which is how most real songs sit between genres:
  - CLI: `-p "metal:0.6+edm:0.4"` (synth metal), `-p "lofi:0.5+game_ost:0.5"` (retro instrumental), `-p "rock:0.6+pop:0.4"` (alt rock). Weights normalise; short names work (`metal+edm`, `game`, `retro`, `djent`, `trap`, `indie`…).
  - GUI: **Blend…** next to the profile list — pick two profiles, set the weight, see the resulting loudness/target, and save it as a preset (it then appears in the list like any custom profile).
  - The blend mixes the tonal curve, loudness targets, glue/multiband timing, stereo window, saturation and clipper drive; the punch caps take the most conservative value of the two halves.
  - The analysis also suggests a blend for the song (`blend_suggestion` in the report), not just the nearest single genre.
- **GUI (PySide6):**
  - drag & drop and a batch queue with a worker thread, per-file progress and cancel
  - loudness-matched **A/B** playback (Space = play/pause, Tab = A/B outside the settings panel and the queue, Ctrl+O = add files, Ctrl+S = save settings)
  - HTML report tab
  - spectrum (before/after against the target curve) and waveform views drawn with QPainter
  - auto-generated **Advanced** settings panel
  - settings persist to `settings.json`
- **Export (several formats at once):**
  - WAV 24-bit (master), WAV 16-bit (CD, TPDF dither), WAV 32-bit float (editing)
  - FLAC 24-bit (lossless archive), FLAC 16-bit
  - **MP3** through libsndfile (LAME), no external encoder: CBR, `compression_level` 0 (about 320 kbps). MP3 only supports 32/44.1/48 kHz, so other output rates are written at 48 kHz (44.1 kHz below that) and the report says so. After encoding, the MP3 is decoded and measured again (`export.decoded_lufs`, `export.decoded_tp_dbtp`); if the encoder pushed the true peak over the ceiling it is re-encoded slightly quieter (up to 3 passes).
  - **Tick one or more formats** in the GUI (Export → "Formats") or pass a comma list in the CLI (`-f wav24,mp3,flac16`). Every format is rendered from the same master in one run, each with its own JSON report.
  - **File names are unambiguous**: the primary format of an extension keeps the plain name and the variants carry a tag, e.g. `song_master.wav`, `song_master_16bit.wav`, `song_master_32f.wav`, `song_master.flac`, `song_master_16bit.flac`, `song_master.mp3`. A live preview under the format list shows exactly which files a run will write, and the queue row shows "mastered · 3 files" with all paths in the tooltip.
  - optional output sample-rate conversion (applied once for every format); true peak is re-checked after conversion
- **CLI** with the same engine, plus a JSON report per written file.

## Manual EQ (post-master)
After mastering, open the **Manual EQ** tab. It is a 7-band parametric EQ on the finished master, styled after FL Studio's Parametric EQ 2.
- **Nodes.** Drag a node for frequency and gain, use the mouse wheel for Q (bandwidth), and double-click to reset the band. Each band also has a gain slider (double-click resets it to 0), FREQ and BW knobs, a type (Bell, Lo/Hi shelf, Lo/Hi cut, 12 dB/oct) and an on/off switch.
- **Play** plays the master through the EQ live. Changes are heard after about 50-100 ms, without clicks. Every change is a 20 ms crossfade between the old and the new EQ, so a switch is never louder than the louder of the two settings. The analyzer behind the curve shows the input (grey) and output (cyan) spectrum.
- **Bypass** lets you hear the auto master alone. **A / B** are two EQ slots to compare (`A>B` copies). Undo/Redo is Ctrl+Z / Ctrl+Y. **Macros** has Brightness / Air (+2.5 dB high shelf at 6 kHz), Low-end weight and Vocal presence.
- **Hold loudness** (on by default) keeps the integrated loudness of the auto master, so a boost is not just "louder". The **OUT** fader (+/-12 dB) trims the result. A true-peak guard after the EQ keeps the ceiling. The order is: auto chain -> manual EQ -> loudness hold -> OUT -> TP guard. If the hold cannot reach the target (heavy boosts against the guard), the info line says so.
- **Export with EQ** writes `<master>_eq<tag>.<ext>` using the same processor as the preview, so the file is what you heard. It writes every ticked export format (the button says how many files, and the tooltip lists them).
- **Presets** are JSON files (Save... / Load...). They live in a per-user data folder outside the install folder: `F:\Apps\Master-Craft-data\eq_presets` by default (a sibling of the app folder), or `%MASTERCRAFT_DATA_DIR%\eq_presets`. If that folder can't be written, the EQ tab tells you and falls back to `<app>\user_data\eq_presets`; `%APPDATA%\Master-Craft` (system drive) is only used if you agree.
- **Per-song EQ.** It is saved automatically as `<master>.eq.json` next to the master, together with the master's SHA-1, and restored when you open that master again. If the master file changed (re-mastered), the saved EQ is not applied; **Apply saved EQ** loads it on request. It never goes into `settings.json`.
- The master the EQ works on is cached on disk (`tmp\eqcache\`); only the selected song is in memory.
- **CLI:** `cli.bat song.wav --eq-preset my_eq.json` applies a preset after the auto chain. The `out:` line and the report then show the post-EQ file.

## Reference matching (match another track)
`--reference file.wav` (CLI) or the **Reference track** row (GUI) makes the master sit on another
track's tonal balance; `--match-loudness` also takes its loudness, `--no-match-width` keeps the profile's
stereo width. The profile still supplies dynamics, punch caps, stereo and clipper — only the tonal target
moves.

How it works (`mastercraft/reference.py`, own implementation; no GPL/AGPL code is used):
- analyses the **loudest 50 % of active frames** of both files (a chorus, not a whole song), 1/6-octave
  smoothing on a 1/12-octave grid from 25 Hz to 16 kHz, level-aligned over 100 Hz–10 kHz so the match is
  loudness-neutral;
- the correction is smoothed to ~1/3 octave, clamped (±6 dB, `modules.reference_match.max_db`), zero below
  30 Hz and above the codec cutoff, and realised as a **chain of up to 7 broad minimum-phase bells**
  (Q ≤ 1.4). A matching FIR was tried first: it matched the curve slightly better but cost 1.7 dB of
  measured punch, so it is kept only as an option (`realize: "fir"`);
- the report carries the band distance to the reference **before and after**, and the punch guard may dial
  the match back (`reference_match` is a back-off candidate) when it starts smearing hits;
- matching a song to *itself* is a null test: max change 0.000 dB.

Measured on RuneCore (synth-metal) against Wait for the Ground as reference: band distance 4.8 → 0.8 dB
RMS, punch score 1.3 (limit 2.0), loudness and true peak still on target. Docs: `docs/improvement_plan.md`
§5, including what each of the five reference projects contributed (and their licences).

## Install
Requirements: Windows 10/11 and Python 3.12 or 3.13 (64-bit).

```bat
cd F:\Apps\Master-Craft
setup.bat
```
`setup.bat` creates `.venv` in the app folder and installs `requirements.txt` into it: numpy, scipy, soundfile, librosa, pyloudnorm, noisereduce and PySide6, plus their dependencies. If `D:\Caches\pip` exists, it is used as the pip cache.

## Launch
- GUI: double-click `run.bat`, or run `F:\Apps\Master-Craft\run.bat [files...]`
- CLI: `F:\Apps\Master-Craft\cli.bat song.wav -p pop`
- Direct: `F:\Apps\Master-Craft\.venv\Scripts\python.exe -m mastercraft` (GUI) or `... -m mastercraft.cli ...`

Both .bat files point TEMP/TMP and the numba/matplotlib caches to `<app>\tmp`.

## CLI
```
cli.bat <files|folders|globs> [options]
  - -p, --profile NAME       one of 16 built-in profiles, a custom profile, or a blend
                           (`-p "metal:0.6+edm:0.4"`) — see --list-profiles
  -m, --mode               streaming_safe | loud
      --spotify-strict     with -m loud: -2 dBTP ceiling (ignored with a warning otherwise)
  -l, --lufs X             override target LUFS          -c, --ceiling X   override dBTP ceiling
  -i, --intensity X        0..1.5 scale of all corrective moves (default 1.0)
  -f, --format LIST        one or more, comma separated: wav24 (default) | wav16 | wav32f |
                           flac24 | flac16 | mp3        e.g. -f wav24,mp3,flac16
      --sr N               output sample rate (0 = keep)
  -o, --out DIR            output folder (default <app>\output)   --suffix S (default _master)
      --analyze-only       just print the analysis           --report-json PATH
      --no-report          no JSON report beside each file
      --reference FILE     match the tone of a reference track (see Reference matching)
      --match-strength X   0..1.5 scale of that match (default 1.0)
      --match-loudness     take the loudness target from the reference too
      --no-match-width     keep the profile's stereo width instead of the reference's
      --disable a,b        switch off modules, e.g. denoise,saturation,stereo
      --settings FILE      use another settings JSON         --no-safety   skip re-analysis
      --calibrate refs... --name NAME [--base neutral]   build a custom profile from reference masters
      --no-overwrite       never replace existing outputs (adds _2, _3 ..)
      --debug              full tracebacks for failures
      --quiet
```
Batch behaviour:
- A file that cannot be read (corrupt, empty, not audio) prints one `FAILED: ...` line and the batch carries on.
- Output names are unique within a batch: two inputs called `song.wav` from different folders give `song_master.wav` and `song_master_2.wav`. A master from an earlier run is replaced unless you pass `--no-overwrite`. The variants of one format set never collide (`song_master.wav`, `song_master_16bit.wav`, `song_master.mp3`, ...), and with several formats each file gets its own report.
- Out-of-range values (`-i` outside 0..1.5, `-c` above 0, `-l` outside -30..-5) print a warning; intensity and ceiling are then clamped.

Exit codes (`cli.bat` passes them on):

| code | meaning |
|---|---|
| 0 | every file OK |
| 1 | some files failed; the others were written |
| 2 | usage error: bad option, unknown profile, no input files, unreadable `--settings` file, MP3 not available |
| 3 | every file failed |
Examples:
```
cli.bat "D:\Suno\*.wav" -p edm -m loud -f mp3
cli.bat song.wav -f wav24,mp3,flac16 -o D:\Masters
cli.bat song.wav --reference "D:\Refs\favourite_master.wav" -p metal -m loud
cli.bat song.wav --analyze-only --report-json song_report.json
cli.bat ref1.wav ref2.wav --calibrate --name MyRock --base rock
```

## Settings
Settings are in `settings.json` in the app folder (or the file named by the `MASTERCRAFT_SETTINGS` environment variable). The GUI writes it, and you can also edit it by hand. Anything you leave unset falls back to the built-in default.

The GUI saves the whole settings dict, so built-in defaults end up stored in the file. The file therefore carries a `settings_version`. When a later version retunes a default, loading migrates every stored value that still equals the *old* default to the new one, and leaves values you changed alone. The new version number is written the next time the app saves. Migrations so far:
- version 2 (transient fix): `modules.limiter.lookahead_ms` 3.0 -> 2.0 and `modules.limiter.release_ms` 100.0 -> 30.0.

`--settings FILE` files go through the same migration (in memory only; the CLI never writes them).

If `settings.json` can't be read (for example a typo from a hand edit), the app keeps the file. It copies it to `settings.json.bad` (or a timestamped `.bad` name), runs on the defaults and does not autosave. Fix the file and restart, or save explicitly (Ctrl+S) to replace it.
- `general`: profile, mode, spotify_strict, target_lufs/ceiling_dbtp overrides, intensity, format (legacy single), formats (list of one or more), out_dir, reference (path), suffix, dither, write_report.
- `modules.<name>`: one block per chain stage:
  - headroom, declip, cleanup, denoise, static_eq, resonance, low_end, low_punch
  - dynamic_eq, deesser, hf_rebuild (off by default), multiband, glue, saturation, stereo, limiter

  Each block has `enabled`, an `amount` scale and its own parameters. A value of `null` means "use the profile or analysis value".
  - `low_punch`: `attack_db` (2.0), `sustain_db` (1.5), `max_attack_db` (3.0), kick band `band_lo_hz`/`band_hi_hz` (50/160), `sub_duck_db` (2.0), `max_duck_db` (2.5), sub band `sub_lo_hz`/`sub_hi_hz` (22/60), `duck_release_ms` (90). Set `amount` to 0 or disable the module to hear the chain without it.
  - `static_eq`: `tolerance_db` (2), `fraction` (0.6), `max_db` (4), `tilt` (true), `tilt_min_slope_db_oct` (0.75), `tilt_low_hz` (200), `tilt_high_hz` (3000), `tilt_max_db` (3.5).
  - `reference_match`: `enabled`, `strength` (1.0), `max_db` (6), `realize` ("biquads" or "fir"), `match_width` (true), `match_loudness` (false).
  - `multiband`: `smart_timing` (true) derives attack/release from the measured crest factor (never shorter than the profile value).
- `safety`: enabled, max_retries (2), backoff (0.6), worse_by (15 severity points; an increase only counts once the output reaches severity 30), punch_check (true), punch_max_loss_db (2.0), punch_max_smeared_pct (15).
- `transients`: protect (true), post_ms (40): hit protection for the dynamic stages.
- `clipper`: enabled, drive_db (null = profile `clipper_db` for the mode).
- `modules.limiter`: lookahead_ms (2), release_ms (30, fast stage), release_slow_ms (250), max_makeup_db (18: the most gain added to reach the target; quieter inputs stop short with a note).

Reports: `<output>.report.json` for WAV, `<output>_<ext>.report.json` for other formats, so a WAV and an MP3 master of one song keep separate reports. Inputs with more than 2 channels are downmixed by speaker layout (centre/LFE -3 dB to both sides, surrounds -3 dB to their side).

## Profiles
Built-in profiles live in `mastercraft/data/builtin_profiles.json` and come from the research brief §1/§2.4. Each one defines:
- a 10-band octave tonal target
- multiband and glue settings
- a stereo correlation window, side gain and side HPF
- saturation
- LUFS for both modes, the TP ceiling and a limiter GR guideline
- `punch` caps (Metal, Rock, Neutral, Alt Rock, Metalcore/Djent, Pop-Punk, Synthwave, Game): max low-mid and low-shelf boost, harsh, fizz and de-esser depth, minimum dynamic-EQ attack, mud scale, `mud_static_scale` and `mud_min_hz` (how much of the static mud cut the profile allows and how high it must sit), `low_punch_attack_max_db` / `low_punch_duck_max_db`, and the loud-mode limiter GR cap
- `clipper_db` per mode (0 in streaming-safe)

Custom profiles are saved in `profiles/*.json` as `{ "base": "<builtin>", "settings": {...overrides...} }`.
A blend is saved the same way with `{ "display_name": ..., "blend": "metal:0.6+edm:0.4" }`; it resolves to a
weighted profile every time it is loaded, so editing the parents changes the blend too.

## Project layout
```
mastercraft/        analysis.py, engine.py, punch.py, loudness.py, audio_io.py, postprocess.py, profiles.py, settings.py, cli.py
  dsp/              filters.py, dynamics.py, limiter.py, repair.py, tone.py, lowend.py (low-end punch)
  gui/              main_window.py, widgets.py, worker.py, player.py, settings_panel.py, report_html.py, app.py
  data/             builtin_profiles.json (16 profiles)
docs/research.md    research brief that the detectors, profiles and plan are based on
docs/research2_eq_automastering.md, docs/transient_fix_quicknotes.md, docs/ref/mc_reference_dsp.py
                    researcher notes on EQ, auto-mastering and transient-safe limiting (verified, partly adopted)
docs/improvement_plan.md   measured review of the low end / auto EQ + the prioritised next steps
test/               make_test_signal.py (synthetic "problem song"), smoke_gui.py,
                    diagnose_punch.py (stage-by-stage and bypass punch/low-end diagnosis),
                    diag_lows.py (before/after punch, per-band crest, every move below 300 Hz),
                    sweep_lowpunch.py (attack/sustain/release sweep on real audio),
                    test_lowpunch_synthetic.py (controlled kick train for the shaper),
                    ref_compare.py (raw vs reference master vs Master-Craft renders), tonal_probe.py,
                    eq\eq_feature_test.py (EQ screenshots, null/click/latency), eq\overshoot_test.py (EQ transitions),
                    mem\mem_test.py (GUI batch memory), m8sweep\ (mud bell sweep)
profiles/  output/  tmp/   created at runtime
```

## Tests
```
.venv\Scripts\python.exe test\make_test_signal.py
cli.bat test\test_song.wav -p neutral -o test\out
set QT_QPA_PLATFORM=offscreen && .venv\Scripts\python.exe test\smoke_gui.py --full
.venv\Scripts\python.exe test\diagnose_punch.py song.wav --profile metal --settings test\test_settings_new.json --out test\diag
.venv\Scripts\python.exe test\diag_lows.py song.wav --profile "metal:0.6+edm:0.4" --mode loud --start 40 --dur 45
.venv\Scripts\python.exe test\test_lowpunch_synthetic.py
.venv\Scripts\python.exe test\sweep_lowpunch.py song.wav --start 40 --dur 25
.venv\Scripts\python.exe test\ref_compare.py --raw song.wav --ref "MIXEA=song_mixea.wav" --mc "MC=test\song_master.wav" --out test\compare
```
Test renders use a separate settings file (`test\test_settings_new.json`), never the user's `settings.json`.

Regression tests for the QA fixes (outputs in `test\out_reg\`): `.venv\Scripts\python.exe test\test_regressions.py`. It prints PASS/FAIL per test and exits non-zero on any failure.

## Known gaps
- Detector thresholds and severity curves are **starting values** from the research brief. They have not been tuned on a labelled set of real Suno songs. Sibilance and the PLR→severity mapping are my own heuristics. Work through §6 of `docs/research.md` (collect references, then `--calibrate`, then adjust) to tune them.
- No stem mode or source separation, by design (no demucs/spleeter). Repairs are broadband or M/S only.
- Pre-echo and spectral-hole detection only report problems; nothing repairs them. HF rebuild exists but is off by default.
- Loud targets (-8 to -9 LUFS) are often out of reach on dense Suno renders without more than 6 dB of limiting (4 dB on Metal/Rock). The app stops early and tells you; it does not crush the track.
- The punch metrics are measured, not listened to. They were tuned on two songs (one synth-metal, one alt-rock) against MIXEA masters. The multiband crossovers are still linear-phase FIR (they must sum back exactly); no pre-ring problem showed up in the bypass tests, but it has not been checked on other material.
- MP3 encoding goes through libsndfile's built-in LAME, CBR at `compression_level` 0 (about 320 kbps). Bitrate and quality are not yet exposed as settings. Encoder overshoot is fixed by re-limiting (loudness held within about 0.1 LU); the decoded file is kept 0.1 dB under the ceiling.
- Mud detection compares against the profile target and the song's neighbouring bands. A build-up that only fills a natural dip of the song (it ends up near the target and is not a bump) is flagged weakly or not at all, and only part of it is cut (RuneCore +9 dB at 300 Hz: 22 %).
- The mud cut is deliberately smaller than the measurement asks for on punch-preserving profiles (`mud_static_scale`, the kick-body guard). A master can therefore measure *slightly muddier* than its input while sounding punchier; the note says so, and the trade-off is exposed as settings.
- The low-end punch is a transient shaper and a kick-triggered sub duck, not a source separator: on a mix where the bass masks the kick completely, it tightens the sum but cannot unmask the kick. Its caps (3 dB attack / 2.5 dB duck by default) are chosen to stay inaudible as an effect.
- The tilt-first correction fits one line over 125 Hz-8 kHz, so a mix with an S-shaped error still gets bells on top of the tilt; the correction is not yet reduced to a least-squares set of 3-6 filters (research2 §3.1 step 8).
- Blends are weighted averages of two parametric profiles; there is no per-band blend UI, no tunnel targets (P25/P75) and no reference-track matching yet (see `docs/improvement_plan.md` for the planned order).
- Hit detection keeps onsets at least 90 ms apart, so very fast double kick (16ths above about 167 BPM) is only half sampled by the punch metrics. The sustain window in the punch metric now ends before the next hit, which is why absolute TSR values differ from earlier reports.
- The `transients` settings are not in the GUI yet (edit `settings.json`). The spectrum view draws the target curve of the profile currently selected, not the one a result was made with.
- Denoise is gentle by design; on low-level hiss the output noise score can read higher than the input because mastering raises the level.
- Decoding m4a/aac/wma needs the user's own `ffmpeg` on PATH. WAV, FLAC, MP3 and OGG work without it.
- The whole file is processed in memory (float64). A 4-minute stereo 48 kHz song takes about 55 s to master on the dev PC. Very long files (more than 15 minutes) need several GB of RAM.
- Manual EQ: the audible latency also includes the Windows audio device's own buffer (not measurable without a loopback recording). The preview plays at the master's sample rate; if the device refuses that rate the sink may fail to start. The EQ transition bound (never louder than the louder setting) is measured numerically, not by listening.
- **Project licence not chosen yet.** See `THIRD_PARTY_NOTICES.md` for the dependency licences and LGPL obligations.
