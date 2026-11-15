use serde_json::{Value, json};

pub(super) const CANCEL_FRAMES: usize = 4096;
pub(super) const MAX_ISSUES: usize = 512;
pub(super) const FULL_SCALE: f64 = 0.9999;
pub(super) const PLATEAU_TOLERANCE: f64 = 0.000001;
pub(super) const MIN_CLIP_FRAMES: usize = 3;
pub(super) const MAX_REPAIR_SECONDS: f64 = 0.002;
pub(super) const MAX_REPAIR_FRAMES: usize = 128;
pub(super) const QUIET_PEAK: f64 = 0.00031622776601683794; // -70 dBFS
pub(super) const SILENCE_SECONDS: f64 = 0.75;
pub(super) const ENDING_GAP_SECONDS: f64 = 1.5;
pub(super) const ACTIVE_RMS: f64 = 0.01; // -40 dBFS per active channel
pub(super) const RESUME_SECONDS: f64 = 1.;
pub(super) const MAX_SPECTRAL_WINDOWS: usize = 512;
pub(super) const FFT_SIZE: usize = 8192;
pub(super) const LOW_MID_FRACTION: f64 = 0.55;
pub(super) const UPPER_FRACTION: f64 = 0.06;
pub(super) const MAX_TOP_THREE_FRACTION: f64 = 0.45;
pub(super) const MIN_SPECTRAL_WINDOWS: usize = 8;
pub(super) const MIN_LOW_MID_SECONDS: f64 = 2.;
pub(super) const BELL_HZ: f64 = 320.;
pub(super) const BELL_Q: f64 = 0.9;
pub(super) const MAX_CUT_DB: f64 = 3.;
pub(super) const FADE_SECONDS: f64 = 0.02;

pub(super) fn report() -> Value {
    json!({
        "max_issues": MAX_ISSUES,
        "clipping": {"absolute_full_scale": FULL_SCALE,
            "minimum_plateau_frames": MIN_CLIP_FRAMES,
            "plateau_tolerance": PLATEAU_TOLERANCE,
            "overshoot_threshold": 1.0,
            "maximum_repair_run_seconds": MAX_REPAIR_SECONDS,
            "maximum_repair_run_frames": MAX_REPAIR_FRAMES},
        "silence": {"all_channel_peak_dbfs": -70.0,
            "minimum_seconds": SILENCE_SECONDS, "block_seconds": 0.01},
        "possible_ending": {"minimum_gap_seconds": ENDING_GAP_SECONDS,
            "minimum_preceding_active_seconds": RESUME_SECONDS,
            "minimum_resumed_active_seconds": RESUME_SECONDS,
            "active_channel_rms_dbfs": -40.0},
        "low_mid_buildup": {"band_hz": [200., 500.],
            "broadband_hz": [20., "nyquist"],
            "minimum_active_channel_rms_dbfs": -40.0,
            "minimum_band_power_fraction": LOW_MID_FRACTION,
            "minimum_above_700_hz_fraction": UPPER_FRACTION,
            "maximum_top_three_band_bins_fraction": MAX_TOP_THREE_FRACTION,
            "minimum_occupied_band_bins": 6,
            "occupied_bin_fraction_of_band_mean": 0.08,
            "minimum_matching_active_window_fraction": 1.0,
            "noncandidate_windows_split_regions": true,
            "minimum_active_windows": MIN_SPECTRAL_WINDOWS,
            "minimum_observed_active_seconds": MIN_LOW_MID_SECONDS,
            "fft_size": FFT_SIZE, "maximum_sampled_windows": MAX_SPECTRAL_WINDOWS},
        "repair": {"bell_hz": BELL_HZ, "bell_q": BELL_Q,
            "maximum_cut_db": MAX_CUT_DB, "fade_seconds": FADE_SECONDS,
            "time_rounding": "nearest_frame_half_up_end_exclusive"}
    })
}
