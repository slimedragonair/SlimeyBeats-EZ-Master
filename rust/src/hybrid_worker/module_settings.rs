//! Admission bounds for the private serialized module controls. DSP stays shared.
use anyhow::{Context, Result, ensure};
use serde_json::Value;

fn range(module: &str, key: &str) -> Option<(f64, f64)> {
    Some(match (module, key) {
        ("headroom", "gain_db") => (-24., 0.),
        ("declip", "max_run") => (1., 256.),
        ("cleanup", "hpf_hz") => (5., 300.),
        ("denoise", "amount") | ("static_eq", "fraction") | ("saturation", "mix") => (0., 1.),
        ("hf_rebuild", "mix_db") | ("low_end", "psycho_bass_db") => (-60., 0.),
        ("static_eq", "max_db" | "max_boost_db" | "response_max_db" | "tolerance_db")
        | ("resonance", "max_db")
        | ("low_punch", "attack_db" | "sustain_db" | "sub_duck_db")
        | ("dynamic_eq", "mud_max_db" | "harsh_max_db")
        | ("deesser", "max_db")
        | ("saturation", "drive_db")
        | ("clipper", "drive_db")
        | ("reference_match", "max_db") => (0., 12.),
        ("dynamic_eq", "total_cut_max_db") => (0., 24.),
        ("resonance", "max_notches") => (0., 16.),
        ("deesser", "lo_hz" | "hi_hz") => (1000., 20000.),
        ("glue", "ratio") => (1., 10.),
        ("glue", "attack_ms") => (0.1, 200.),
        ("glue", "release_ms") => (5., 2000.),
        ("stereo", "width") => (0., 2.),
        ("limiter", "lookahead_ms") => (0.1, 20.),
        ("limiter", "release_ms") => (1., 1000.),
        ("limiter", "release_slow_ms") => (1., 5000.),
        ("limiter", "max_makeup_db") => (0., 40.),
        ("limiter", "max_gr_abort_db") => (0., 12.),
        _ => return None,
    })
}

pub fn validate(module: &str, controls: &Value) -> Result<()> {
    for (key, value) in controls
        .as_object()
        .context("Module controls must be objects")?
    {
        if key == "enabled" {
            ensure!(value.is_boolean(), "Module enabled must be boolean");
        } else {
            let (low, high) = range(module, key).context("Unknown module control")?;
            let number = value.as_f64().context("Module controls must be numeric")?;
            ensure!(
                number.is_finite() && (low..=high).contains(&number),
                "Module control outside server bounds"
            );
            if matches!(
                (module, key.as_str()),
                ("resonance", "max_notches") | ("declip", "max_run")
            ) {
                ensure!(number.fract() == 0., "Module count must be an integer");
            }
        }
    }
    Ok(())
}
