# Peak reconstruction and automatic tone limits

Version 0.4.1 addresses the recorded 0.4.0 peak-limit failure and strength-scaling problem. The original corpus audit remains historical evidence rather than being relabelled as a passing run.

## Offline peak detection

`src/dsp/true_peak.rs` owns cached 64-tap Kaiser-windowed fractional-delay kernels at seven fractional phases, providing an 8× reconstruction alongside original sample values. It includes zero-padded start/end tails. Measurement keeps the maximum of this reconstruction, sample peak and the existing EBU library's estimate, rather than allowing one detector to hide a larger reading from another. A conservative local amplitude bound skips blocks that cannot exceed the known maximum.

`src/meters.rs` reports that combined estimate. `src/dsp/limiter.rs` uses the new detector for its input envelope and final guard. The guard retains a small margin below the selected ceiling; export still verifies the decoded saved file and attenuates/re-encodes if necessary. The older `oversampling::peak_envelope` API delegates to the shared detector; nonlinear oversampling remains separate.

This work runs in analysis/processing workers, not audio callbacks. The live EQ source retains its bounded streaming guard; settled full-track preparation supplies its measured final parameters and saved-export verification. The finite reconstruction and external cross-checks do not constitute EBU certification or a universal bound for every possible reconstruction filter or output device.

Regression coverage includes an analytic high-frequency signal, a licensed 256-sample numerical excerpt from the actual failing render, short hot starts/endings, silence, channel maxima and multiple sample rates. `tests/fixtures/README.md` records attribution. Real-song saved exports must also pass the independent FFmpeg checks; repeating the internal meter alone is insufficient.

## Automatic tonal correction

`src/engine/tonal_correction.rs` owns broad profile/reference EQ proposals and cascade response measurement. `planning.rs` owns dynamic stages; `chain.rs` supplies the static filters when planning dynamic cuts.

The full-strength profile proposal is capped before applying Correction intensity. Defaults are 2.5 dB per cut, 1 dB per boost and 2.5 dB for the combined static response, evaluated over a dense logarithmic grid. Global intensity then scales the bounded gains. This prevents large profile deviations from pinning low-intensity settings at full-strength caps. Existing low-band protection remains in place. Explicit reference matching retains its separate default 6 dB response budget and ignores frequency bands absent from the reference.

Dynamic mud/harshness/fizz cuts share a default 3 dB cut budget with the static response in their region, scaled by intensity. They no longer spend a full extra correction allowance on a region already cut by static EQ. Transient de-essing remains separately controlled. Advanced settings can specify `static_eq.max_db`, `max_boost_db`, `response_max_db`, and `dynamic_eq.total_cut_max_db`; these are correction budgets rather than a claim about measured whole-song octave-power changes.

Manual EQ, its deterministic suggestion function, selected loudness targets and intentional limiter/gain constraints remain separate. These bounds address reproducible behavior and reduce aggressive default moves; musical preference still requires listening.

`src/dsp/manual_eq.rs` determines compensation status from the final rendered loudness after output gain and peak protection. The solver's last intermediate flag cannot label an already resolved render as limited, or claim success after the final guard makes the requested level unreachable. This status correction does not change rendered samples.

For future detector changes, retain independent peak/codec regressions and compare processing time on the same decoded fixtures. For new automatic tone stages, account for existing correction before assigning another broad cut budget.
