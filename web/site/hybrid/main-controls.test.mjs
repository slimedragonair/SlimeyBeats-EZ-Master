import test from "node:test";
import assert from "node:assert/strict";
import { serverProcessing } from "./main-controls.js";

test("main selection survives a rejected local decode and later control refreshes; local clear releases it", () => {
    const selected = [];
    const controller = { enabled: false, selectMainFile: (file) => selected.push(file) };
    serverProcessing.attach(controller, () => {});
    const old = { name: "old.wav" }, longer = { name: "longer.mp3" };
    serverProcessing.rememberLocalFile(old);
    serverProcessing.select(longer);
    serverProcessing.rememberLocalFile(old);
    assert.equal(selected.at(-1), longer, "Old player refresh must not erase the new pending main file");
    serverProcessing.rememberLocalFile(null); assert.equal(selected.at(-1), null);
});

test("server-mode selections and review stay independent from local source metadata", () => {
    let file, reviews = 0;
    const controller = { enabled: true, selectMainFile: (value) => { file = value; } };
    serverProcessing.attach(controller, () => { reviews++; });
    const remote = { name: "remote.mp3" };
    serverProcessing.select(remote);
    serverProcessing.rememberLocalFile({ name: "local.wav" });
    assert.equal(file, remote); serverProcessing.review(); assert.equal(reviews, 1);
    controller.enabled = false; serverProcessing.review(); assert.equal(reviews, 1);
});

test("server updates preserve local busy locks, and mode off without a local app leaves Master unavailable", () => {
    const controls = new Map(["master", "mobile-master", "add"].map((id) => [id, { textContent: "Master", disabled: false }]));
    const original = globalThis.document;
    globalThis.document = { getElementById: (id) => controls.get(id) };
    try {
        const controller = { enabled: true, selectMainFile() {} };
        serverProcessing.attach(controller, () => {});
        serverProcessing.renderMainControls({ locked: true });
        serverProcessing.changed();
        assert.equal(controls.get("master").disabled, true);
        assert.equal(controls.get("add").disabled, true);
        serverProcessing.renderMainControls({ locked: false });
        assert.equal(controls.get("master").disabled, false);
        controller.enabled = false; serverProcessing.changed();
        assert.equal(controls.get("master").disabled, true);
        assert.equal(controls.get("master").textContent, "◇ Master track");
    } finally { if (original === undefined) delete globalThis.document; else globalThis.document = original; }
});
