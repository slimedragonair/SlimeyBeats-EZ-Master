// Exercise the production controller directly, including disabled-button bypasses.
import assert from "node:assert/strict";
import { wireStudioDoctor } from "../site/studio-doctor.js";
import { subscribeRecovery } from "../site/recovery-bridge.js";

class Node {
    constructor() { this.children = []; this.dataset = {}; this.style = { setProperty() {} }; this.classList = { toggle() {} }; this.textContent = ""; this.value = ""; }
    setAttribute(key, value) { this[key] = value; }
    append(...nodes) { this.children.push(...nodes); }
    replaceChildren(...nodes) { this.children = nodes; }
}
const nodes = new Map();
globalThis.document = {
    getElementById: id => { if (!nodes.has(id)) nodes.set(id, new Node()); return nodes.get(id); },
    createElement: () => new Node(),
    querySelectorAll: () => {
        const all = [], visit = node => { if (node.dataset.doctorIssue) all.push(node); node.children.forEach(visit); };
        for (const node of nodes.values()) visit(node);
        return all;
    },
};
const $ = name => document.getElementById(`s-doctor-${name}`), target = document.getElementById("s-target");
target.value = "vocals"; target.selectedOptions = [{ textContent: "Fixture vocals stem" }]; $("strength").value = "0.5";
const current = { busy: false, pendingMix: false, filename: "Exact song", info: { loaded: true } };
const calls = [], jobs = [], seeks = []; let recoveryChanges = 0, settle;
subscribeRecovery(() => ++recoveryChanges);
const report = { schema_version: 1, duration_seconds: 2, thresholds: { example_dbfs: -70 }, channels: 2,
    sample_rate: 48000, truncated: false, notes: [], issues: [{ id: "one", kind: "clipping", title: "Full-scale sample", severity: "warning",
        description: "Review", evidence: { absolute_peak: 1 }, repair: "declip", start_seconds: 0.5, end_seconds: 0.501 }] };
const doctor = wireStudioDoctor({
    getState: () => current,
    client: { call: (action, values) => { calls.push({ action, values }); return new Promise(resolve => { settle = resolve; }); } },
    job: (...args) => { jobs.push(args); return args[1](); }, mutate: (...args) => calls.push(args),
    seek: value => seeks.push(value), draw() {}, sync() {},
});
const scan = $("scan").onclick();
assert.equal(doctor.scanning, true); assert.equal(jobs.length, 0, "Read-only scan entered mutating job");
settle(structuredClone(report)); await scan;
assert.equal(doctor.scanning, false); assert.equal(recoveryChanges, 0, "Scan changed recovery state");
assert.match($("target").textContent, /Fixture vocals stem.*Exact song/);
$("issues").children[0].children[0].onclick(); assert.deepEqual(seeks, [0.5]);
current.pendingMix = true; doctor.sync();
assert.equal($("fix").disabled, true);
$("fix").disabled = false; $("fix").onclick();
assert.equal(jobs.length, 0, "Direct handler bypassed pending-mix guard");
current.pendingMix = false; current.busy = true; $("fix").onclick(); assert.equal(jobs.length, 0);
current.busy = false; $("fix").onclick();
assert.equal(jobs.length, 1); assert.deepEqual(jobs[0].slice(2), [true, true], "Doctor must guard live mix and preserve playing transport");
assert.deepEqual(calls.at(-1), ["doctor", { target: "vocals", repair: { action: "declip", start_seconds: 0.5, end_seconds: 0.501, strength: 0.5 } }]);
const stale = $("scan").onclick(); target.value = "preview"; doctor.invalidate(); settle(report); await stale;
assert.equal($("issues").children.length, 0, "Late target reply was displayed");
assert.equal($("download").disabled, true, "Stale report remained exportable");
const cancelled = $("scan").onclick(); doctor.cancel(); settle(report); await cancelled;
assert.equal($("issues").children.length, 0, "Cancelled reply was displayed");
assert.equal(recoveryChanges, 0, "Doctor navigation mutated recovery");
console.log("Doctor controller checks passed: read-only scan/navigation, exact target, pending-mix/busy direct-handler guard, transport option, late/cancel rejection.");
