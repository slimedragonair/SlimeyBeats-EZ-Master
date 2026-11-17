import { buildProjectZip } from "./zip-builder.js";

const check = signal => { if (signal?.aborted) throw new DOMException("Cancelled", "AbortError"); };
export async function saveArrangementProject(client, snapshot, { expectedRevision = snapshot.revision, signal } = {}) {
    check(signal);
    if (snapshot.revision !== expectedRevision) throw new Error("Project revision changed before save");
    const manifest = await client.request("project-manifest", { expectedRevision }, { signal });
    const files = [{ name: "arrangement.json", bytes: new TextEncoder().encode(JSON.stringify(manifest)) }];
    const names = new Set();
    for (const source of manifest.sources) {
        check(signal);
        if (names.has(source.file)) continue;
        names.add(source.file);
        const bytes = await client.request("asset-wav", { asset_id: source.id }, { signal });
        files.push({ name: source.file, bytes });
    }
    const current = await client.request("read", {}, { signal });
    if (current.epoch !== snapshot.epoch || current.revision !== expectedRevision)
        throw new Error("The arrangement changed during save. Save the current revision again.");
    check(signal);
    const bytes = new Uint8Array(await buildProjectZip(files).arrayBuffer());
    check(signal);
    return { bytes, name: "Studio-arrangement.zip", type: "application/zip" };
}

export async function openArrangementProject(client, input, expectedRevision, { signal } = {}) {
    check(signal);
    const size = input instanceof Uint8Array ? input.byteLength : input?.size;
    if (!Number.isSafeInteger(size) || size > 256 * 1024 * 1024 || size < 22) throw new Error("Choose a project ZIP up to 256 MiB");
    // Copy caller-owned bytes so failure never detaches its retained project.
    const bytes = input instanceof Uint8Array ? input.slice() : new Uint8Array(await input.arrayBuffer());
    check(signal);
    return client.request("open-project", { expectedRevision, bytes, epoch: crypto.randomUUID() }, { signal, transfer: [bytes.buffer] });
}
