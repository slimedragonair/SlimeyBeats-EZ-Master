import test from "node:test";
import assert from "node:assert/strict";
import { createRemoteArrangementImport } from "./arrangement-remote-import.js";

function harness(parts = [{ id: "owned_part", name: "Lead", versions: [{ id: "owned_wav", format: "wav" }, { id: "owned_mp3", format: "mp3" }] }]) {
    const uploaded = [], jobs = [], discarded = [];
    const snapshot = { epoch: "epoch", revision: 3, capabilities: { import_zip: true } };
    const project = { caps: { arrangement: { max_archive_bytes: 1024 }, limits: { max_upload_bytes: 2048 } },
        async upload(file, role) { uploaded.push({ file, role }); return "upload"; } };
    const importer = createRemoteArrangementImport({ project, getSnapshot: () => snapshot,
        async job(operation, values) {
            jobs.push({ operation, values });
            return operation === "review_archive" ? { review_id: "owned_review", parts } : { snapshot: { revision: 4 } };
        }, publish: async value => value, discardArchive: async id => discarded.push(id) });
    return { importer, uploaded, jobs, discarded };
}

test("ZIP review uploads once and commit uses only owned part/version IDs without reading the archive", async () => {
    const state = harness(), file = new File(["ZIP"], "parts.zip"); file.arrayBuffer = () => { throw new Error("whole archive read"); };
    const review = await state.importer.reviewImport([file]);
    review.parts[0].versions.push({ id: "foreign", format: "wav" });
    await assert.rejects(state.importer.addImport(review, [{ part_id: "owned_part", version_id: "foreign" }], { expectedRevision: 3 }), /owned file version/);
    const value = await state.importer.addImport(review, [{ part_id: "owned_part", version_id: "owned_wav" }], { expectedRevision: 3 });
    assert.equal(value.revision, 4); assert.equal(state.uploaded.length, 1); assert.equal(state.uploaded[0].role, "arrangement_archive");
    assert.deepEqual(state.jobs.at(-1), { operation: "commit_import", values: { review_id: "owned_review", selection: [{ part_id: "owned_part", version_id: "owned_wav" }] } });
    await assert.rejects(state.importer.addImport(review, [{ part_id: "owned_part", version_id: "owned_wav" }], { expectedRevision: 3 }), /review.*changed/);
});

test("valid bounded review metadata has no additional frontend part-count policy", async () => {
    const parts = Array.from({ length: 600 }, (_, i) => ({ id: `part_${i}`, name: `Part ${i}`, versions: [{ id: `wav_${i}`, format: "wav" }] }));
    const { importer } = harness(parts);
    assert.equal((await importer.reviewImport([new File(["ZIP"], "parts.zip")])).parts.length, 600);
});

test("archive admission checks the server ZIP budget before upload and discard retains ownership", async () => {
    const state = harness();
    await assert.rejects(state.importer.reviewImport([new File([new Uint8Array(1025)], "large.zip")]), /ZIP limit/);
    assert.equal(state.uploaded.length, 0);
    const review = await state.importer.reviewImport([new File(["ZIP"], "parts.zip")]);
    await state.importer.discardImport({ id: "foreign" }); assert.deepEqual(state.discarded, []);
    await state.importer.discardImport(review); assert.deepEqual(state.discarded, ["owned_review"]);
});
