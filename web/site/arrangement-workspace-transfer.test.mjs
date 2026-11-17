import test from "node:test";
import assert from "node:assert/strict";
import { freshArrangement, continueLocalArrangement, arrangementAssetBytes, LOCAL_ARRANGEMENT_PCM_BYTES } from "./arrangement-workspace-transfer.js";
import { ArrangementDrafts } from "./arrangement-timeline-drafts.js";

const fresh = () => ({ arrangement: { lanes: [], eq: [] }, assets: [], history: {} });
test("explicit continuation transfers one canonical file and never merges into an existing server history", async () => {
    let captures = 0, opens = 0, target = fresh(); const file = new Blob(["canonical project"]), ui = { selectedClip: "stable_clip" };
    const local = { loaded: true, async captureProject() { captures++; return { file, ui }; } };
    const server = { state: () => target, async openProject(actual, draft) { assert.equal(actual, file); assert.equal(draft, ui); opens++; } };
    assert.equal(await continueLocalArrangement(local, server, () => true), true);
    assert.equal(captures, 1); assert.equal(opens, 1);
    target = { ...fresh(), history: { can_undo: true } };
    assert.equal(freshArrangement(target), false);
    assert.equal(await continueLocalArrangement(local, server, () => true), false);
    assert.equal(captures, 1); assert.equal(opens, 1);
    target = fresh(); await assert.rejects(continueLocalArrangement(local, server, () => false), /changed before transfer/);
    assert.equal(opens, 1);
});

test("unfinished raw fields survive a metadata-only handoff and retain conflict checks", () => {
    const local = new ArrangementDrafts(), record = { id: "clip", source_id: "asset", start_frame: 0 };
    local.set("timing:clip", record, "clip-start", "", ["id", "source_id", "start_frame"]);
    const remote = new ArrangementDrafts(); remote.restore(local.capture());
    assert.equal(remote.value("timing:clip", "clip-start", "0"), ""); assert.equal(remote.conflicts("timing:clip", record), false);
    assert.equal(remote.conflicts("timing:clip", { ...record, start_frame: 1 }), true);
    remote.clear("timing:clip"); assert.equal(local.changed("timing:clip"), true);
});

test("server-to-local budget uses decoded source frames and channels before any download", () => {
    const server = { assets: [{ frames: LOCAL_ARRANGEMENT_PCM_BYTES / 4, channels: 1 }, { frames: 1, channels: 2 }] };
    assert.equal(arrangementAssetBytes(server), LOCAL_ARRANGEMENT_PCM_BYTES + 8);
});
