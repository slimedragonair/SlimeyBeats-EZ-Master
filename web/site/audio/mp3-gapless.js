// MPEG Layer III Xing/Info + LAME timing. The 529-sample synthesis delay is
// added to the encoder priming, as in FFmpeg's MP3 demuxer. No audio is decoded here.
const DECODER_DELAY = 529;
const RATES = [44100, 48000, 32000];
const BITRATES = [
    [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
    [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320],
];
const ascii = (bytes, start, length) =>
    String.fromCharCode(...bytes.subarray(start, start + length));

export function readMp3Timing(encoded) {
    const bytes = new Uint8Array(encoded), view = new DataView(encoded);
    let start = 0;
    if (bytes.length >= 10 && ascii(bytes, 0, 3) === "ID3") {
        if ([...bytes.subarray(6, 10)].some((v) => v & 128)) return null;
        const tagSize = bytes[6] * 2097152 + bytes[7] * 16384 + bytes[8] * 128 + bytes[9];
        start = 10 + tagSize + (bytes[3] === 4 && (bytes[5] & 16) ? 10 : 0);
    }
    if (start + 4 > bytes.length || bytes[start] !== 255 || (bytes[start + 1] & 224) !== 224)
        return null;
    const version = (bytes[start + 1] >> 3) & 3;
    const layer = (bytes[start + 1] >> 1) & 3;
    const rateIndex = (bytes[start + 2] >> 2) & 3;
    const bitrateIndex = bytes[start + 2] >> 4;
    if (version === 1 || layer !== 1 || rateIndex === 3 || !bitrateIndex || bitrateIndex === 15)
        return null;
    const mpeg1 = version === 3;
    const rate = RATES[rateIndex] / (mpeg1 ? 1 : version === 2 ? 2 : 4);
    const mono = (bytes[start + 3] >> 6) === 3;
    const sideInfo = mpeg1 ? (mono ? 17 : 32) : (mono ? 9 : 17);
    const frameBytes = Math.floor((mpeg1 ? 144000 : 72000) * BITRATES[+mpeg1][bitrateIndex] / rate)
        + ((bytes[start + 2] >> 1) & 1);
    const end = Math.min(bytes.length, start + frameBytes);
    // Some encoders include a CRC before the side information.
    let tag = start + 4 + sideInfo;
    if (!["Xing", "Info"].includes(ascii(bytes, tag, 4))) {
        tag += 2;
        if (!["Xing", "Info"].includes(ascii(bytes, tag, 4))) return null;
    }
    if (tag + 12 > end) return null;
    const flags = view.getUint32(tag + 4);
    if (!(flags & 1) || flags & ~15) return null;
    const frameCount = view.getUint32(tag + 8);
    const encoder = tag + 8 + 4 + (flags & 2 ? 4 : 0) + (flags & 4 ? 100 : 0) + (flags & 8 ? 4 : 0);
    if (encoder + 24 > end || !["LAME", "Lavc", "Lavf"].includes(ascii(bytes, encoder, 4)))
        return null;
    const delay = bytes[encoder + 21] * 16 + (bytes[encoder + 22] >> 4);
    const padding = (bytes[encoder + 22] & 15) * 256 + bytes[encoder + 23];
    const encodedSamples = frameCount * (mpeg1 ? 1152 : 576);
    const sourceSamples = encodedSamples - delay - padding;
    if (!frameCount || sourceSamples <= 0 || padding < DECODER_DELAY) return null;
    return { rate, encodedSamples, sourceSamples, leadingSamples: delay + DECODER_DELAY };
}

export function normalizeMp3Timing(context, buffer, timing, maxSeconds) {
    if (!timing || timing.sourceSamples / timing.rate > maxSeconds) return buffer;
    const ratio = buffer.sampleRate / timing.rate;
    const frames = Math.round(timing.sourceSamples * ratio);
    if (buffer.length === frames || buffer.length === frames - 1) return buffer;
    let first = 0;
    if (buffer.length !== frames + 1) {
        const rawFrames = Math.round(timing.encodedSamples * ratio);
        // Correct only a recognized padded layout, never guess by extension or
        // trim an arbitrarily overlong recording to fit the duration limit.
        if (Math.abs(buffer.length - rawFrames) > 1) return buffer;
        first = Math.round(timing.leadingSamples * ratio);
    }
    if (frames <= 0 || first + frames > buffer.length) return buffer;
    const result = context.createBuffer(buffer.numberOfChannels, frames, buffer.sampleRate);
    for (let ch = 0; ch < buffer.numberOfChannels; ch++)
        result.copyToChannel(buffer.getChannelData(ch).subarray(first, first + frames), ch);
    return result;
}
