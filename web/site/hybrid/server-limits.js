import { bytesLabel } from "./protocol.js";

// The first server preview normalizes MP3 input to 48 kHz stereo. Its decoded
// memory guard budgets four bytes per sample, independently of upload size.
const MP3_RATE = 48000;
const MP3_CHANNELS = 2;
const DECODED_SAMPLE_BYTES = 4;

function approximateTime(seconds) {
    const whole = Math.floor(seconds);
    return `${Math.floor(whole / 60)} min ${whole % 60} sec`;
}

export function serverStoragePolicy(caps) {
    const { limits, privacy } = caps;
    const hasDuration = Object.hasOwn(limits, "max_audio_seconds");
    const hasDecoded = Object.hasOwn(limits, "max_decoded_bytes");
    const policy = [
        "Temporary server storage",
        `retention ${privacy.retention_seconds.toLocaleString()} seconds`,
        `maximum upload ${bytesLabel(limits.max_upload_bytes)}`,
    ];
    if (hasDuration) policy.push(`configured audio duration limit ${limits.max_audio_seconds.toLocaleString()} seconds`);
    if (hasDecoded) policy.push(`decoded PCM budget ${bytesLabel(limits.max_decoded_bytes)}`);
    policy.push(`${limits.max_jobs} job(s)`);
    let mp3 = "This server preview normalizes MP3 input to 48 kHz stereo; its decoded memory budget can impose a shorter duration than its configured time limit.";
    if (hasDecoded) {
        const memorySeconds = Math.floor(limits.max_decoded_bytes / (MP3_CHANNELS * DECODED_SAMPLE_BYTES)) / MP3_RATE;
        const bound = hasDuration ? Math.min(memorySeconds, limits.max_audio_seconds) : memorySeconds;
        mp3 += ` The MP3 duration bound from the reported ${hasDuration ? "duration and decoded PCM limits together" : "decoded PCM budget alone"} is about ${approximateTime(bound)}.`;
    }
    return `${policy.join(" · ")}. The server reports an exact expiry after job creation. ${mp3} ` +
        "Upload size and other server resource limits may reduce what succeeds; this is not a universal track maximum. WAV capacity depends on its sample rate and channels. Local browser mastering has a separate 15-minute limit.";
}
