export const LOCAL_ARRANGEMENT_PCM_BYTES = 128 * 1024 ** 2;

export function freshArrangement(snapshot) {
    return !!snapshot?.arrangement && !snapshot.arrangement.lanes.length && !snapshot.arrangement.eq.length &&
        !snapshot.assets.length && !snapshot.history?.can_undo && !snapshot.history?.can_redo;
}

export function arrangementAssetBytes(snapshot) {
    return (snapshot?.assets || []).reduce((total, asset) => total + asset.frames * asset.channels * 4, 0);
}

// Serialization is explicit at Arrange tracks, never at the engine switch.
// The target must still be fresh when the immutable canonical file is ready.
export async function continueLocalArrangement(local, server, current) {
    if (!local?.loaded || !freshArrangement(server.state())) return false;
    const captured = await local.captureProject();
    if (!current() || !freshArrangement(server.state())) throw new Error("The workspace changed before transfer. Both projects were kept.");
    await server.openProject(captured.file, captured.ui);
    return true;
}
