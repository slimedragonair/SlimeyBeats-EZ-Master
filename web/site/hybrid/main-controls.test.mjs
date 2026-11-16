import test from "node:test";
import assert from "node:assert/strict";
import { serverProcessing } from "./main-controls.js";
import { studioTemplate } from "./studio-template.js";

test("engine selection does not replace ordinary editor controls or their locks", () => {
    const controls = new Map(["analyze", "master", "mobile-master", "export", "add", "profile", "hybrid-enabled"]
        .map((id) => [id, { textContent: id, disabled: id === "master" }]));
    const original = globalThis.document;
    globalThis.document = { getElementById: (id) => controls.get(id) };
    try {
        const controller = { enabled: true };
        serverProcessing.attach(controller, () => {});
        serverProcessing.renderMainControls({ locked: true });
        assert.equal(controls.get("hybrid-enabled").disabled, true);
        assert.equal(controls.get("master").disabled, true);
        assert.equal(controls.get("export").textContent, "export");
        assert.equal(controls.get("profile").disabled, false);
        controller.enabled = false; serverProcessing.changed();
        assert.equal(controls.get("master").disabled, true);
        assert.equal(controls.get("analyze").disabled, false);
    } finally { if (original === undefined) delete globalThis.document; else globalThis.document = original; }
});

test("only explicit Master dispatches audio to the selected server engine", async () => {
    let calls = 0, cancelled = 0;
    const audio = { decoded: true }, settings = { profile: "neutral" }, reference = { measured: true };
    const controller = { enabled: false };
    serverProcessing.attach(controller, async (...args) => { calls++; assert.deepEqual(args.slice(0,3), [audio, settings, reference]); return "master"; },
        () => { cancelled++; });
    await assert.rejects(serverProcessing.master(audio, settings, reference), /Turn on server/);
    controller.enabled = true; serverProcessing.changed();
    assert.equal(calls, 0);
    assert.equal(await serverProcessing.master(audio, settings, reference), "master");
    assert.equal(calls, 1);
    serverProcessing.cancel(); assert.equal(cancelled, 1);
});

test("top switch has contextual help without another file picker, settings form, consent checkbox or submit button", () => {
    assert.equal((studioTemplate.match(/type="checkbox"/g) || []).length, 1);
    for (const obsolete of ["hybrid-consent", "hybrid-file", "hybrid-profile", "hybrid-format", "hybrid-submit"])
        assert.equal(studioTemplate.includes(obsolete), false, obsolete);
    assert.match(studioTemplate, /Analyze → Master → tweak → Export/);
});
