import test from "node:test";
import assert from "node:assert/strict";
import { eqTargetKey, matchingEffects, newScopedEq, ArrangementEqProposal } from "./arrangement-timeline-eq-state.js";

const bands = Array.from({ length: 7 }, (_, i) => ({ type: "bell", freq: 50 * 2 ** i, gain: 0, q: 0.707, on: true }));

test("EQ matching includes explicit scope identity and the correct coordinate range", () => {
    const target = { scope: { kind: "clip", clip_id: "trimmed" }, range: { start_frame: 24000, end_frame: 48000 } };
    const effect = newScopedEq("eq", target, bands);
    assert.equal(matchingEffects([effect], target).length, 1);
    for (const next of [
        { scope: { kind: "clip", clip_id: "other" }, range: target.range },
        { scope: { kind: "lane", lane_id: "trimmed" }, range: target.range },
        { scope: target.scope, range: { start_frame: 0, end_frame: 24000 } },
        { scope: target.scope, range: null },
    ]) assert.equal(matchingEffects([effect], next).length, 0);
    assert.equal(eqTargetKey({ error: "Select a clip" }), "");
});

test("scope, revision and edited controls invalidate a proposal without changing committed settings", () => {
    const target = { scope: { kind: "mix" }, range: null }, draft = newScopedEq("eq", target, bands);
    draft.output_gain_db = -1;
    const before = structuredClone(draft), state = new ArrangementEqProposal();
    const proposal = { bands: structuredClone(bands), notes: ["Possible excess; compare by ear."] };
    proposal.bands[6].gain = -1.5;
    state.stage(proposal, "revision:4;target:mix;draft:0");
    assert.deepEqual(draft, before);
    const applied = state.effect(draft, "revision:4;target:mix;draft:0");
    assert.equal(applied.bands[6].gain, -1.5);
    assert.equal(applied.output_gain_db, -1);
    assert.deepEqual(draft, before);
    assert.equal(state.current("revision:5;target:mix;draft:0"), null);
    assert.throws(() => state.effect(draft, "revision:5;target:mix;draft:0"));
});
