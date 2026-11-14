// A bounded, uncompressed ZIP keeps raw stems aligned without another dependency.
const table = Uint32Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let i = 0; i < 8; i++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
});
function crc32(bytes) {
    let c = 0xffffffff;
    for (const b of bytes) c = table[(c ^ b) & 255] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
}
export function stemZip(files, manifest) {
    if (files.length < 2) throw new Error("Export at least two stems");
    const encoder = new TextEncoder();
    if (
        new Set(files.map((file) => file.name)).size !== files.length ||
        files.some((file) => !/^[a-z0-9_]+\.wav$/i.test(file.name))
    )
        throw new Error("Stem ZIP requires unique, simple WAV filenames");
    const contents = [...files];
    if (manifest !== undefined) {
        if (
            manifest?.format !== "wav32f" ||
            !Array.isArray(manifest.stems) ||
            manifest.stems.length !== files.length ||
            new Set(manifest.stems.map((stem) => stem.file)).size !==
                files.length ||
            manifest.stems.some(
                (stem) =>
                    !files.some((file) => file.name === stem.file) ||
                    typeof stem.kind !== "string" ||
                    typeof stem.display_name !== "string",
            )
        )
            throw new Error("Stem ZIP manifest does not match its audio files");
        const bytes = encoder.encode(JSON.stringify(manifest, null, 2));
        if (bytes.length > 1024 * 1024)
            throw new Error("Stem ZIP manifest is too large");
        contents.push({ name: "stems.json", bytes });
    }
    if (contents.length >= 0xffff)
        throw new Error(
            "This many ZIP entries requires ZIP64. Export a smaller set.",
        );
    const entries = contents.map((file) => ({
        ...file,
        nameBytes: encoder.encode(file.name),
        crc: crc32(file.bytes),
    }));
    const payload = entries.reduce(
        (n, file) => n + 30 + file.nameBytes.length + file.bytes.length,
        0,
    );
    const centralSize = entries.reduce(
        (n, file) => n + 46 + file.nameBytes.length,
        0,
    );
    if (payload > 140 * 1024 * 1024)
        throw new Error("Stem ZIP exceeds the browser export budget");
    const output = new Uint8Array(payload + centralSize + 22),
        view = new DataView(output.buffer);
    let offset = 0;
    for (const file of entries) {
        file.offset = offset;
        view.setUint32(offset, 0x04034b50, true);
        view.setUint16(offset + 4, 20, true);
        view.setUint16(offset + 6, 0x800, true);
        view.setUint32(offset + 14, file.crc, true);
        view.setUint32(offset + 18, file.bytes.length, true);
        view.setUint32(offset + 22, file.bytes.length, true);
        view.setUint16(offset + 26, file.nameBytes.length, true);
        output.set(file.nameBytes, offset + 30);
        output.set(file.bytes, offset + 30 + file.nameBytes.length);
        offset += 30 + file.nameBytes.length + file.bytes.length;
    }
    for (const file of entries) {
        view.setUint32(offset, 0x02014b50, true);
        view.setUint16(offset + 4, 20, true);
        view.setUint16(offset + 6, 20, true);
        view.setUint16(offset + 8, 0x800, true);
        view.setUint32(offset + 16, file.crc, true);
        view.setUint32(offset + 20, file.bytes.length, true);
        view.setUint32(offset + 24, file.bytes.length, true);
        view.setUint16(offset + 28, file.nameBytes.length, true);
        view.setUint32(offset + 42, file.offset, true);
        output.set(file.nameBytes, offset + 46);
        offset += 46 + file.nameBytes.length;
    }
    view.setUint32(offset, 0x06054b50, true);
    view.setUint16(offset + 8, entries.length, true);
    view.setUint16(offset + 10, entries.length, true);
    view.setUint32(offset + 12, centralSize, true);
    view.setUint32(offset + 16, payload, true);
    return output;
}
