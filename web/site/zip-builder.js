// Stored ZIP32 files, assembled as Blob parts to avoid a second full-size buffer.
// Layout follows PKWARE APPNOTE sections 4.3.7, 4.3.12 and 4.3.16.
// Primary specification: https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT
export const ZIP_LIMITS = Object.freeze({
    maxEntries: 4096,
    maxEntryBytes: 400 * 1024 * 1024,
    maxArchiveBytes: 512 * 1024 * 1024,
});
const ZIP32_MAX = 0xfffffffe;
const encoder = new TextEncoder();
const table = Uint32Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let i = 0; i < 8; i++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
});
function crc32(bytes) {
    let c = 0xffffffff;
    for (let i = 0; i < bytes.length; i++) c = table[(c ^ bytes[i]) & 255] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
}
export function safeZipName(name) {
    if (typeof name !== "string" || !/^[a-z0-9][a-z0-9_.-]{0,119}$/i.test(name) ||
        name.includes("..") || name.endsWith(".") ||
        /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name))
        throw new Error("ZIP requires safe flat filenames of at most 120 ASCII characters");
    return name;
}
function limitsFor(overrides) {
    const limits = { ...ZIP_LIMITS, ...overrides };
    for (const key of Object.keys(limits))
        if (!(key in ZIP_LIMITS) || !Number.isSafeInteger(limits[key]) || limits[key] < 0 ||
            limits[key] > ZIP_LIMITS[key]) throw new Error("Invalid ZIP export limit");
    return limits;
}
function checkCount(files, limits) {
    if (!Array.isArray(files) || files.length === 0) throw new Error("ZIP requires at least one file");
    if (files.length >= 0xffff) throw new Error("ZIP32 entry count requires ZIP64");
    if (files.length > limits.maxEntries) throw new Error("ZIP entry count exceeds the browser export budget");
}
// Size-only planning permits large-limit regressions without allocating audio.
export function planZip(files, overrides = {}) {
    const limits = limitsFor(overrides);
    checkCount(files, limits);
    const names = new Set();
    let payload = 0, centralSize = 0;
    const entries = files.map((file) => {
        const name = safeZipName(file?.name), key = name.toLowerCase();
        if (names.has(key)) throw new Error("ZIP filenames must be unique, including case");
        names.add(key);
        const size = file.size;
        if (!Number.isSafeInteger(size) || size < 0 || size > ZIP32_MAX)
            throw new Error("Invalid entry size or ZIP32 entry requires ZIP64");
        const nameBytes = encoder.encode(name), offset = payload;
        payload += 30 + nameBytes.length + size;
        centralSize += 46 + nameBytes.length;
        return { name, size, nameBytes, offset };
    });
    const size = payload + centralSize + 22;
    if (payload > ZIP32_MAX || centralSize > ZIP32_MAX || size > ZIP32_MAX)
        throw new Error("ZIP32 archive requires ZIP64");
    if (entries.some((file) => file.size > limits.maxEntryBytes))
        throw new Error("ZIP entry exceeds the browser export budget");
    if (size > limits.maxArchiveBytes) throw new Error("ZIP exceeds the browser export budget");
    return { entries, payload, centralSize, size };
}
export function buildZip(files, overrides = {}) {
    checkCount(files, limitsFor(overrides));
    if (files.some((file) => !(file?.bytes instanceof Uint8Array)))
        throw new Error("ZIP entries must provide Uint8Array bytes");
    const plan = planZip(files.map((file) => ({ name: file.name, size: file.bytes.byteLength })), overrides);
    // All validation precedes CRC scans, header allocation and Blob construction.
    const parts = [], central = [];
    for (let i = 0; i < plan.entries.length; i++) {
        const file = plan.entries[i], bytes = files[i].bytes, crc = crc32(bytes);
        const local = new Uint8Array(30 + file.nameBytes.length), lv = new DataView(local.buffer);
        lv.setUint32(0, 0x04034b50, true);
        lv.setUint16(4, 20, true);
        lv.setUint16(6, 0x800, true);
        lv.setUint16(12, 33, true); // 1980-01-01; deterministic, valid DOS date.
        lv.setUint32(14, crc, true);
        lv.setUint32(18, file.size, true);
        lv.setUint32(22, file.size, true);
        lv.setUint16(26, file.nameBytes.length, true);
        local.set(file.nameBytes, 30);
        parts.push(local, bytes);
        const directory = new Uint8Array(46 + file.nameBytes.length), cv = new DataView(directory.buffer);
        cv.setUint32(0, 0x02014b50, true);
        cv.setUint16(4, 20, true);
        cv.setUint16(6, 20, true);
        cv.setUint16(8, 0x800, true);
        cv.setUint16(14, 33, true);
        cv.setUint32(16, crc, true);
        cv.setUint32(20, file.size, true);
        cv.setUint32(24, file.size, true);
        cv.setUint16(28, file.nameBytes.length, true);
        cv.setUint32(42, file.offset, true);
        directory.set(file.nameBytes, 46);
        central.push(directory);
    }
    const end = new Uint8Array(22), view = new DataView(end.buffer);
    view.setUint32(0, 0x06054b50, true);
    view.setUint16(8, plan.entries.length, true);
    view.setUint16(10, plan.entries.length, true);
    view.setUint32(12, plan.centralSize, true);
    view.setUint32(16, plan.payload, true);
    return new Blob([...parts, ...central, end], { type: "application/zip" });
}
