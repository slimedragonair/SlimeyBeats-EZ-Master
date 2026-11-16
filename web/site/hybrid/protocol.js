const STATES = new Set(["uploading", "queued", "running", "completed", "failed", "cancelled"]);
export const TERMINAL_STATES = new Set(["completed", "failed", "cancelled"]);
export const CLIENT_CHUNK_BYTES = 4 * 1024 * 1024;
const integer = (value, min = 1) => Number.isSafeInteger(value) && value >= min;
const optionalInteger = (value, key) => !Object.hasOwn(value, key) || integer(value[key]);
const text = (value, max = 256) => typeof value === "string" && value.length > 0 && value.length <= max;
const id = (value) => text(value, 128) && /^[A-Za-z0-9_-]+$/.test(value);
const ARTIFACT_EXTENSIONS = new Map([
    ["audio/wav", "wav"], ["audio/x-wav", "wav"], ["audio/wave", "wav"],
    ["audio/mpeg", "mp3"], ["application/json", "json"], ["text/plain", "txt"],
]);

export function capabilities(value) {
    const limits = value?.limits, privacy = value?.privacy;
    if (value?.api_version !== 1 || !Array.isArray(value.operations) || !value.operations.includes("master") ||
        !Array.isArray(value.formats) || !value.formats.some((item) => ["wav24", "mp3"].includes(item)) ||
        !Array.isArray(value.profiles) || !value.profiles.length || value.profiles.length > 100 ||
        value.profiles.some((p) => !text(p?.id) || !text(p?.label)) ||
        new Set(value.profiles.map((p) => p.id)).size !== value.profiles.length ||
        !integer(limits?.max_upload_bytes) || !integer(limits?.chunk_bytes) || !integer(limits?.max_jobs) ||
        !optionalInteger(limits, "max_audio_seconds") || !optionalInteger(limits, "max_decoded_bytes") ||
        !integer(limits?.retention_seconds) || privacy?.temporary_storage !== true ||
        !integer(privacy?.retention_seconds) || limits.retention_seconds !== privacy.retention_seconds)
        throw new Error("The server did not provide compatible mastering limits and a consistent temporary-storage policy. Upload is disabled.");
    return value;
}

export function snapshot(value, expectedId) {
    if (!id(value?.id) || (expectedId && value.id !== expectedId) || !STATES.has(value.state) ||
        !integer(value.total_bytes) || !integer(value.uploaded_bytes, 0) || value.uploaded_bytes > value.total_bytes ||
        !Number.isFinite(value.progress) || value.progress < 0 || value.progress > 1 ||
        typeof value.message !== "string" || value.message.length > 2000 ||
        typeof value.expires_at !== "string" || !Number.isFinite(Date.parse(value.expires_at)) ||
        !Array.isArray(value.artifacts) || value.artifacts.length > 20 ||
        value.artifacts.some((a) => !id(a?.id) || !text(a?.name) || !integer(a?.bytes, 0) || !text(a?.content_type)))
        throw new Error("The server returned an invalid job snapshot. Check the job before retrying; server deletion is not confirmed.");
    return value;
}

// Use a known extension and strip paths/control characters from both metadata
// and Content-Disposition. The downloaded bytes never become a bearer URL.
export function artifactExtension(contentType) {
    const type = typeof contentType === "string" ? contentType.toLowerCase().split(";")[0].trim() : "";
    const extension = ARTIFACT_EXTENSIONS.get(type);
    if (!extension) throw new Error("This artifact type is not supported for download.");
    return extension;
}

export function artifactFilename(artifact, disposition = "") {
    const extension = artifactExtension(artifact.content_type);
    let name = artifact.name;
    const encoded = /filename\*=UTF-8''([^;]+)/i.exec(disposition);
    const plain = /filename=(?:"([^"]*)"|([^;]*))/i.exec(disposition);
    try { name = encoded ? decodeURIComponent(encoded[1].trim()) : plain ? (plain[1] ?? plain[2]).trim() : name; }
    catch { /* Malformed header: fall back to the validated artifact name. */ }
    const leaf = name.split(/[\\/]/).pop().replace(/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/g, "");
    const stem = leaf.replace(/\.[^.]*$/, "").replace(/[^\p{L}\p{N}_ -]/gu, "_").trim().slice(0, 100);
    return `server-${stem || "master"}.${extension}`;
}

export function bytesLabel(bytes) {
    if (bytes < 1024) return `${bytes} bytes`;
    return `${(bytes / 1024 ** 2).toLocaleString(undefined, { maximumFractionDigits: 2 })} MiB (${bytes.toLocaleString()} bytes)`;
}
