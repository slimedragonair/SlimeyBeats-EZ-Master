const MIB = 1024 * 1024;

function duration(seconds) {
    const rounded = Math.ceil(seconds);
    return `${Math.floor(rounded / 60)}:${String(rounded % 60).padStart(2, "0")}`;
}

export function validateFile(file, limits) {
    if (!limits)
        throw new Error(
            "The audio engine is still starting. Try again in a moment.",
        );
    if (file.size > limits.max_file_bytes)
        throw new Error(
            `This file is ${(file.size / MIB).toFixed(1)} MiB. The web preview accepts files up to ${limits.max_file_bytes / MIB} MiB. Try a smaller file or use the desktop app.`,
        );
}

export function validateDecoded(buffer, limits) {
    if (buffer.numberOfChannels < 1 || buffer.numberOfChannels > 2)
        throw new Error(
            "Use a mono or stereo track. Downmix surround audio explicitly first.",
        );
    if (buffer.duration > limits.max_seconds)
        throw new Error(
            `This track is ${duration(buffer.duration)} long. The web preview supports up to ${duration(limits.max_seconds)} per track. Use a shorter track or the desktop app.`,
        );
    if (buffer.length * buffer.numberOfChannels * 4 > limits.max_decoded_bytes)
        throw new Error(
            `This track exceeds the ${limits.max_decoded_bytes / MIB} MiB decoded-audio limit. Try a smaller file or the desktop app.`,
        );
    for (let ch = 0; ch < buffer.numberOfChannels; ch++)
        if (!buffer.getChannelData(ch).every(Number.isFinite))
            throw new Error("Decoded track contains invalid audio samples.");
}

export function limitsSummary(limits) {
    return `Up to ${duration(limits.max_seconds)} per track · mono/stereo · ${limits.max_file_bytes / MIB} MiB file limit. Available device memory may limit large tracks.`;
}
