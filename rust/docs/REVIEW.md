# Source assessment and verification — November 12, 2026

Original application assessment: **7/10 for code/product readiness**, based on the working source, observed UI, and timed processing. This is not a subjective listening score.

The Python tool has a strong practical feature set: genre profiles, reference matching, transient protection, safety retries, multiple export formats, decoded-file peak checks, A/B playback, and manual EQ. It is a useful foundation rather than a throwaway prototype.

The main weaknesses are repeated full-track filtering and analysis, large float64 intermediates, a 995-line engine coordinating planning and execution, and heuristic detectors/genre curves whose confidence is difficult for beginners to judge. The GUI exposes many controls with uneven hierarchy. The existing tree already contained substantial uncommitted changes; the rewrite was added alongside it without replacing those changes.

One concrete bug was verified in the low-bass ducking hold: the Python implementation converts its millisecond hold to control-rate blocks, then divides by the block size again. Rust converts once. The regression checks attenuation 20–50 ms after a hit, rather than simply mirroring the code.

## Benchmark method

The included `test/test_song.wav` is 48 seconds, 48000 Hz, stereo. Both tools used their default Neutral mastering configuration, with safety enabled. Each measured processing three times after decoding. Python timings exclude imports and loading; Rust `--benchmark 3` also excludes decoding, startup, and exporting. Medians reduce the effect of one atypical run. No other applications were stopped to produce favorable numbers.

Python runs: 12.5610, 12.1611, 12.3775 s; median 12.3775 s. The 0.3.1 Rust comparison measured 1.8057, 1.8351, 2.1261 s; median 1.8351 s, **6.74× faster on this fixture**. Version 0.4.0 changes interface code and artwork without changing the measured DSP; the benchmark retains its measured 0.3.1 identity. The release package includes a small JSON summary of the verification run.

This is a whole-pipeline comparison. Rust uses revised detectors, a bounded spectrum front end, native EBU meters, and different safety/transient rules. The fixture's hiss detector also triggers differently. The ratio therefore does not isolate language speed, establish identical sound, or predict every song's performance.

The tested default Rust output measured −14.0966 LUFS, −1.1510 dBTP, zero full-scale clipped samples, and unchanged frame count. The independent FFmpeg export check, measured on 0.3.2, returned approximately −14.1 LUFS and −1.1 dBTP across six formats after 44100 Hz conversion. These are identified as earlier measurements, not newly timed 0.4.0 results.

## Functional checks

The 0.4.1 73-test release suite (41 unit, 24 audio, two EQ-analysis and six real-time EQ regressions) covers:

- EBU sine/channel-energy calibration, multiple sample rates, intersample peaks, silence, short files, invalid/nonfinite data, and cancellation.
- Filter response, FFT-convolution alignment, complementary reconstruction, limiter gain bounds, makeup limits, and final ceiling.
- Duration and overwrite preservation, validated settings/preset import, profile blends, and the ducking timing regression.
- Separate Master/EQ preparation versus explicit Export, without writing audio during preview work.
- Deterministic suggested EQ, bounded/profile-guided moves, and unavailable high bands.
- No-suggestion states for silence, short excerpts, invalid data and missing bands; mean-bin versus octave power units; spectral-estimate comparisons with measured broad EQ deltas on steady signals.
- Real-time parameter messages changing audible samples without rendering a file; fixed live source/export sample agreement; smooth gain ramps; rapid coefficient changes staying finite and peak-safe.
- Actual analyzer samples before listening volume, paused seeking, A/B transitions, and rejection of stale renders after queue rows shift.
- Generated-frame alpha/opening, bounded UV samples, crown aspect/clearance, and actual visible material connections at every corner at three sizes and three pixel densities. A deliberately displaced side must fail the connectivity check.
- Native pointer/keyboard and disabled-state behavior beneath custom button/slider paint; numeric precision, suffixes, and actual slider-handle alignment.
- Stable settings width over 64 frames with loaded previews, long paths, precise values, expanded controls and 125% scaling; Export disclosure state and capture-only scrolling without changing settings.
- Whole-control transport wrapping and dedicated waveform label/trace/time bands at compact and default heights.
- Longer reconstructed peaks, a numerical excerpt of the actual Cipher failure, analytic high-frequency signals, file-boundary tails and cross-rate final protection.
- Combined automatic EQ response limits, reduced/zero intensity after capping, shared static/dynamic cut budgets and valid reference bands.
- Compensation status based on the final rendered level, including output gain and final peak protection.

The external export harness checks WAV 16/24/float, FLAC 16/24 and MP3 320 kbps, independent decoded loudness/true peak, 44100 Hz resampling, input preservation, output collisions, mixed/all-input failure exit codes, and invalid targets. `cargo clippy --all-targets -- -D warnings` and formatting/build checks are also required.

The research report informed visible changes to profile/loudness wording, per-band suggestion explanations, compensation semantics and graph labels. It recommended no speculative DSP retune. Full EBU test-set conformance, dataset-derived genre ranges, album workflows and broad listening evaluation remain separate work.

## Visual review and limits

Artwork was independently inspected and revised. The detailed portrait and small companion icon each passed 9/10 for look, polish, and readability at their stated sizes on obsidian. Early UI/art revisions failed their gates and were corrected; scores were not raised merely to meet the target.

Earlier 0.3.2/0.3.3 review scores did not establish acceptance: the user rejected the procedural appearance, then identified visible side/corner offsets in the generated frame. The earlier review missed those offsets. The source corner and edge slices used different scales, and rectangle-only checks could not detect disconnected visible alpha. Version 0.4.0 supersedes that integration with shared silver/amethyst chrome and true matching corner/edge source grids. Only the uniformly scaled hanging crown remains from the earlier slime image.

The user's new concept is the current visual target: sculpted silver frames, amethyst crests, teal hanging slime, restrained purple ribbons, display typography, faceted buttons and diamond handles. Built-in ImageGen produced the detailed supporting artwork. All controls, text, measurements, graphs and interactions remain native and editable; this is an implemented skin rather than a flattened concept image.

A separate reviewer inspected the actual generated artwork and native-renderer captures, including enlarged corner crops. Initial revisions exposed oversized welcome cards, growing settings widths, panel/scroll clipping and compact waveform label overlap. Fixes measure native text/value widths, use allocated panel bounds, keep solid scrollbars inside their viewport, wrap controls, and reserve separate label/trace/tick bands. Subsequent independent review passed the following states:

| Reviewed item | Look | Polish | Readability | Evidence boundary |
| --- | --- | --- | --- | --- |
| Portrait / companion icon | 9/10 | 9/10 | 9/10 | 190 px portrait and 48/64 px icon roles on obsidian |
| Current integrated artwork and welcome | 9/10 | 9/10 | 9/10 | Default and minimum clients; connected outer/panel/footer frames, legible hierarchy, preserved character identity |
| Overview, report, expanded Export settings | 9/10 | 9/10 | 9/10 | Default client, real fixture and completed master; readable data, scrolling and pinned playback |
| Manual EQ | 9/10 | 9/10 | 9/10 | Default and minimum clients, including the scrolled compact controls; layered graph, separated waveform labels, complete transport and reachable EQ fields |

The default client is 1480×940 logical pixels (1850×1175 framebuffer); minimum is 1100×720 (1375×900 framebuffer), at 125% scaling. At minimum size, editing content scrolls while playback and Master/Export remain accessible. Format/folder controls are under Export settings. The final small footer layout change reserves its complete left status slot; the final idle captures were inspected again after rebuilding.

No blocking visual defects remained in those displayed states. Scores do not cover every resolution, hover-tooltip appearance, accessibility conformance, busy-footer progress state or subjective audibility. Synthetic layout/input checks complement the captures but do not establish all device behavior. Corner alpha connectivity additionally passes at 1100×720, 1480×940 and 1920×1080, at 1×/1.25×/2× density, and rejects a deliberate 12-logical-point side offset.

Original references and accepted generated PNG bytes are unchanged; the app samples texture regions. Exact generation prompts, reference roles, alpha observations and slice layout are included with the assets. The six-format check remains the earlier 0.3.2 measurement; 0.4.0 receives a fresh packaged CLI WAV export and source/package integrity check.

Native captures show the branded implementation, not a mockup. The user also exercised real tracks, playback, settings, and export in the live application. Direct agent interaction was intermittently blocked by the computer-use user-input guard while the user was operating the preview; the agent preserved those sessions. Final review captures use egui's native renderer screenshot callback after jobs and layout settle, without manipulating the user's editing windows. They are included with the release.

No subjective listening verdict or universal audio-device latency guarantee is claimed. Live behavior on additional output devices, sustained extreme-rate/high-Q workloads, broad musical acceptance, and the future web/iPhone version remain separate work. The original application remains available for advanced features listed in the README that were not reproduced.

## Subsequent real-music audit

The 12-song, 24-render audit in `CORPUS_VALIDATION.md` extends the earlier fixture checks. It found a peak-limit issue on Cipher despite the existing regression suite passing: 22/24 full-song renders pass the independent FFmpeg ceiling criterion, while Cipher fails in both modes. A third reconstruction confirms a Louder ceiling breach. This supersedes any inference that the earlier six-format fixture result established universal peak compliance. The audit also records a substantial relative tonal movement on A Quiet Descent, bounded deterministic EQ probes and additional mono/rate cases. Musical preference remains untested; 19 level-matched excerpts are prepared for listening rather than assigned invented scores.

Version 0.4.1 resolves those reproducible peak/intensity failures and corrects final EQ compensation status. Its final binary passes all 24 saved-song checks, six Cipher formats in both modes, the separate Runecore export harness, 24 EQ probes and 73 unit/integration tests. Full figures, exact limits, benchmark cost and remaining listening/conformance boundaries are in `CORPUS_VALIDATION.md` and the included fix-verification evidence. The historical 0.3.1 speedup must not be treated as a new timing of the stronger 0.4.1 pipeline.
