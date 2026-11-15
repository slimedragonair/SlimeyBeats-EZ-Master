// Only values constructed here may enter a technical support report.
const CATEGORIES = new Set(["general", "import", "playback", "mastering", "studio", "recovery", "startup"]);
const FAILURES = new Set(["page_error", "operation_rejected", "startup_failed", "storage_failed"]);
export function redactDescription(value) {
    return String(value).slice(0, 1500)
        .replace(/(?:https?:\/\/|file:\/\/)[^\s<>]+/gi, "[link removed]")
        .replace(/\b[A-Z]:[\\/][^\r\n<>"|]+/gi, "[local path removed]")
        .replace(/\\\\[^\r\n<>"|]+/g, "[network path removed]")
        .replace(/(?:\/|~\/)(?:Users|home|tmp|var|mnt|storage|sdcard|Volumes|private|media)\/[^\r\n<>"|]+/g, "[local path removed]")
        .replace(/\/(?:[\w.-]+\/)+[^\s<>"|]*/g, "[local path removed]")
        .replace(/\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b/gi, "[email removed]")
        .replace(/\b(?:\d{1,3}\.){3}\d{1,3}\b/g, "[IP removed]")
        .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, "");
}
function browserFamily(ua) {
    for (const [pattern, name] of [[/Firefox\/(\d+)/, "Firefox"], [/Edg\/(\d+)/, "Edge"],
        [/Chrome\/(\d+)/, "Chromium"], [/Version\/(\d+).*Safari\//, "Safari"]]) {
        const match = ua.match(pattern);
        if (match) return `${name} ${match[1].slice(0, 4)}`;
    }
    return "Other / unknown";
}
function platformFamily(ua) {
    if (/iPhone|iPad|iPod|Macintosh.*Mobile/.test(ua)) return "iOS / iPadOS";
    if (/Android/.test(ua)) return "Android";
    if (/Windows/.test(ua)) return "Windows";
    if (/Macintosh/.test(ua)) return "macOS";
    if (/Linux/.test(ua)) return "Linux";
    return "Other / unknown";
}
export function buildSupportReport(description, category, context = {}, failures = [], recovery = {}) {
    const boolean = (value) => value === true ? "yes" : "no / unavailable";
    const revision = new URL(import.meta.url).searchParams.get("v");
    const lines = ["SlimeyBeats-EZ-Master · private browser support report", "Report format: 1 · Web Studio v0.3 beta",
        `Runtime build: ${/^[0-9a-f]{16}$/.test(revision || "") ? revision : "unversioned development copy"}`,
        `Area: ${CATEGORIES.has(category) ? category : "general"}`,
        `Browser family: ${browserFamily(navigator.userAgent)}`,
        `Platform family: ${platformFamily(navigator.userAgent)}`,
        `Secure context: ${boolean(globalThis.isSecureContext)}`,
        `WebAssembly available: ${boolean(typeof WebAssembly === "object")}`,
        `Web Audio available: ${boolean(!!(globalThis.AudioContext || globalThis.webkitAudioContext))}`,
        `AudioWorklet available: ${boolean(!!globalThis.AudioWorkletNode)}`,
        `Engine initialized: ${boolean(context.engineReady)}`,
        `Mastering audio loaded: ${boolean(context.masterLoaded)}`,
        `Master preview available: ${boolean(context.previewReady)}`,
        `Studio audio loaded: ${boolean(context.studioLoaded)}`,
        `Stem count: ${Number.isSafeInteger(context.stems) && context.stems >= 0 ? Math.min(context.stems, 10000) : "unknown"}`,
        `Audio job running: ${boolean(context.busy)}`,
        `Local recovery enabled: ${boolean(recovery.enabled)}`,
        `Recovery paused: ${boolean(recovery.paused)}`,
        `Observed event categories: ${[...new Set(failures.filter((f) => FAILURES.has(f)))].slice(0, 4).join(", ") || "none recorded"}`,
        "", "User description (review for personal details):", redactDescription(description) || "No description provided.", "",
        "Technical context excludes audio, filenames, paths, song names, profiles, raw logs, stack traces and full browser identification.",
        "Description filtering catches some obvious paths, links and contact details; it cannot guarantee removal of all personal information.",
        "Nothing was uploaded. Sharing this text is your choice."];
    return lines.join("\n");
}
