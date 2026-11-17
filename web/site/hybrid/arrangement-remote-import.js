import { groupFileVersions } from "../arrangement-import-review.js";

// Reviews retain file handles only. Selected uploads are admitted together by
// the arrangement actor, without using legacy import/history as scratch space.
export function createRemoteArrangementImport({ project, getSnapshot, commit }) {
    const reviews = new Map();
    return {
        reviewImport(files, { signal } = {}) {
            signal?.throwIfAborted();
            if ([...files].some(file => /\.zip$/i.test(file.name))) throw new Error("Server arrangement ZIP review is not available yet. Choose WAV or MP3 files.");
            const grouped = groupFileVersions(files, { max_file_bytes: project.caps.limits.max_upload_bytes });
            const review = { id: crypto.randomUUID(), parts: grouped.parts, skipped_files: grouped.skipped_files, notes: grouped.notes };
            reviews.set(review.id, { review: structuredClone(review), entries: grouped.entries });
            return structuredClone(review);
        },
        discardImport(review) { reviews.delete(review?.id); },
        async addImport(review, selection, { expectedRevision, signal } = {}) {
            const owned = reviews.get(review?.id), before = getSnapshot(); signal?.throwIfAborted();
            if (!owned || before.revision !== expectedRevision) throw new Error("The import review or arrangement revision changed. Review the files again.");
            if (!Array.isArray(selection) || !selection.length) throw new Error("Select at least one part.");
            const chosen = [], seen = new Set();
            for (const item of selection) {
                const part = owned.review.parts.find(part => part.id === item.part_id);
                const version = part?.versions.find(version => version.id === item.version_id);
                if (!version || seen.has(part.id) || !owned.entries.has(version.id)) throw new Error("Choose one owned file version per part.");
                seen.add(part.id); chosen.push({ name: part.name, file: owned.entries.get(version.id) });
            }
            const upload_ids = [], names = [];
            for (const item of chosen) {
                signal?.throwIfAborted();
                upload_ids.push(await project.upload(item.file, "original", signal)); names.push(item.name);
            }
            signal?.throwIfAborted();
            const result = await commit({ upload_ids, names }, { expectedRevision, epoch: before.epoch, signal });
            reviews.delete(review.id); return result;
        },
        clear() { reviews.clear(); },
    };
}
