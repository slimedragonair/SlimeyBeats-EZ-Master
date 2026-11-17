import test from "node:test";
import assert from "node:assert/strict";
import { analysisDocument } from "./session-export-metadata.js";

test("portable analysis retains bounded treble measurements separately from proposed cuts", () => {
    const input = { duration_s: 8, sample_rate: 48000, channels: 2, metrics: { integrated_lufs: -18 },
        spectrum: { freqs: [10000, 16000], db: [-40, -42], high_frequency: {
            version: 1, band_low_hz: 10000, band_high_hz: 16000,
            reference_low_hz: 2000, reference_high_hz: 8000, sampled_windows: 180, active_windows: 170,
            mean_high_power_fraction: .4, p90_high_to_presence_db: 6, mean_high_flatness: .5,
            sampling_capped: false, candidate_windows: 80, candidate_groups: 4, candidate_fraction: 80 / 170,
            observed_candidate_seconds: 2.1, excess_candidate: true, suggested_cut_db: -1.25,
            unexpected_private_field: "must be omitted",
        } } };
    const output = analysisDocument(input);
    assert.equal(output.measurements.preview.spectrum.high_frequency.band_low_hz, 10000);
    assert.equal(output.measurements.preview.spectrum.high_frequency.mean_high_power_fraction, .4);
    assert.equal(output.measurements.preview.spectrum.high_frequency.excess_candidate, undefined);
    assert.equal(output.measurements.preview.spectrum.high_frequency.suggested_cut_db, undefined);
    assert.equal(output.suggestions.preview.high_frequency.suggested_cut_db, -1.25);
    assert.equal(output.suggestions.preview.high_frequency.excess_candidate, true);
    assert.equal(output.suggestions.preview.high_frequency.observed_candidate_seconds, 2.1);
    assert.ok(!JSON.stringify(output).includes("must be omitted"));
    assert.equal(input.spectrum.high_frequency.suggested_cut_db, -1.25);
});

test("older and unavailable high-frequency reports keep their portable shape", () => {
    for (const high of [undefined, null]) {
        const output = analysisDocument({ metrics: { integrated_lufs: -20 }, spectrum: { high_frequency: high } });
        assert.equal(output.measurements.preview.spectrum.high_frequency, undefined);
        assert.equal(output.suggestions.preview.high_frequency, undefined);
    }
});
