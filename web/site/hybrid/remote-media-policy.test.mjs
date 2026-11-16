import test from "node:test";
import assert from "node:assert/strict";
import { remoteMediaPath, parseMediaUrl, validByteRange } from "./remote-media-policy.js";
import { RemoteMediaBridge } from "./remote-media-bridge.js";
import { readBoundedJson, PROJECT_JSON_LIMIT_BYTES } from "./bounded-json.js";

const scope = "https://slimedragonair.github.io/SlimeyBeats-EZ-Master/current-work/";
test("stream addresses carry only resource identities and cannot escape the preview scope", () => {
    for (const kind of ["preview", "audio", "artifact"]) {
        const value = parseMediaUrl(`${scope}remote-media/project/${kind}/asset`, scope);
        assert.equal(value.path, remoteMediaPath("project", "asset", kind));
        assert.equal(value.path.includes("token"), false);
    }
    for (const url of [`${scope}remote-media/p/preview/a?token=secret`, `${scope}remote-media/p/preview/a#x`,
        `${scope}../remote-media/p/preview/a`, `${scope}remote-media/../preview/a`,
        `${scope}remote-media/p/preview/%2F`, "https://evil.example/remote-media/p/preview/a"])
        assert.equal(parseMediaUrl(url, scope), null);
    for (const [project, id, kind] of [["../x", "a", "audio"], ["p", "a/b", "audio"], ["p", "a", "jobs"]])
        assert.throws(() => remoteMediaPath(project, id, kind));
});
test("only single native byte ranges are forwarded", () => {
    for (const value of [null, "bytes=0-", "bytes=7-17", "bytes=-500"]) assert.equal(validByteRange(value), true);
    for (const value of ["bytes=0-1,5-8", "items=1-2", "bytes= -1", "bytes=-", "bytes=NaN-"])
        assert.equal(validByteRange(value), false);
});
test("media credentials go only to the controlling worker for a registered resource", () => {
    const controller = { scriptURL: new URL("../remote-media-worker.js", import.meta.url).href }, handlers = {}, replies = [];
    const container = { controller, addEventListener: (name, fn) => handlers[name] = fn,
        removeEventListener: (name) => delete handlers[name] };
    const bridge = new RemoteMediaBridge({ replyMediaAccess: (port, path) => replies.push(path) }, { container });
    const sent = [], port = { postMessage: value => sent.push(value), close() {} };
    const path = remoteMediaPath("p", "a");
    const event = { source: controller, ports: [port], data: { type: "slimeybeats-media-access", version: 2, path } };
    handlers.message(event); assert.deepEqual(replies, []);
    bridge.url("p", "a");
    handlers.message({ ...event, source: {} }); assert.deepEqual(replies, []);
    handlers.message(event); assert.deepEqual(replies, [path]);
    bridge.clear(); handlers.message(event); assert.deepEqual(replies, [path]);
    assert.equal(sent.some(value => Object.hasOwn(value, "token")), false);
    bridge.dispose(); assert.equal(handlers.message, undefined);
});
test("v2 metadata has an explicit 1 MiB budget while v1 default remains 64 KiB", async () => {
    const body = JSON.stringify({ assets: "x".repeat(70000) });
    await assert.rejects(readBoundedJson(Response.json(JSON.parse(body))), /budget/);
    const result = await readBoundedJson(Response.json(JSON.parse(body)), { maxBytes: PROJECT_JSON_LIMIT_BYTES });
    assert.equal(result.assets.length, 70000);
    await assert.rejects(readBoundedJson(new Response("{}"), { maxBytes: PROJECT_JSON_LIMIT_BYTES + 1 }), /Invalid JSON response budget/);
});
test("two workspaces leave each other's media request port available", () => {
    const controller = { scriptURL: new URL("../remote-media-worker.js", import.meta.url).href }, listeners = [], replies = [];
    const container = { controller, addEventListener: (_, fn) => listeners.push(fn), removeEventListener() {} };
    const master = new RemoteMediaBridge({ replyMediaAccess: () => replies.push("master") }, { container });
    const studio = new RemoteMediaBridge({ replyMediaAccess: () => replies.push("studio") }, { container });
    master.url("master-project", "master-asset"); studio.url("studio-project", "studio-asset");
    let closed = false;
    const port = { postMessage() { assert.equal(closed, false); }, close() { closed = true; } };
    const event = { source: controller, ports: [port], data: { type: "slimeybeats-media-access", version: 2,
        path: remoteMediaPath("studio-project", "studio-asset") } };
    listeners[0](event); assert.equal(closed, false); assert.deepEqual(replies, []);
    listeners[1](event); assert.equal(closed, true); assert.deepEqual(replies, ["studio"]);
    master.dispose(); studio.dispose();
});
