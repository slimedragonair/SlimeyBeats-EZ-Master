// A saved master and the current direction can have different reference
// revisions even when both settings use the same "browser-reference" locator.
function signature(json) {
    const value = JSON.parse(json);
    if (!value || typeof value !== "object" || Array.isArray(value) ||
        !Number.isSafeInteger(value.reference_revision) || value.reference_revision < 0)
        throw new Error("Invalid recovery preview identity.");
    return value;
}

async function legacyReferencesDiffer(value) {
    const reference = value.reference || null, baseline = value.baselineReference || null;
    if (!reference || !baseline) return reference !== baseline;
    if (!(reference instanceof Blob) || !(baseline instanceof Blob))
        throw new Error("Invalid recovery reference.");
    if (reference.size !== baseline.size) return true;
    // Older files saved both references but no current signature. Compare exact
    // encoded bytes in bounded chunks, including separately restored Blobs.
    const chunkBytes = 64 * 1024;
    for (let start = 0; start < reference.size; start += chunkBytes) {
        const [left, right] = await Promise.all([
            reference.slice(start, start + chunkBytes).arrayBuffer(),
            baseline.slice(start, start + chunkBytes).arrayBuffer(),
        ]);
        const a = new Uint8Array(left), b = new Uint8Array(right);
        if (a.some((byte, i) => byte !== b[i])) return true;
    }
    return false;
}

export async function recoverMasterIdentity(value) {
    const saved = signature(value.signature);
    const referenceStale = value.currentSignature === undefined
        ? await legacyReferencesDiffer(value)
        : signature(value.currentSignature).reference_revision !== saved.reference_revision;
    return { signature: saved, referenceStale };
}

export function rebindMasterSignature(saved, currentReferenceRevision, referenceStale) {
    if (!Number.isSafeInteger(currentReferenceRevision) || currentReferenceRevision < Number(referenceStale) ||
        typeof referenceStale !== "boolean")
        throw new Error("Invalid recovery reference revision.");
    // Revisions belong to this document. Preserve a stale relationship while
    // leaving every saved mastering setting intact for the normal export guard.
    return JSON.stringify({ ...saved, reference_revision: currentReferenceRevision - Number(referenceStale) });
}
