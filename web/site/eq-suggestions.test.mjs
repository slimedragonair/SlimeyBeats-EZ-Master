import test from "node:test";
import assert from "node:assert/strict";
import { wireEqSuggestions } from "./eq-suggestions.js";

function fixture() {
    const bands = Array.from({ length: 7 }, (_, i) => ({ type: i === 6 ? "high_shelf" : "bell",
        freq: [50, 120, 300, 800, 2000, 5000, 10000][i], gain: i === 6 ? -1.25 : 0, q: 0.707, on: true }));
    const original = {}, source = {};
    const context = { original, source, trackRevision: 1, eqRevision: 4, profile: "neutral" };
    const settings = { manual_eq: structuredClone(bands), eq_output_db: 0.75, eq_hold_loudness: true };
    settings.manual_eq[2].gain = 2.75;
    const proposal = { bands, notes: ["Possible excess energy above 10 kHz; try a gentle cut.",
        "This is a heuristic. Compare at matched loudness."], high_frequency: { excess_candidate: true, band_low_hz: 10000, band_high_hz: 16000 } };
    const button = { disabled: false }, note = { textContent: "" }, details = { open: false }, explanation = { textContent: "" }, messages = [];
    let requests = 0, applies = 0, request = async () => proposal;
    const controller = wireEqSuggestions({ button, note, details, explanation, getContext: () => ({ ...context }),
        request: async () => { requests++; return request(); },
        apply: value => { applies++; settings.manual_eq = value; context.eqRevision++; },
        runJob: action => action(() => {}), onStatus: (...message) => messages.push(message) });
    return { button, note, details, explanation, controller, context, settings, proposal, messages,
        request: value => { request = value; }, get requests() { return requests; }, get applies() { return applies; } };
}

test("Suggest keeps pending EQ and source intact; the existing button explicitly applies a copied proposal", async () => {
    const f = fixture(), before = structuredClone(f.settings), source = f.context.source;
    await f.button.onclick();
    assert.deepEqual(f.settings, before);
    assert.equal(f.context.source, source);
    assert.equal(f.applies, 0);
    assert.equal(f.button.textContent, "Apply curve");
    for (const note of f.proposal.notes) assert.ok(f.explanation.textContent.includes(note));
    assert.match(f.note.textContent, /10–16 kHz.*-1.25 dB/);
    assert.ok(f.note.textContent.length < 180);
    assert.equal(f.details.open, false);
    assert.match(f.note.textContent, /Not applied/);
    await f.button.onclick();
    assert.equal(f.requests, 1);
    assert.equal(f.applies, 1);
    assert.deepEqual(f.settings.manual_eq, f.proposal.bands);
    assert.notEqual(f.settings.manual_eq, f.proposal.bands);
    assert.equal(f.settings.eq_output_db, before.eq_output_db);
    assert.equal(f.settings.eq_hold_loudness, before.eq_hold_loudness);
    assert.equal(f.context.source, source);
    assert.match(f.explanation.textContent, /B · Master.*A compares with the original mix/);
    assert.equal(f.button.textContent, "✦ Suggest EQ");
});

test("source identity, same-source reload, profile and manual edits each invalidate unapplied proposals", async () => {
    for (const [key, value] of [["source", {}], ["original", {}], ["trackRevision", 2],
        ["profile", "metal"], ["eqRevision", 5]]) {
        const f = fixture(), before = structuredClone(f.settings);
        await f.button.onclick();
        f.context[key] = value;
        await f.button.onclick();
        assert.equal(f.applies, 0, key);
        assert.equal(f.requests, 1, key);
        assert.deepEqual(f.settings, before, key);
        assert.equal(f.button.textContent, "✦ Suggest EQ");
    }
});

test("metadata from an applied curve is cleared when later edits change its context", async () => {
    const f = fixture();
    await f.button.onclick();
    await f.button.onclick();
    f.details.open = true;
    f.context.eqRevision++;
    f.controller.sync();
    assert.equal(f.details.open, false);
    assert.doesNotMatch(f.note.textContent, /Applied\./);
    assert.ok(!f.explanation.textContent.includes(f.proposal.notes[0]));
});

test("edits and same-layout source replacement during a request cannot publish or apply a late proposal", async () => {
    for (const key of ["eqRevision", "source", "profile"]) {
        const f = fixture(), before = structuredClone(f.settings);
        let resolve;
        f.request(() => new Promise(done => { resolve = done; }));
        const running = f.button.onclick();
        f.context[key] = key === "source" ? {} : key === "profile" ? "metal" : 5;
        resolve(f.proposal);
        await running;
        assert.equal(f.applies, 0);
        assert.deepEqual(f.settings, before);
        assert.equal(f.button.textContent, "✦ Suggest EQ");
        assert.match(f.note.textContent, /changed while measuring/);
    }
});

test("disabled actions and Clear preserve settings and cannot apply an obsolete proposal", async () => {
    const f = fixture(), before = structuredClone(f.settings);
    f.button.disabled = true;
    await f.button.onclick();
    assert.equal(f.requests, 0);
    f.button.disabled = false;
    await f.button.onclick();
    f.button.disabled = true;
    await f.button.onclick();
    assert.equal(f.applies, 0);
    f.controller.clear();
    assert.equal(f.button.textContent, "✦ Suggest EQ");
    assert.deepEqual(f.settings, before);
});

test("unavailable and malformed proposals leave valid EQ intact and show an explanation", async () => {
    for (const value of [null, [], { bands: [] }, { bands: Array(7).fill({}), notes: [] }]) {
        const f = fixture(), before = structuredClone(f.settings);
        f.request(async () => value);
        await assert.rejects(f.button.onclick(), /suggestion could not be read/);
        assert.deepEqual(f.settings, before);
        assert.equal(f.applies, 0);
        assert.equal(f.button.textContent, "✦ Suggest EQ");
        assert.match(f.note.textContent, /current EQ was kept/);
    }
    const f = fixture();
    f.request(async () => { throw new Error("No reliable suggestion for silent audio."); });
    await assert.rejects(f.button.onclick(), /silent audio/);
    assert.match(f.explanation.textContent, /silent audio/);
    assert.equal(f.applies, 0);
});

test("original-only proposals remain editable and cancellation before delivery never publishes a curve", async () => {
    const f = fixture();
    f.context.source = f.context.original;
    await f.button.onclick();
    assert.match(f.explanation.textContent, /Proposed from the original mix/);
    f.controller.clear();
    f.request(async () => { throw new DOMException("Cancelled", "AbortError"); });
    await assert.rejects(f.button.onclick(), { name: "AbortError" });
    assert.equal(f.applies, 0);
    assert.equal(f.button.textContent, "✦ Suggest EQ");
});
