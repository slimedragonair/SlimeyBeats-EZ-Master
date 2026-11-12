# Native mastering architecture

| Responsibility | Files |
|---|---|
| Audio validation/decoding | `src/audio.rs` |
| Settings, profile resolution, locations | `src/config.rs`, `src/profiles.rs`, `src/paths.rs` |
| EBU measurements | `src/meters.rs` |
| Spectrum and transient measurements | `src/analysis/`, including `playback_spectrum.rs` for bounded playhead FFT windows |
| Planning, stage dispatch, safety selection | `src/engine/planning.rs`, `chain.rs`, `mod.rs` |
| DSP algorithms | `src/dsp/biquad.rs`, `convolution.rs`, `dynamics.rs`, `limiter.rs`, `low_end.rs`, `repair.rs`, `stereo.rs`, `tone.rs`, `manual_eq.rs` |
| Transactional encoding and output verification | `src/export.rs` |
| Desktop state and background jobs | `src/gui/app.rs`, `worker.rs`, `job_events.rs` |
| Desktop layout, panels, plots, auditioning | `src/gui/shell.rs`, `queue_panel.rs`, `settings_panel.rs`, `overview.rs`, `eq_panel.rs`, `plots.rs`, `transport.rs`, `player.rs`, `theme.rs`, `brand_art.rs`, `help.rs`, `eq_curve.rs`, `live_spectrum.rs` |
| Batch interface | `src/cli_arguments.rs`, `src/bin/mastercraft_cli.rs` |

Input decodes into validated interleaved float32 samples. Filters maintain double-precision state. Analysis combines EBU loudness measurement with bounded, evenly distributed FFT windows; it avoids retaining an entire STFT matrix. Complementary zero-phase bands use FFT overlap-save convolution. Dynamic stages compute linked block-rate envelopes and interpolate gains. The limiter prepares its oversampled peak envelope once, then reuses it during gain searches. Final peak measurement keeps the maximum of sample peak, the EBU-library estimate and the longer native reconstruction; independent saved-file checks remain separate validation.

Safety retries reuse input analysis and transient detections and retain only the best candidate. They compare the revised low-band punch metric and selected tonal heuristics. All report notes preserve whether safety could resolve the detected regression. No measurement is presented as proof of perceived quality.

The desktop never runs full-track mastering, export preparation, or encoding inside a UI frame or audio callback. Stateful live EQ and peak guarding run in the audio source; parameter compilation and whole-track measurement stay outside it. Workers communicate progress, results, failures, and completion over a channel; cancellation is checked between stages, limiter iterations, and export operations. The player reads immutable shared source buffers, atomic transport controls, and a coalesced parameter channel. The analyzer reads a bounded playback tap before listening volume and A/B gain. GUI buffers keep the original, automatic master, and current EQ revision distinct, so repeated EQ renders do not accumulate filtering.

Desktop Analyze and Master produce in-memory results. Export is a separate job and writes the selected revision without rerunning mastering. Encoders work on temporary files in the chosen output directory; verification precedes `persist_noclobber`. Existing outputs survive collisions, parallel exports, and failed encoding attempts. Explicit JSON report paths also reject replacement. This file-level transaction does not promise all-or-nothing behavior across an entire multi-format batch: completed files remain if a later format fails.

Built-in profiles remain owned by the existing data file, embedded at compile time. New diagnostic families should go into dedicated analysis modules; new processing stages belong in focused DSP modules, with planning separated from execution. Extend `Report` when a new control changes reproducibility. Tests should check audio invariants, timing units, independent meters, and actual export behavior.


`src/dsp/eq_suggestion.rs` owns deterministic manual-EQ proposals. Its weights, caps, and quantization are named constants; it never writes files or changes audio. A worker measures the pre-EQ buffer and sends a suggestion event. The UI distinguishes suggested bands, editable parameters, and the rendered revision. Playback FFTs operate on short windows and cache the last position; the full mastering chain never runs during UI painting.

Future web/iPhone work should extract the reusable buffer/DSP/measurement modules behind platform adapters. Native process-based decoding/export, filesystem dialogs, thread orchestration, and rodio playback need browser replacements. WebAssembly workers, bounded-memory processing, format coverage, and actual iPhone tests remain future engineering work. The current deliverable is the Windows desktop application and CLI.


Real-time EQ ownership:

- `src/dsp/eq_chain.rs`: compiled coefficients, per-channel double-precision filter state, and pole-decay timing.
- `src/dsp/realtime_eq.rs`: bounded raw history, warmed parallel states, convex crossfades, and shared frame processing.
- `src/dsp/stream_peak_guard.rs`: 32-tap fractional reconstruction, linked lookahead gain, release, and delay.
- `src/dsp/manual_eq.rs`: fixed-state rendering through the same processor, delay compensation, loudness solving, and independent final peak verification.
- `src/dsp/eq_loudness.rs`: immediate spectral loudness estimate for interactive controls.
- `src/gui/audio_source.rs`, `playback_tap.rs`: continuous audition, delay-aligned A/B/seek, and actual pre-volume analyzer samples.
- `src/gui/eq_live_control.rs`, `eq_jobs.rs`, `job_events.rs`: revisions, debounce, source identity, export preparation, and rejection of stale results.
- `src/gui/eq_curve.rs`, `live_spectrum.rs`, `timeline_panel.rs`: one layered graph and a pinned timeline.

Steady audio processing reuses its DSP buffers. Coefficient changes can warm up to 1.5 seconds of retained input history; this work is bounded but can affect callback timing under extreme settings or unusually high rates. The application does not claim a universal hard real-time deadline. Automated tests exercise fixed live/export alignment, immediate channel-driven parameter changes, peak safety, gain ramps, and stale async-result rejection.

Spectrum units and compensation contract:

- The Hann-window FFT is power-normalized by window energy, FFT length and channel count. Display points average power per FFT bin across local frequency ranges and convert with 10 log10. Octave measurements instead sum bins across each octave. White noise is therefore approximately flat in the per-bin trace and rises about 3 dB per octave in the octave totals.
- Full-track analysis averages active windows, with at most 3072 evenly distributed windows. FFT length is 4096 up to 50 kHz sample rate, otherwise 8192. Live analysis uses 4096 samples and shows a playhead window or the actual output tap. The graph explicitly identifies the current mode; temporal averages and instantaneous windows need not coincide.
- No visual tilt is applied. Spectrum values are energy per bin relative to full-scale sample amplitude, not tone-amplitude calibration or integrated octave levels. Resolution and window conventions affect their values. The EQ response is a separate gain transfer function on its own axis.
- `EqLoudnessModel` integrates the per-bin trace on a log-frequency grid, so its relative-energy quadrature uses a frequency factor for represented bandwidth. It applies an approximate K-like weighting and does not reconstruct EBU time gating. Steady noise/multitone tests check the units and a bounded estimate error for broad EQ; they do not certify the estimate as a LUFS meter. Full-track measured preparation remains authoritative.

`tools/package_release.py` owns portable release assembly. It uses an explicit source allowlist, embeds no personal audio/settings/exports, preserves the profile JSON's compile-time relative location, gathers dependency attribution, and refuses an existing release destination.

`src/gui/ui_capture.rs` provides opt-in visual QA through egui 0.37's renderer screenshot callback. `--capture-ui PATH` waits for jobs/EQ measurements and five settled frames, writes a new PNG without replacing a file, then closes that diagnostic instance. `--capture-size WIDTH HEIGHT` exercises another window size. `--capture-settings-scroll OFFSET` opens only Export settings and scrolls the real settings viewport for review; its transient app field is not saved in Settings JSON. Captures are actual rendered application pixels, not a synthesized layout or automation of an active user window. Ordinary sessions never enable these options.

Current interface ownership:

The 0.4.1 DSP additions are owned by `dsp/true_peak.rs` and `engine/tonal_correction.rs`, with separate regression modules. The former supplies offline reconstruction/envelopes and boundary handling; the latter bounds automatic static correction before intensity and exposes the remaining regional cut budget to dynamic planning. `docs/DSP_SAFETY.md` explains their flow and extension points. No reconstruction scan runs in the audio callback.

- `skin_frame.rs` and `skin_frame_tests.rs`: shared eight-patch metal frames. Corners and edges use the same 128-pixel source grid, preserving visible alpha connections. Bilinear sampling/flood-fill tests check all corners at three window sizes and 1×/1.25×/2× density; a deliberately displaced side is rejected.
- `brand_art.rs` and the generated PNGs: cached portraits, icon, crest and ambient ribbon textures. Crest aspect is fixed; ribbons use aspect-preserving source crops behind non-data regions. Installed fonts are loaded at runtime, never redistributed.
- `slime_border.rs`: uniformly scaled header/footer crowns and tapered droplet-free shoulders. The old side/corner goo slices are removed. Decoration creates no input regions, timers or audio work.
- `crystal_button.rs`, `crystal_slider.rs`, `skin_icons.rs`, `theme.rs`: native input/semantics with custom paint. The egui 0.37 slider's actual emitted handle shape supplies the jewel position; mapping is not guessed from a response rectangle containing labels. Revalidate paint-shape detection and native comparison tests when upgrading egui.
- `welcome_panel.rs`, `overview.rs`, `queue_panel.rs`: welcome art/cards, real measurements/reports, bounded queues and drop target. Hero text height is measured; step rows align at the top rather than starting zero-height children at the row midpoint.
- `settings_panel.rs` and its tests: width-bounded value fields, measured native slider overhead, long-path/profile truncation, wrapped actions and Export disclosure. Repeated-frame tests keep resizable panels stable across welcome/loaded states, expanded controls, long values and 125% scale without changing settings.
- `eq_band_controls.rs`, `eq_curve.rs`, `timeline_panel.rs`, `plots.rs`, `transport.rs`: preallocated wrapping EQ groups, layered data graph, compact waveform on small windows, intact control/clock wrapping and pinned playback. Measured waveform labels and time ticks have dedicated bands outside the trace lanes. Layout never changes the DSP revision model.
- `shell.rs`: allocated panel bounds, clipping, chrome and status composition. Chrome follows allocated workspace bounds even if a child requests extra room. Responsive side limits retain workspace space, solid scrollbars expose overflow, and footer text reserves the central crest area.

The default client is 1480×940 logical pixels, minimum 1100×720. At the minimum, editing/settings content scrolls while playback and Master/Export actions stay accessible. The skin changes no audio processing or export algorithms. Recheck actual empty, loaded, narrow, expanded and scrolled captures when altering ornament or typography; touching rectangles alone do not establish connected visible artwork.
