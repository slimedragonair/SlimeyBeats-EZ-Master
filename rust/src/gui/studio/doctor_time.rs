//! Keep sample-scale findings distinguishable in both list labels and marker hints.
use super::editor_state::time_label_precise;

const SHORT_ISSUE_SECONDS: f64 = 0.01;

pub(super) fn range_label(start: f64, end: f64, sample_rate: u32) -> String {
    let duration = end - start;
    let short = duration < SHORT_ISSUE_SECONDS;
    let precision = if short { 6 } else { 3 };
    let range = format!(
        "{}–{}",
        time_label_precise(start, precision),
        time_label_precise(end, precision)
    );
    if short {
        let frames = (duration * sample_rate as f64).round().max(1.) as usize;
        format!(
            "{range} ({frames} frame{} per channel · {:.3} ms)",
            if frames == 1 { "" } else { "s" },
            duration * 1000.
        )
    } else {
        range
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_scale_ranges_have_distinct_endpoints_and_channel_frame_units() {
        let label = range_label(1002. / 48000., 1005. / 48000., 48000);
        assert!(label.starts_with("0:00.020875–0:00.020938"), "{label}");
        assert!(label.contains("3 frames per channel"), "{label}");
        assert!(label.contains(" ms)"));
        let label = range_label(0., 1. / 384000., 384000);
        assert!(label.starts_with("0:00.000000–0:00.000003"), "{label}");
        assert!(label.contains("1 frame per channel"));
        assert!(!label.contains("1 frames"));
    }

    #[test]
    fn ten_milliseconds_and_longer_keep_existing_timestamp_precision() {
        assert_eq!(range_label(0., 0.01, 48000), "0:00.000–0:00.010");
        assert_eq!(range_label(59.999, 60.1, 48000), "0:59.999–1:00.100");
    }
}
