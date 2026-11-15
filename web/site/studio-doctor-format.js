export function issueTime(issue, rate) {
    const duration = issue.end_seconds - issue.start_seconds, precision = duration < 0.01 ? 6 : 2;
    const stamp = seconds => `${Math.floor(seconds / 60)}:${(seconds % 60).toFixed(precision).padStart(precision + 3, "0")}`;
    const frames = Math.max(1, Math.round(duration * rate));
    const length = duration < 0.01
        ? `${frames} frame${frames === 1 ? "" : "s"} per channel · ${(duration * 1000).toFixed(3)} ms`
        : `${duration.toFixed(2)} s`;
    return `${stamp(issue.start_seconds)}–${stamp(issue.end_seconds)} (${length})`;
}
