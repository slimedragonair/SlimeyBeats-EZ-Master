import { artifactExtension, artifactFilename } from "./protocol.js";

export const BROWSER_DOWNLOAD_BUDGET_BYTES = 128 * 1024 * 1024;
export const DOWNLOAD_BUDGET_NOTICE = "Large WAV downloads may exceed this preview’s 128 MiB download budget; choose MP3 or desktop.";
const DOWNLOAD_BUFFER_BYTES = 1024 * 1024;

// Only fixed, client-authored messages from this class may bypass the API
// client's sanitization of unknown network/decoder errors.
export class ArtifactDownloadError extends Error {}

export function checkArtifactDownload(artifact) {
    if (!Number.isSafeInteger(artifact?.bytes) || artifact.bytes < 0)
        throw new ArtifactDownloadError("The artifact has an invalid declared size. No download was requested.");
    if (artifact.bytes > BROWSER_DOWNLOAD_BUDGET_BYTES)
        throw new ArtifactDownloadError(DOWNLOAD_BUDGET_NOTICE);
    try { artifactExtension(artifact.content_type); }
    catch { throw new ArtifactDownloadError("This artifact type is not supported for browser download."); }
}

export async function readArtifactDownload(response, artifact) {
    let reader;
    try {
        checkArtifactDownload(artifact);
        let actualType;
        try { actualType = artifactExtension(response.headers.get("Content-Type")); }
        catch { throw new ArtifactDownloadError("Download stopped: the server did not return a supported artifact Content-Type."); }
        if (actualType !== artifactExtension(artifact.content_type))
            throw new ArtifactDownloadError("Download stopped: the server Content-Type differs from this artifact's declared type.");
        if (!response.body?.getReader)
            throw new ArtifactDownloadError("Bounded download streaming is unavailable. Choose a supported browser or the desktop app.");
        reader = response.body.getReader();
        const parts = [];
        let received = 0;
        let buffer, used = 0;
        while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            if (!(value instanceof Uint8Array))
                throw new ArtifactDownloadError("Download stopped: invalid artifact stream.");
            if (value.byteLength > artifact.bytes - received || value.byteLength > BROWSER_DOWNLOAD_BUDGET_BYTES - received)
                throw new ArtifactDownloadError("Download cancelled: the server exceeded the declared artifact size or this preview's 128 MiB download budget. Choose MP3 or desktop.");
            // Merge small network chunks into bounded buffers, so a stream of
            // tiny chunks cannot grow an unbounded array of backing objects.
            for (let offset = 0; offset < value.byteLength;) {
                if (!buffer || used === buffer.byteLength) {
                    buffer = new Uint8Array(Math.min(DOWNLOAD_BUFFER_BYTES, artifact.bytes - received));
                    used = 0;
                    parts.push(buffer);
                }
                const count = Math.min(value.byteLength - offset, buffer.byteLength - used);
                buffer.set(value.subarray(offset, offset + count), used);
                used += count; offset += count; received += count;
            }
        }
        if (received !== artifact.bytes)
            throw new ArtifactDownloadError("Download incomplete: received size differs from the declared artifact size. Retry the download.");
        const name = artifactFilename(artifact, response.headers.get("Content-Disposition") || "");
        return { blob: new Blob(parts, { type: artifact.content_type }), name };
    } catch (error) {
        // Initiate cancellation without letting a stalled/failed cancellation
        // hide the original size, type or network error.
        try { (reader ? reader.cancel() : response.body?.cancel())?.catch(() => {}); }
        catch { /* Cancellation is best effort; the download error is reported. */ }
        throw error;
    } finally { reader?.releaseLock(); }
}
