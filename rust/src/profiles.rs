use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

pub const BANDS: [f64; 10] = [
    31.5, 63., 125., 250., 500., 1000., 2000., 4000., 8000., 16000.,
];
pub fn builtin() -> BTreeMap<String, Value> {
    let db: Value =
        serde_json::from_str(include_str!("../../mastercraft/data/builtin_profiles.json"))
            .expect("Bundled profiles must be valid JSON");
    db["profiles"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
pub fn number(value: &Value, path: &[&str], default: f64) -> f64 {
    let mut cur = value;
    for key in path {
        cur = if cur.is_array() {
            key.parse::<usize>()
                .ok()
                .and_then(|i| cur.get(i))
                .unwrap_or(&Value::Null)
        } else {
            &cur[*key]
        };
    }
    cur.as_f64().filter(|v| v.is_finite()).unwrap_or(default)
}

pub fn resolve(name: &str, custom_dir: &Path) -> Result<Value> {
    let name = name.trim().strip_prefix("combo:").unwrap_or(name.trim());
    if name.contains('+') {
        let mut parts = Vec::new();
        for part in name.split('+') {
            let (key, weight) = part
                .rsplit_once(':')
                .filter(|(_, w)| w.parse::<f64>().is_ok())
                .unwrap_or((part, "1"));
            let w: f64 = weight.parse()?;
            ensure!(w.is_finite() && w > 0., "Blend weights must be positive");
            parts.push((resolve(key, custom_dir)?, w));
        }
        let refs: Vec<_> = parts.iter().map(|(v, w)| (v, *w)).collect();
        let mut mixed = blend_values(&refs);
        mixed["display_name"] = Value::String(name.into());
        return Ok(mixed);
    }
    let key = name.to_lowercase().replace([' ', '-', '/'], "_");
    let alias = match key.as_str() {
        "hiphop" | "hip_hop" | "hip_hop_trap" | "trap" | "rap" => "hiphop_trap",
        "game" | "ost" | "soundtrack" | "instrumental" => "game_ost",
        "synthwave" | "retro" => "synthwave_retro",
        "metalcore" | "djent" => "metalcore_djent",
        "alt" | "indie" => "alt_rock",
        "acoustic" | "folk" => "acoustic_folk",
        "orchestral" | "cinematic" => "orchestral_cinematic",
        "rnb" | "soul" => "rnb_soul",
        "electronic" | "dance" => "edm",
        other => other,
    };
    let all = builtin();
    if let Some(p) = all.get(alias) {
        return Ok(p.clone());
    }
    if let Some((_, p)) = all.iter().find(|(_, p)| {
        p["display_name"]
            .as_str()
            .is_some_and(|d| d.eq_ignore_ascii_case(name))
    }) {
        return Ok(p.clone());
    }
    let file = custom_dir.join(format!(
        "{}.json",
        name.strip_prefix("custom:").unwrap_or(name)
    ));
    if file.is_file() {
        let p: Value =
            serde_json::from_str(std::fs::read_to_string(&file)?.trim_start_matches('\u{feff}'))?;
        ensure!(
            p["tonal_db"].as_array().is_some_and(
                |a| a.len() == 10 && a.iter().all(|v| v.as_f64().is_some_and(f64::is_finite))
            ),
            "Custom profile needs ten finite tonal_db values"
        );
        return Ok(p);
    }
    bail!("Unknown profile '{name}'. Use --list-profiles or a blend such as metal:0.6+edm:0.4")
}

fn blend_values(parts: &[(&Value, f64)]) -> Value {
    let Some((first, _)) = parts.first() else {
        return Value::Null;
    };
    match first {
        Value::Number(_) => {
            let total: f64 = parts
                .iter()
                .filter(|(v, _)| v.is_number())
                .map(|(_, w)| w)
                .sum();
            serde_json::json!(
                parts
                    .iter()
                    .filter_map(|(v, w)| v.as_f64().map(|x| x * w))
                    .sum::<f64>()
                    / total
            )
        }
        Value::Object(map) => Value::Object(
            map.keys()
                .map(|key| {
                    let child: Vec<_> = parts
                        .iter()
                        .filter_map(|(v, w)| v.get(key).map(|v| (v, *w)))
                        .collect();
                    (key.clone(), blend_values(&child))
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            (0..items.len())
                .map(|i| {
                    blend_values(
                        &parts
                            .iter()
                            .filter_map(|(v, w)| v.get(i).map(|v| (v, *w)))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect(),
        ),
        _ => (*first).clone(),
    }
}

pub fn target_curve(profile: &Value) -> Result<Vec<f64>> {
    profile["tonal_db"]
        .as_array()
        .context("Profile lacks tonal_db")?
        .iter()
        .map(|v| v.as_f64().context("Invalid profile band"))
        .collect()
}
