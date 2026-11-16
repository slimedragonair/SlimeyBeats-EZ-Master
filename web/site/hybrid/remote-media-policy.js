// Media addresses contain only opaque resource IDs. Credentials are requested
// from the controlling tab for each fetch and never put in URLs or storage.
const ID = /^[A-Za-z0-9_-]{1,128}$/;
export function remoteMediaPath(project, id, kind = "preview") {
    if (!ID.test(project) || !ID.test(id) || !["preview", "audio", "artifact"].includes(kind))
        throw new Error("Invalid server media identity.");
    return kind === "artifact" ? `/v2/projects/${project}/artifacts/${id}`
        : `/v2/projects/${project}/assets/${id}/${kind}`;
}
export function parseMediaUrl(url, scope) {
    let address, root;
    try { address = new URL(url); root = new URL(scope); }
    catch { return null; }
    if (address.origin !== root.origin || address.search || address.hash) return null;
    const prefix = `${root.pathname}remote-media/`;
    if (!address.pathname.startsWith(prefix)) return null;
    const parts = address.pathname.slice(prefix.length).split("/");
    if (parts.length !== 3) return null;
    const [project, kind, id] = parts;
    try { return { project, kind, id, path: remoteMediaPath(project, id, kind) }; }
    catch { return null; }
}
export function validByteRange(value) {
    return value === null || /^bytes=(?:\d+-\d*|-\d+)$/.test(value);
}
