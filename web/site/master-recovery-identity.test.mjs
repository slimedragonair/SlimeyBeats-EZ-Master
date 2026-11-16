import test from "node:test";
import assert from "node:assert/strict";
import { captureMasterRecovery } from "./master-recovery.js";
import { recoverMasterIdentity, rebindMasterSignature } from "./master-recovery-identity.js";
import { recoveryFile, readRecoveryFile } from "./recovery-format.js";

const key = (reference_revision, profile = "neutral") => JSON.stringify({ profile, reference: "browser-reference", reference_revision });
const buffer = { length: 2, numberOfChannels: 1, sampleRate: 48000, getChannelData: () => new Float32Array([.1, -.1]) };

test("capture and portable recovery preserve reference-only staleness", async () => {
    const master = captureMasterRecovery({ player: { original: buffer, master: buffer },
        settings: {}, baseline: {}, signature: key(1), currentSignature: key(2),
        reference: new Blob(["reference B"]), baselineReference: new Blob(["reference A"]) });
    const restored = await readRecoveryFile(recoveryFile({ version: 1, master }));
    const identity = await recoverMasterIdentity(restored.master);
    assert.equal(identity.referenceStale, true);
    assert.notEqual(rebindMasterSignature(identity.signature, 5, identity.referenceStale), key(5));
});

test("unchanged reference remains current and saved settings are not replaced", async () => {
    const identity = await recoverMasterIdentity({ signature: key(1), currentSignature: key(1) });
    assert.equal(rebindMasterSignature(identity.signature, 5, identity.referenceStale), key(5));
    const changed = await recoverMasterIdentity({ signature: key(1), currentSignature: key(1, "edm") });
    assert.notEqual(rebindMasterSignature(changed.signature, 5, changed.referenceStale), key(5, "edm"));
    assert.equal(changed.signature.profile, "neutral");
});

test("a stale recovered master remains stale through another save and restore", async () => {
    const first = await recoverMasterIdentity({ signature: key(1), currentSignature: key(2) });
    const saved = rebindMasterSignature(first.signature, 7, first.referenceStale);
    const second = await recoverMasterIdentity({ signature: saved, currentSignature: key(7) });
    assert.equal(second.referenceStale, true);
    assert.notEqual(rebindMasterSignature(second.signature, 9, second.referenceStale), key(9));
});

test("legacy files compare exact reference blobs across chunk boundaries", async () => {
    const bytes = new Uint8Array(65540);
    const baselineReference = new Blob([bytes]);
    let identity = await recoverMasterIdentity({ signature: key(1), baselineReference, reference: new Blob([bytes]) });
    assert.equal(identity.referenceStale, false);
    bytes[65537] = 1;
    identity = await recoverMasterIdentity({ signature: key(1), baselineReference, reference: new Blob([bytes]) });
    assert.equal(identity.referenceStale, true);
});

test("legacy absent or different references preserve their actual relationship", async () => {
    const baselineReference = new Blob(["A"]);
    for (const [reference, baseline, stale] of [[null, null, false], [baselineReference, null, true],
        [null, baselineReference, true], [new Blob(["longer"]), baselineReference, true]]) {
        const identity = await recoverMasterIdentity({ signature: key(1), reference, baselineReference: baseline });
        assert.equal(identity.referenceStale, stale);
    }
});

test("explicit signatures retain revision changes even for identical reference bytes", async () => {
    const reference = new Blob(["same bytes"]);
    const identity = await recoverMasterIdentity({ signature: key(1), currentSignature: key(2), reference, baselineReference: reference });
    assert.equal(identity.referenceStale, true);
});

test("invalid identities fail before recovery commits", async () => {
    for (const invalid of ["null", "[]", "{}", "{", '{"reference_revision":-1}', '{"reference_revision":"1"}']) {
        await assert.rejects(recoverMasterIdentity({ signature: invalid, currentSignature: key(1) }));
        await assert.rejects(recoverMasterIdentity({ signature: key(1), currentSignature: invalid }));
    }
    assert.throws(() => rebindMasterSignature({}, 0, true));
    assert.throws(() => rebindMasterSignature({}, 1, "true"));
});
