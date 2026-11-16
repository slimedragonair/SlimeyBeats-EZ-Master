import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const policyUrl = new URL("./endpoint-policy.js", import.meta.url);
const source = await readFile(policyUrl, "utf8");
const pinnedOrigin = "https://fixture-device.fixture-tailnet.ts.net";
const assignment = /export const PUBLIC_API_ORIGIN = [^;\r\n]+;/;
assert.match(source, assignment);
const configuredSource = source.replace(assignment, `export const PUBLIC_API_ORIGIN = ${JSON.stringify(pinnedOrigin)};`);
const moduleUrl = (value) => `data:text/javascript;base64,${Buffer.from(value).toString("base64")}`;
const configuredUrl = moduleUrl(configuredSource);
const policy = await import(configuredUrl);

test("one isolated deployment pin permits only its exact HTTPS origin, legacy API and self; no tailnet wildcard", () => {
    const pageUrl = "https://studio.example/current-work/";
    assert.equal(policy.PUBLIC_API_ORIGIN, pinnedOrigin);
    for (const origin of [pinnedOrigin, policy.LEGACY_API_ORIGIN, "https://studio.example"])
        assert.equal(policy.serverEndpoint(`${origin}/`, pageUrl), origin);
    for (const origin of [
        "https://another-device.fixture-tailnet.ts.net", "https://fixture-device.another-tailnet.ts.net",
        `${pinnedOrigin}.evil.example`, `${pinnedOrigin}.`, `${pinnedOrigin}:444`,
        pinnedOrigin.replace("https:", "http:"), pinnedOrigin.replace("https://", "https://token@"),
        `${pinnedOrigin}/v1`, `${pinnedOrigin}?token=x`, `${pinnedOrigin}#x`,
        "https://api.slimedragon.net.evil.example", "https://unrelated.example", "http://127.0.0.1:9000",
    ]) assert.throws(() => policy.serverEndpoint(origin, pageUrl), undefined, origin);
    assert.equal(policy.serverEndpoint("http://127.0.0.1:9000", "http://localhost:8000/"), "http://127.0.0.1:9000");
    const help = policy.serverEndpointHelp();
    assert.ok(help.includes(pinnedOrigin)); assert.ok(help.includes(policy.LEGACY_API_ORIGIN));
    assert.match(help, /exact origins/); assert.ok(!help.includes("*.ts.net"));
});

test("changing the deployment pin updates health and the visible processing destination", async () => {
    async function configuredModule(name) {
        const url = new URL(name, import.meta.url);
        const value = (await readFile(url, "utf8")).replace(/from (["'])(\.{1,2}\/[^"']+)\1/g, (_, quote, path) =>
            `from ${JSON.stringify(path === "./endpoint-policy.js" ? configuredUrl : new URL(path, url).href)}`);
        return import(moduleUrl(value));
    }
    const { ServerAvailability } = await configuredModule("./server-availability.js");
    const { createStudioView } = await configuredModule("./studio-view.js");
    const availability = new ServerAvailability({ pageUrl: "https://studio.example/" });
    const fields = new Map();
    const view = createStudioView({ innerHTML: "", querySelector(selector) {
        if (!fields.has(selector)) fields.set(selector, { dataset: {}, classList: { toggle() {} } });
        return fields.get(selector);
    } });
    view.render({}, { enabled: true, availability });
    assert.equal(availability.endpoint, pinnedOrigin);
    assert.equal(fields.get("#hybrid-destination").textContent, `Server: ${pinnedOrigin}`);
});
