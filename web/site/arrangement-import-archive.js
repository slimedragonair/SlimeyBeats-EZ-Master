// ZIPs stay in the normal WASM worker. Main-thread review retains descriptors;
// selected entries alone cross the worker boundary for decoding/import.
export async function openArrangementArchives(client, files, signal, heldBytes = 0) {
    const inputs = [], archives = []; let bytesHeld = 0, skipped = 0;
    try {
        for (const file of files) {
            if (signal?.aborted) throw new DOMException("Cancelled", "AbortError");
            if (!/\.zip$/i.test(file.name)) { inputs.push(file); continue; }
            bytesHeld += file.size;
            if (bytesHeld + heldBytes > 128 * 1024 * 1024) throw new Error("Pending arrangement ZIP reviews exceed 128 MiB; discard an earlier review first");
            const archive_id = crypto.randomUUID(), bytes = new Uint8Array(await file.arrayBuffer());
            if (signal?.aborted) throw new DOMException("Cancelled", "AbortError");
            const value = await client.request("archive-open", { archive_id, bytes }, { signal, transfer: [bytes.buffer] });
            archives.push(archive_id); skipped += value.skipped_files;
            for (const entry of value.files) inputs.push({
                name: entry.name.split("/").pop(), webkitRelativePath: `${archive_id}/${entry.name}`, size: entry.bytes,
                async arrayBuffer() { return (await client.request("archive-read", { archive_id, index: entry.index })).buffer; },
            });
        }
        return { inputs, archives, bytesHeld, skipped };
    } catch (error) {
        await Promise.all(archives.map(archive_id => client.request("archive-close", { archive_id })));
        throw error;
    }
}

export function closeArrangementArchives(client, archives) {
    return Promise.all(archives.map(archive_id => client.request("archive-close", { archive_id })));
}
