//! Distinguish a changed mastering direction from live EQ and delivery-only changes.
use crate::{config::Settings, report::Report};

const DIRECTION_FIELDS: &[&str] = &[
    "profile",
    "mode",
    "target_lufs",
    "intensity",
    "reference",
    "match_strength",
    "match_loudness",
    "match_width",
    "safety",
    "max_makeup_db",
    "max_limiter_gr_db",
    "disabled",
    "modules",
];

pub(super) fn direction_changed(report: &Report, current: &Settings) -> bool {
    let Ok(current) = serde_json::to_value(current) else {
        return true;
    };
    DIRECTION_FIELDS
        .iter()
        .any(|key| report.configuration[*key] != current[*key])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audio::Audio, preview, profiles};
    #[test]
    fn direction_changes_are_visible_until_remastered_while_eq_and_export_choices_remain_live() {
        let input = Audio::new(
            (0..24000)
                .map(|frame| (frame as f32 * 0.06).sin() * 0.08)
                .collect(),
            48000,
            1,
        )
        .unwrap();
        let original = Settings::default();
        let profile = profiles::resolve("neutral", std::path::Path::new(".")).unwrap();
        let report = crate::engine::master(
            &input,
            &original,
            &profile,
            None,
            "fixture.wav",
            &|_, _| {},
            &|| false,
        )
        .unwrap()
        .report;
        assert!(!direction_changed(&report, &original));
        let mut current = original.clone();
        current.manual_eq[3].gain = -1.;
        current.eq_output_db = -0.5;
        current.formats = vec!["wav16".into(), "mp3".into()];
        current.output_sr = 44100;
        current.out_dir = "Another export folder".into();
        current.suffix = "_final".into();
        current.dither = false;
        current.ceiling_dbtp = -1.5;
        assert!(!direction_changed(&report, &current));
        current.intensity = 0.5;
        assert!(direction_changed(&report, &current));
        let raw = crate::dsp::manual_eq::render(&input, &original, &|| false)
            .unwrap()
            .audio;
        let (_, _, revised) =
            preview::resolve_eq(&raw, &current, &profile, &report, &|| false).unwrap();
        assert!(
            direction_changed(&revised, &current),
            "An EQ render must not silently accept new mastering settings"
        );
        current.intensity = original.intensity;
        assert!(!direction_changed(&revised, &current));
        current.mode = "loud".into();
        assert!(direction_changed(&report, &current));
        let remastered = preview::master(
            &input,
            &current,
            &profile,
            None,
            "fixture.wav",
            &|_, _| {},
            &|| false,
        )
        .unwrap();
        assert!(!direction_changed(&remastered.report, &current));
    }
}
