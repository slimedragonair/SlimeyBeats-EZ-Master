import { groupFileVersions } from "../arrangement-import-review.js";

// Reviews retain file handles only. Selected uploads are admitted together by
// the arrangement actor, without using legacy import/history as scratch space.
export function createRemoteArrangementImport({ project, getSnapshot, commit, job, publish, discardArchive }) {
    const reviews = new Map();
    return {
        async reviewImport(files, { signal } = {}) {
            signal?.throwIfAborted();
            files = [...files];
            if (files.some(file => /\.zip$/i.test(file.name))) {
                if (!getSnapshot().capabilities.import_zip) throw new Error("Server arrangement ZIP review is not available yet. Choose WAV or MP3 files.");
                if (files.length !== 1) throw new Error("Review one ZIP at a time. Add loose WAV or MP3 files separately.");
                const archiveLimit = project.caps.arrangement.max_archive_bytes;
                if (Number.isSafeInteger(archiveLimit) && files[0].size > archiveLimit)
                    throw new Error(`This archive exceeds the server's ${(archiveLimit / 1048576).toFixed(0)} MiB ZIP limit.`);
                const before = getSnapshot(), upload_id = await project.upload(files[0], "arrangement_archive", signal);
                const raw = await job("review_archive", { upload_id }, { expectedRevision: before.revision, signal });
                if (!raw.review_id || !Array.isArray(raw.parts) ||
                    raw.parts.some(part => !part.id || !part.name || !Array.isArray(part.versions) || !part.versions.length ||
                        part.versions.some(version => !version.id || !["wav", "mp3"].includes(version.format))))
                    throw new Error("The server archive review is incomplete. Existing tracks were kept.");
                const review = { id: raw.review_id, parts: raw.parts, skipped_files: raw.skipped_files, notes: raw.notes };
                reviews.set(review.id, { review: structuredClone(review), archive: true }); return structuredClone(review);
            }
            const grouped = groupFileVersions(files, { max_file_bytes: project.caps.limits.max_upload_bytes });
            const review = { id: crypto.randomUUID(), parts: grouped.parts, skipped_files: grouped.skipped_files, notes: grouped.notes };
            reviews.set(review.id, { review: structuredClone(review), entries: grouped.entries });
            return structuredClone(review);
        },
        async discardImport(review, options = {}) {
            if (reviews.get(review?.id)?.archive) await discardArchive(review.id, options);
            reviews.delete(review?.id);
        },
        async addImport(review, selection, { expectedRevision, signal } = {}) {
            const owned = reviews.get(review?.id), before = getSnapshot(); signal?.throwIfAborted();
            if (!owned || before.revision !== expectedRevision) throw new Error("The import review or arrangement revision changed. Review the files again.");
            if (!Array.isArray(selection) || !selection.length) throw new Error("Select at least one part.");
            const chosen = [], seen = new Set();
            for (const item of selection) {
                const part = owned.review.parts.find(part => part.id === item.part_id);
                const version = part?.versions.find(version => version.id === item.version_id);
                if (!version || seen.has(part.id) || (!owned.archive && !owned.entries.has(version.id))) throw new Error("Choose one owned file version per part.");
                seen.add(part.id); chosen.push({ name: part.name, file: owned.entries?.get(version.id) });
            }
            if (owned.archive) {
                const result = await job("commit_import", { review_id: review.id, selection }, { expectedRevision, signal });
                const value = await publish(result.snapshot); reviews.delete(review.id); return value;
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
