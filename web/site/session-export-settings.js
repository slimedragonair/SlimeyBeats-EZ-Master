import { jsonBytes, portableTree, select } from "./session-export-metadata.js";
const TYPES = ["bell", "low_shelf", "high_shelf", "low_cut", "high_cut"];
const FORMATS = ["wav16", "wav24", "wav32f", "flac16", "flac24", "mp3"];
const MODULES = ["headroom", "declip", "cleanup", "denoise", "hf_rebuild", "static_eq", "resonance", "low_end",
    "low_punch", "dynamic_eq", "deesser", "multiband", "glue", "saturation", "stereo", "clipper", "reference_match", "limiter"];
function range(value, low, high, field) {
    if (typeof value !== "number" || !Number.isFinite(value) || value < low || value > high)
        throw new Error(`Invalid export setting: ${field}`);
    return value;
}
function boolean(value, field) {
    if (typeof value !== "boolean") throw new Error(`Invalid export setting: ${field}`);
    return value;
}
export function eqProfile(settings) {
    if (!Array.isArray(settings?.manual_eq) || settings.manual_eq.length !== 7)
        throw new Error("Portable EQ requires exactly seven bands");
    return {
        // Matches the desktop EQ preset contract, including its version.
        version: 2,
        bands: settings.manual_eq.map((band) => {
            if (!TYPES.includes(band?.type)) throw new Error("Invalid export EQ type");
            return { type: band.type, freq: range(band.freq, 5, 20000, "frequency"),
                gain: range(band.gain, -18, 18, "band gain"), q: range(band.q, 0.1, 18, "Q"),
                on: boolean(band.on, "band enabled") };
        }),
        hold_loudness: boolean(settings.eq_hold_loudness, "EQ loudness hold"),
        out_gain_db: range(settings.eq_output_db, -12, 12, "EQ output"), bypass: false,
    };
}
export function portableSettings(settings) {
    const eq = eqProfile(settings), result = {
        manual_eq: eq.bands, eq_output_db: eq.out_gain_db, eq_hold_loudness: eq.hold_loudness,
    };
    for (const [key, low, high] of [["target_lufs", -40, -5], ["ceiling_dbtp", -12, 0],
        ["intensity", 0, 1.5], ["match_strength", 0, 1.5], ["max_makeup_db", 0, 40], ["max_limiter_gr_db", 0, 12]]) {
        if (settings[key] !== undefined)
            result[key] = key === "target_lufs" && settings[key] === null ? null : range(settings[key], low, high, key);
    }
    for (const key of ["match_loudness", "match_width", "dither", "safety"])
        if (settings[key] !== undefined) result[key] = boolean(settings[key], key);
    if (settings.profile !== undefined) {
        if (typeof settings.profile !== "string" || settings.profile.length > 256 ||
            !/^[a-z][a-z0-9_]*(?::[0-9.]+)?(?:\+[a-z][a-z0-9_]*(?::[0-9.]+)?)*$/i.test(settings.profile))
            throw new Error("Invalid portable tonal profile");
        result.profile = settings.profile;
    }
    if (settings.mode !== undefined) {
        if (!["streaming_safe", "loud"].includes(settings.mode)) throw new Error("Invalid portable mastering mode");
        result.mode = settings.mode;
    }
    if (settings.formats !== undefined) {
        if (!Array.isArray(settings.formats) || !settings.formats.length || settings.formats.length > FORMATS.length ||
            settings.formats.some((format) => !FORMATS.includes(format))) throw new Error("Invalid portable export formats");
        result.formats = [...settings.formats];
    }
    if (settings.output_sr !== undefined) {
        if (!Number.isInteger(settings.output_sr) || (settings.output_sr !== 0 &&
            (settings.output_sr < 8000 || settings.output_sr > 192000))) throw new Error("Invalid portable output sample rate");
        result.output_sr = settings.output_sr;
    }
    if (settings.suffix !== undefined) {
        if (typeof settings.suffix !== "string" || settings.suffix.length > 120 || /[\\/:]|\.\./.test(settings.suffix))
            throw new Error("Invalid portable export suffix");
        result.suffix = settings.suffix;
    }
    if (settings.disabled !== undefined) {
        if (!Array.isArray(settings.disabled) || settings.disabled.length > 32 ||
            settings.disabled.some((name) => !MODULES.includes(name)))
            throw new Error("Invalid portable disabled modules");
        result.disabled = [...settings.disabled];
    }
    if (settings.modules !== undefined) result.modules = portableTree(settings.modules);
    return result;
}
export function eqProfileJson(settings) { return jsonBytes(eqProfile(settings)); }
export function settingsJson(settings) {
    return jsonBytes({ schema: "slimeybeats-settings-export", version: 1,
        producer: portableTree(select(settings, ["schema", "schema_version", "version"])), ...portableSettings(settings) });
}
