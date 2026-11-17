// Pure filename/format review shared by local and remote adapters. Files remain
// owned inputs; no reads, uploads, decoding, DSP or inferred EQ targets occur.
export function groupFileVersions(files, { max_file_bytes = 128 * 1024 * 1024,
    newId = () => crypto.randomUUID() } = {}) {
    const groups = new Map(), entries = new Map(); let skipped = 0, charge = 0;
    for (const file of files) {
        const path = file.webkitRelativePath || file.name;
        if (typeof path !== "string") throw new Error("Choose named audio files");
        const format = path.match(/\.([^.]+)$/)?.[1].toLowerCase();
        if (!["wav", "mp3"].includes(format)) { skipped++; continue; }
        if (!Number.isSafeInteger(file.size) || file.size < 1 || file.size > max_file_bytes)
            throw new Error("Audio input exceeds this engine's file budget");
        charge += new TextEncoder().encode(path).length + 256;
        if (charge > 8 * 1024 * 1024) throw new Error("Import labels exceed the 8 MiB metadata budget");
        const filename = path.split(/[\\/]/).pop(), name = filename.replace(/\.[^.]+$/, "");
        if (!name.trim() || new TextEncoder().encode(name).length > 512 || /[\x00-\x1f]/.test(name))
            throw new Error("Audio labels must be bounded plain text");
        let folder = path.slice(0, path.length - filename.length).replace(/\\/g, "/");
        folder = folder.replace(new RegExp(`(?:^|/)${format}/$`, "i"), "/");
        const key = `${folder}\u0000${name.toLowerCase()}`;
        let part = groups.get(key);
        if (!part || part.versions.some(version => version.format === format)) {
            part = { id: newId(), name, versions: [] };
            groups.set(groups.has(key) ? `${key}:${part.id}` : key, part);
        }
        const version = { id: newId(), format, label: file.name };
        part.versions.push(version); entries.set(version.id, file);
    }
    const parts = [...groups.values()];
    if (!parts.length) throw new Error("Choose one or more WAV or MP3 files");
    return { parts, entries, skipped_files: skipped,
        notes: skipped ? ["Only WAV and MP3 inputs are currently supported by this arrangement adapter."] : [] };
}
