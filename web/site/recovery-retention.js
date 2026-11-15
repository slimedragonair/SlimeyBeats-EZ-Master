export const MAX_RECOVERY_TOTAL = 256 * 1024 * 1024;
export const MAX_RECOVERY_POINTS = 12;
export const POINTS_PER_SESSION = 3;
export function retentionPlan(rows, owner, incomingBytes) {
    if (rows.some((r) => !Number.isSafeInteger(r.bytes) || r.bytes < 0))
        throw new Error("Recovery is paused: the saved point index could not be validated. Existing points were kept.");
    const owned = rows.filter((r) => r.owner === owner).sort((a, b) => b.sequence - a.sequence);
    const remove = new Set(owned.slice(POINTS_PER_SESSION - 1).map((r) => r.id));
    const fits = () => {
        const kept = rows.filter((r) => !remove.has(r.id));
        return kept.length < MAX_RECOVERY_POINTS && kept.reduce((n, r) => n + r.bytes, incomingBytes) <= MAX_RECOVERY_TOTAL;
    };
    // Thin older points from this session under pressure. Keep its previous
    // latest-good point until the new point and deletions commit together.
    for (const older of owned.slice(1).reverse()) {
        if (fits()) break;
        remove.add(older.id);
    }
    if (!fits())
        throw new Error("Recovery is paused: its 256 MiB / 12-point device budget is full. Previous points are kept. Download a recovery copy or delete saved recovery data; editing limits are unchanged.");
    return remove;
}
