"""Complete bounded Analysis shape for schema tests; native tests use real analysis."""


def reference_analysis():
    return {
        "duration_s": 1, "sample_rate": 48000, "channels": 1,
        "metrics": {"integrated_lufs": -18, "true_peak_dbtp": -3, "sample_peak_dbfs": -3,
                    "rms_dbfs": -20, "crest_db": 17, "lra_lu": 4, "plr_db": 15,
                    "correlation": 0.9, "dc_offset": [0], "clipped_samples": 0},
        "spectrum": {"freqs": [20 + i * 30 for i in range(240)], "db": [-40] * 240,
                     "cutoff_hz": None, "quiet_hf_flatness": 0.3, "noise_candidate": False,
                     "sibilance_spike_fraction": 0.1, "octave_power": [0.01] * 10},
        "measured_db": [0] * 10, "deviation_db": [0] * 10, "valid": [True] * 10,
        "issues": [{"key": "private/path", "severity": 1, "detail": "private reference title"}],
        "suggested_profile": "private reference title",
    }
