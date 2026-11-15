// Header validation only: exported audio is never decoded or reprocessed here.
export function waveInfo(bytes) {
    if (!(bytes instanceof Uint8Array) || bytes.length < 44) throw new Error("Incomplete exported WAV");
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const tag = (offset) => String.fromCharCode(...bytes.subarray(offset, offset + 4));
    if (tag(0) !== "RIFF" || tag(8) !== "WAVE" || view.getUint32(4, true) + 8 !== bytes.length)
        throw new Error("Invalid exported RIFF WAV length");
    let format, dataSize;
    for (let offset = 12, chunks = 0; offset < bytes.length; chunks++) {
        if (chunks >= 4096 || offset + 8 > bytes.length) throw new Error("Invalid exported WAV chunks");
        const size = view.getUint32(offset + 4, true), body = offset + 8, end = body + size;
        // Hound's current writer omits the final pad on odd-byte mono PCM24
        // data. Accept that exact terminal layout without accepting truncation.
        const terminalData = tag(offset) === "data" && end === bytes.length;
        if (end > bytes.length || (end + (size & 1) > bytes.length && !terminalData))
            throw new Error("Truncated exported WAV chunk");
        if (tag(offset) === "fmt ") {
            if (format || size < 16) throw new Error("Invalid exported WAV format");
            let encoding = view.getUint16(body, true);
            if (encoding === 0xfffe) {
                const guidTail = [0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];
                if (size < 40 || view.getUint16(body + 16, true) < 22 ||
                    guidTail.some((byte, i) => bytes[body + 26 + i] !== byte))
                    throw new Error("Unsupported extensible WAV format");
                encoding = view.getUint16(body + 24, true);
            }
            const channels = view.getUint16(body + 2, true), sample_rate = view.getUint32(body + 4, true);
            const block = view.getUint16(body + 12, true), bits = view.getUint16(body + 14, true);
            if (![1, 2].includes(channels) || sample_rate < 8000 || sample_rate > 192000 ||
                !((encoding === 1 && [16, 24, 32].includes(bits)) || (encoding === 3 && bits === 32)) ||
                block !== channels * bits / 8 || view.getUint32(body + 8, true) !== sample_rate * block)
                throw new Error("Unsupported exported WAV layout");
            format = { channels, sample_rate, block, bits, encoding };
        } else if (tag(offset) === "data") {
            if (dataSize !== undefined) throw new Error("Multiple exported WAV data chunks");
            dataSize = size;
        }
        offset = end + (terminalData ? 0 : size & 1);
    }
    if (!format || !dataSize || dataSize % format.block) throw new Error("Invalid exported WAV frames");
    return { ...format, frames: dataSize / format.block };
}
export function audioEntry(audio, base, allowOriginal = false) {
    if (!(audio?.bytes instanceof Uint8Array) || !audio.bytes.length || typeof audio.name !== "string")
        throw new Error("Export audio requires a filename and nonempty Uint8Array bytes");
    const extension = audio.name.match(/\.([a-z0-9]+)$/i)?.[1].toLowerCase();
    if (!(allowOriginal ? ["wav", "mp3", "flac", "ogg", "m4a", "aac"] : ["wav", "mp3"]).includes(extension))
        throw new Error("Choose a supported exported audio format");
    if (extension === "wav") waveInfo(audio.bytes);
    return { name: `${base}.${extension}`, bytes: audio.bytes };
}
