import assert from "node:assert/strict";
import { DoctorState } from "../site/studio-doctor-state.js";
import { issueTime } from "../site/studio-doctor-format.js";
import { applyStudioEdit, StudioRecoveryRecipe } from "../site/studio-recovery-worker.js";

const state = new DoctorState();
assert.match(issueTime({ start_seconds: 1, end_seconds: 1 + 3 / 48000 }, 48000),
    /0:01\.000000–0:01\.000063 \(3 frames per channel · 0\.063 ms\)/);
assert.match(issueTime({ start_seconds: 1, end_seconds: 1 + 1 / 192000 }, 192000), /1 frame per channel/);
const report = { schema_version: 1, duration_seconds: 2, issues: [
    { id: "one", repair: "reduce_low_mids", start_seconds: 0, end_seconds: 1 },
    { id: "silence", repair: null, start_seconds: 1, end_seconds: 2 }], notes: [] };
const ready = { busy: false, pendingMix: false, target: "vocals" };
const stale = state.begin("vocals", "Vocals stem");
state.invalidate();
assert.equal(state.accept(stale, "vocals", report), false);
const older = state.begin("vocals", "Vocals stem"), newer = state.begin("preview", "Saved preview");
assert.equal(state.accept(older, "vocals", report), false);
assert.equal(state.accept(newer, "vocals", report), false);
const token = state.begin("vocals", "Exact fixture name stem");
assert.equal(state.accept(token, "vocals", report), true);
state.selected = "one";
assert.equal(state.canRepair(ready), true);
for (const changed of [{ busy: true }, { pendingMix: true }, { target: "source" }])
    assert.equal(state.canRepair({ ...ready, ...changed }), false);
state.selected = "silence";
assert.equal(state.canRepair(ready), false);
state.selected = "one";
assert.equal(state.download().target_label, "Exact fixture name stem");
assert.match(state.download().scope, /No audio, undo history, recovery recipe or live pending mix/);
for (const reason of ["source replacement", "preview", "bank", "undo", "redo", "reset", "recovery", "fader"]) {
    const token = state.begin("vocals", reason); state.accept(token, "vocals", report); state.selected = "one";
    state.invalidate();
    assert.equal(state.report, null, reason); assert.equal(state.download(), null, reason);
    assert.equal(state.canRepair(ready), false, reason);
    assert.equal(state.accept(token, "vocals", report), false, reason);
}
assert.throws(() => state.accept(state.begin("source", "Source"), "source", { issues: [] }), /invalid/);

const calls = [], candidate = { doctor: (...args) => calls.push(args) };
const repair = { action: "declip", start_seconds: 0.5, end_seconds: 0.501, strength: 0.5 };
applyStudioEdit(candidate, "doctor", { target: "vocals", repair });
assert.deepEqual(calls, [["vocals", JSON.stringify(repair)]]);
const recipe = new StudioRecoveryRecipe(class {});
recipe.commit("doctor", { target: "vocals", repair, report }, {});
assert.deepEqual(recipe.edits, [{ action: "doctor", target: "vocals", repair }]);
assert.equal("report" in recipe.edits[0], false, "Scan snapshots must not enter replay recipes");
assert.throws(() => applyStudioEdit(candidate, "doctor-scan", { target: "source" }), /Unsupported/);
console.log("Doctor state/recovery checks passed: stale replies, target/revision invalidation, pending-mix and busy guards, metadata-only report, repair-only recipe.");
