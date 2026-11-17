import test from "node:test";
import assert from "node:assert/strict";
import { RemoteMastering } from "./remote-mastering.js";

const bands = Array.from({ length: 7 }, (_, index) => ({
    type: index === 6 ? "high_shelf" : "bell", freq: 50 * 2 ** index,
    gain: 0, q: 0.707, on: true,
}));

test("remote suggestions preserve detailed evidence and use the pre-EQ master", async () => {
    const remote = new RemoteMastering();
    remote.original = { id: "original" };
    remote.master = { id: "master" };
    const proposal = { bands, notes: ["Review before applying."], high_frequency: { version: 1 } };
    const before = structuredClone(proposal), requests = [];
    remote.project.command = async (...args) => { requests.push(args); return { command: { result: proposal } }; };
    const result = await remote.suggest({ profile: "neutral" });
    assert.deepEqual(result, proposal);
    assert.deepEqual(proposal, before);
    assert.equal(requests.length, 1);
    assert.equal(requests[0][0], "suggest");
    assert.equal(requests[0][1].audio_id, "master");
    assert.deepEqual(remote.original, { id: "original" });
    assert.deepEqual(remote.master, { id: "master" });
});

test("legacy and wrapped remote proposals normalize to the detailed contract", async () => {
    for (const response of [{ suggestion: { bands } }, { command: { result: { suggestion: { bands } } } },
        { command: { result: { bands } } }]) {
        const remote = new RemoteMastering();
        remote.original = { id: "original" };
        remote.project.command = async (operation, request) => {
            assert.equal(request.audio_id, "original");
            return response;
        };
        assert.deepEqual(await remote.suggest({}), { bands, notes: [], high_frequency: null });
    }
});

test("missing or malformed server bands fail without changing retained audio", async () => {
    for (const response of [{}, { command: { result: { bands: null } } }, { suggestion: { bands: bands.slice(1) } }]) {
        const remote = new RemoteMastering();
        const original = remote.original = { id: "original" };
        remote.project.command = async () => response;
        await assert.rejects(remote.suggest({}), /invalid EQ suggestion/);
        assert.equal(remote.original, original);
        assert.equal(remote.master, undefined);
    }
});
