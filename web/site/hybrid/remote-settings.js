const FIELDS = ["profile", "mode", "target_lufs", "ceiling_dbtp", "intensity", "safety", "manual_eq",
    "eq_output_db", "eq_hold_loudness", "dither", "match_strength", "match_loudness", "match_width",
    "max_makeup_db", "max_limiter_gr_db", "output_sr", "disabled"];
export function remoteSettings(settings) {
    return Object.fromEntries(FIELDS.filter(key => Object.hasOwn(settings, key))
        .map(key => [key, structuredClone(settings[key])]));
}
