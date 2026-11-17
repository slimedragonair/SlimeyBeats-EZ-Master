export const MAX_JSON_BYTES = 1024 * 1024;
const PRIVATE_KEY = /(?:path|directory|filename|file_name|out_dir|url|uri|token|password|secret)|^(?:file|reference)$/i;
const hasPath = (text) => /[\\/]|\b[a-z]:|(?:file|https?):/i.test(text);
export function plainName(value, fallback = "Untitled") {
    if (value === undefined || value === null) return fallback;
    if (typeof value !== "string" || value.length > 4096) throw new Error("Invalid export display name");
    const name = value.replace(/[\u0000-\u001f\u007f]/g, "").split(/[\\/]/).pop().replace(/^[a-z]:/i, "").trim();
    return name.slice(0, 512) || fallback;
}
// Never copy arbitrary report/settings objects directly into a portable artifact.
// Bound traversal before serialization and remove path-bearing strings and keys.
export function portableTree(value) {
    let nodes = 0;
    const active = new WeakSet();
    function visit(item, depth) {
        if (++nodes > 65536 || depth > 12) throw new Error("Export metadata is too complex");
        if (item === null || typeof item === "boolean") return item;
        if (typeof item === "number") {
            if (!Number.isFinite(item)) throw new Error("Export metadata contains a nonfinite number");
            return item;
        }
        if (typeof item === "string") {
            if (item.length > 4096) throw new Error("Export metadata text is too long");
            return hasPath(item) ? "[path removed]" : item.replace(/[\u0000-\u001f\u007f]/g, "");
        }
        if (!item || typeof item !== "object" || active.has(item)) throw new Error("Export metadata must be a finite JSON tree");
        active.add(item);
        let result;
        if (Array.isArray(item)) {
            if (item.length > 8192) throw new Error("Export metadata array is too large");
            result = item.map((entry) => visit(entry, depth + 1));
        } else {
            if (![Object.prototype, null].includes(Object.getPrototypeOf(item))) throw new Error("Export metadata must contain plain objects");
            const keys = Object.keys(item);
            if (keys.length > 128) throw new Error("Export metadata object is too large");
            result = Object.create(null);
            for (const key of keys) {
                if (key === "__proto__" || key === "constructor" || key === "prototype" || PRIVATE_KEY.test(key) || hasPath(key)) continue;
                if (key.length > 120) throw new Error("Export metadata key is too long");
                if (item[key] !== undefined) result[key] = visit(item[key], depth + 1);
            }
        }
        active.delete(item);
        return result;
    }
    return visit(value, 0);
}
export function jsonBytes(value) {
    const bytes = new TextEncoder().encode(JSON.stringify(value, null, 2) + "\n");
    if (bytes.length > MAX_JSON_BYTES) throw new Error("Export JSON exceeds the 1 MiB metadata budget");
    return bytes;
}
export function select(value, keys) {
    const result = {};
    for (const key of keys) if (value?.[key] !== undefined) result[key] = value[key];
    return result;
}
export function sourceMetadata(value) {
    if (!value) return null;
    return { name: plainName(value.name), ...portableTree(select(value,
        ["duration_seconds", "sample_rate", "channels", "schema", "schema_version", "version"])) };
}
const MEASUREMENTS = ["duration_s", "sample_rate", "channels", "metrics", "spectrum", "measured_db", "valid"];
const METRICS = ["integrated_lufs", "true_peak_dbtp", "sample_peak_dbfs", "lra_lu", "rms_dbfs", "crest_db", "plr_db",
    "correlation", "dc_offset", "clipped_samples"];
const SPECTRUM = ["freqs", "db", "cutoff_hz", "quiet_hf_flatness", "sibilance_spike_fraction", "octave_power"];
const HIGH_MEASUREMENTS = ["version", "band_low_hz", "band_high_hz", "reference_low_hz", "reference_high_hz",
    "sampled_windows", "active_windows", "mean_high_power_fraction", "p90_high_to_presence_db", "mean_high_flatness", "sampling_capped"];
const HIGH_GUIDANCE = ["version", "candidate_windows", "candidate_groups", "candidate_fraction", "observed_candidate_seconds", "excess_candidate", "suggested_cut_db"];
const HEURISTICS = ["deviation_db", "issues", "suggested_profile"];
export function analysisDocument(report) {
    if (report !== undefined && report !== null && (typeof report !== "object" || Array.isArray(report)))
        throw new Error("Export analysis must be a report or measurement object");
    const pair = report?.input !== undefined || report?.output !== undefined
        ? { input: report.input, output: report.output } : { preview: report };
    const measurements = {}, suggestions = {};
    for (const [key, value] of Object.entries(pair)) {
        const measured = value?.metrics ? select(value, MEASUREMENTS) : { metrics: select(value, METRICS) };
        if (value?.spectrum) measured.spectrum = select(value.spectrum, SPECTRUM);
        if (value?.spectrum?.high_frequency)
            measured.spectrum.high_frequency = select(value.spectrum.high_frequency, HIGH_MEASUREMENTS);
        const recorded = value && (Object.keys(measured.metrics || {}).length || Object.keys(measured).some((field) => field !== "metrics"));
        measurements[key] = recorded ? portableTree(measured) : null;
        const suggested = select(value, HEURISTICS);
        if (value?.spectrum?.noise_candidate !== undefined) suggested.noise_candidate = value.spectrum.noise_candidate;
        if (value?.spectrum?.high_frequency)
            suggested.high_frequency = select(value.spectrum.high_frequency, HIGH_GUIDANCE);
        suggestions[key] = value ? portableTree(suggested) : null;
    }
    return {
        schema: "slimeybeats-analysis-export", version: 1,
        producer: portableTree(select(report, ["schema", "schema_version", "version"])),
        measurement_scope: "Caller-supplied DSP analysis; this packager does not remeasure encoded audio.",
        suggestion_scope: "Heuristic warnings, profile proposals and deviations are guidance, not measurements or accepted EQ.",
        status: Object.values(measurements).some((value) => value !== null) ? "recorded" : "not_recorded", measurements, suggestions,
        processing: portableTree(select(report, ["profile", "target_lufs", "ceiling_dbtp", "limiter", "log",
            "processing_seconds", "punch_loss_db", "safety_attempts", "notes", "exports"])),
    };
}
