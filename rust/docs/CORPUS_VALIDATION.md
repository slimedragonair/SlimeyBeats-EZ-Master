# Real-music corpus audit — November 12, 2026

Version tested: 0.4.0. Six Music-folder originals and six artist-released CC BY tracks provided 42 minutes of distinct full songs. Neutral Gentler/Louder produced 24 WAV24 renders. Eight existing mastered files were measured separately; they are not controlled current-provider benchmarks. Two dry mono takes, three 12-second rate-converted fixtures and one reduced-intensity render provided six extra cases.

The audit preserved source hashes and independently decoded exports with FFmpeg 8.0.1. All 24 runs preserved sample rate, channel count and frame count, with finite samples and no full-scale clipped samples. FFmpeg integrated loudness agreed within 0.054 LU. All 12 Gentler runs reached −14 LUFS within 0.2 LU. Louder targets were not reached on Cipher and Gymnopedie because of gain/limiter limits; the app reported those limits.

## Peak-ceiling issue recorded in 0.4.0

Cipher at 44.1 kHz exposes an internal true-peak under-read. With a −1 dBTP ceiling, the app reports about −1.16 dBTP while FFmpeg reads −0.8 dBTP for Gentler and −0.6 dBTP for Louder. A separate 16× Kaiser reconstruction reads −1.024 and −0.893 dBTP: the latter independently confirms an intersample-peak ceiling breach. These methods are approximations, not a conformance certification, but the discrepancy cannot be dismissed by checking the same internal meter again.

The Cipher Gentler export matrix fails the independent ceiling criterion on WAV16/24/float and FLAC16/24; MP3 passes at −1.1 dBTP. Runecore passes the separate six-format harness with 44.1 kHz conversion, collisions and expected failure exits. No sample clipping was found in either matrix. The discrepancy remained open at the end of that audit; DSP was not retuned during the original testing request. Version 0.4.1's fix is described in `DSP_SAFETY.md`; the earlier measurements are preserved as the failing baseline.

Prioritize a stronger reconstruction/final guard and repeat the decoded-export checks before asserting cross-meter peak compliance. Preserve the failing files and original settings. Do not loosen the test tolerance to mask the failure.

## Tonal and EQ results

A Quiet Descent's default processing shifts the 4 kHz octave +5.07 dB relative to median 125 Hz–8 kHz band movement. Intensity 0.35 reduces that to +4.18 dB. This is a whole-pipeline energy measurement, not a standalone EQ response or subjective defect verdict. Blind listening is needed before retuning tonal behavior.

`examples/corpus_eq_probe.rs` runs actual suggestions and fixed shared-processor renders against a private corpus without embedding music in tests. All 24 cases produced identical repeated suggestions within the intended per-band bounds. On 12-second middle excerpts, fast EQ level estimates differed from measured filter deltas by at most 0.040 dB; resolved loudness hold differed by at most 0.048 LU. These mild-curve checks do not qualify extreme settings, full-device latency or arbitrary music.

The manifest is a JSON array of `{ "id": "fixture", "report": "path/to/export.report.json" }`. Each report must include a valid saved export path. Run:

```powershell
cargo run --release --example corpus_eq_probe -- MANIFEST.json NEW_RESULTS.json
```

The destination must be new. The example checks finite samples, unchanged duration, deterministic bounded suggestions and native peak protection; it records estimation/hold error rather than treating its fast estimate as a LUFS meter. Independent saved-file checks remain separate.

## Evidence and limits

The local deliverable `SlimeyBeats-Testing-2026-11-12` includes the detailed report, CSV, JSON, plot, source credits, scripts and 19 level-matched 24-second FLAC excerpts across five blind comparisons. No listening preference ratings were collected. Online inputs are already released MP3s from one artist, limiting musical diversity and premaster conclusions.

Three processing-only runs each on Runecore, Riding Lightspeed and Gymnopedie were sequential with four Rayon threads after the other audit jobs completed. Medians were 6.78 s / 179.80 s audio, 13.74 s / 359.56 s audio and 7.65 s / 187.09 s audio respectively. Startup, decoding and export are excluded. This is approximately 24–27× real time on this PC, not a competitor or language benchmark. A fresh Runecore WAV repeats byte-for-byte including deterministic dither.

The existing 63 tests, formatting and Clippy passed after adding the probe. More genres/artists, unmastered studio material, actual listening preferences and independent peak correction remain required before broad quality claims.

## 0.4.1 correction and retest

The peak and intensity issues were corrected and the final 0.4.1 binary reran all 24 full-song cases. All passed independent decoded peak/integrity checks. Cipher passed all six formats in both modes; Louder MP3 required and received a second corrective encode. The third-method 16× reconstruction measured −1.115 dBTP and −1.080 dBTP for its Gentler/Louder WAVs. Runecore's separate six-format/error-path harness also passed.

A Quiet Descent's default relative 4 kHz movement fell from +5.07 dB to +1.08 dB. At intensity 0.35 it is +0.46 dB, compared with +4.18 dB in the failing baseline. This is whole-song band energy relative to median core-band change, not a standalone filter response or a listening score.

The final suite passes 73 tests (41 unit, 24 audio, two analysis and six real-time EQ regressions). All 24 real-music EQ probes pass, with maximum measured hold error 0.035 LU and fast-estimate error 0.071 dB; final convergence statuses agree with the rendered levels. Mono/rate/intensity checks pass, a repeated WAV is identical, and the native GUI processes/captures successfully.

The stronger checks cost processing time. Fresh sequential three-run medians with four Rayon threads are 9.52 s / 179.80 s audio for Runecore, 20.04 s / 359.56 s for Riding Lightspeed and 8.50 s / 187.09 s for Gymnopedie, approximately 18–22× real time. Earlier medians were 6.78, 13.74 and 7.65 s. These are whole-pipeline timings, not an isolated detector/language or competitor comparison.

The portable 0.4.1 release includes the fix evidence. A separate verification report provides before/after plots and level-matched excerpts. Musical preference, more diverse premaster material and formal conformance/device qualification remain outside these technical passes.
