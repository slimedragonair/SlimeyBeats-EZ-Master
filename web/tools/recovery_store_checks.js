export async function checkRecoveryStore() {
    const { RecoveryStore } = await import("/recovery-store.js");
    const { createRecovery } = await import("/recovery-controller.js");
    const { recoveryFile, readRecoveryFile } = await import("/recovery-format.js");
    const { buildSupportReport, redactDescription } = await import("/support-report.js");
    const { retentionPlan } = await import("/recovery-retention.js");
    const ok = (condition, text) => { if (!condition) throw new Error(text); };
    const checks = [], store = new RecoveryStore();
    for (const key of ["configure", "state", "save", "list", "read"]) {
        const original = store[key].bind(store);
        store[key] = (...args) => { window.recoveryTestStep = `${key}:${args[0]}:${args[1]}`; return original(...args); };
    }
    await store.configure(false, true);
    ok(!(await store.state()).enabled, "Storage defaults off");
    const point = { version: 1, master: { marker: "PRIVATE_SONG_CANARY", audio: new Blob([new Float32Array([.25, -.25])]) } };
    const policy = await store.configure(true);
    for (let n = 1; n <= 5; n++) await store.save("fixture", n, policy.generation, point);
    let rows = await store.list();
    ok((await store.read(rows[0].id)).master.audio instanceof Blob, "ArrayBuffer payload restores the audio Blob contract");
    const db = await store.open();
    const metadata = await new Promise((resolve) => { const r = db.transaction("points").objectStore("points").get(rows[0].id); r.onsuccess = () => resolve(r.result); });
    ok(!("snapshot" in metadata), "Point index contains no audio payload");
    ok(rows.length === 3 && rows[0].sequence === 5, "Only three latest good points retained");
    ok(!await store.save("fixture", 4, policy.generation, point), "An older writer cannot replace newer data");
    ok(!await store.save("fixture", 6, policy.generation, point, () => false), "Superseded capture cannot publish");
    const before = JSON.stringify(await store.list());
    try { await store.save("fixture", 7, policy.generation, { version: 9, master: {} }); throw new Error("Unexpected valid version"); }
    catch (e) { ok(e.message !== "Unexpected valid version", "Unknown versions rejected"); }
    ok(JSON.stringify(await store.list()) === before, "Invalid snapshots preserve latest good");
    try { await store.save("fixture", 7, policy.generation, { version: 1, master: { uncloneable: () => {} } }); }
    catch { /* Real IDB structured-clone refusal must roll back retention deletes too. */ }
    ok(JSON.stringify(await store.list()) === before, "Clone failure rolled back deletes of older good points");
    const put = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function(...args) {
        if (this.name === "points") throw new DOMException("PRIVATE_QUOTA_CONTEXT", "QuotaExceededError");
        return put.apply(this, args);
    };
    try { await store.save("fixture", 8, policy.generation, point); }
    catch (e) { ok(e.name === "QuotaExceededError", "Real quota refusal surfaced"); }
    finally { IDBObjectStore.prototype.put = put; }
    ok(JSON.stringify(await store.list()) === before, "Quota failure preserved all good restore points");
    IDBObjectStore.prototype.put = function(value, ...args) {
        if (this.name === "payloads") return this.add({ ...value, id: "fixture:5" });
        return put.call(this, value, ...args);
    };
    try { await store.save("fixture", 9, policy.generation, point); throw new Error("unexpected failed request accepted"); }
    catch (e) { ok(e.name === "ConstraintError", "Asynchronous request failure retains its actual error type"); }
    finally { IDBObjectStore.prototype.put = put; }
    ok(JSON.stringify(await store.list()) === before, "Asynchronous failed write rolled back index and payload pruning");
    await store.configure(false, true);
    ok(!await store.save("fixture", 8, policy.generation, point), "Disable/delete invalidates in-flight writers");
    ok((await store.list()).length === 0, "No resurrection after delete");
    checks.push("real IndexedDB: atomic retention, stale generation, rejected capture and latest-good preservation");
    const large = [{ id: "own:2", owner: "own", sequence: 2, bytes: 120 * 1048576 },
        { id: "own:1", owner: "own", sequence: 1, bytes: 120 * 1048576 }];
    const thinning = retentionPlan(large, "own", 120 * 1048576);
    ok(thinning.has("own:1") && !thinning.has("own:2"), "Large consecutive saves thin old own points, keeping previous latest-good");
    try { retentionPlan([...large, { id: "other", owner: "other", sequence: 1, bytes: 120 * 1048576 }], "own", 120 * 1048576); throw new Error("unexpected eviction of other tab"); }
    catch (e) { ok(!e.message.startsWith("unexpected"), "Other-tab points never evicted automatically"); }
    const capacity = await store.configure(true);
    for (let owner = 0; owner < 4; owner++)
        for (let n = 1; n <= 3; n++) await store.save(`tab-${owner}`, n, capacity.generation, point);
    try { await store.save("tab-5", 1, capacity.generation, point); throw new Error("unexpected capacity accepted"); }
    catch (e) { ok(!e.message.startsWith("unexpected"), "Global point budget enforced"); }
    ok((await store.list()).length === 12, "No other tab's points pruned to make space");
    await store.configure(false, true);
    const restored = await readRecoveryFile(recoveryFile(point));
    ok(restored.master.marker === point.master.marker && (await restored.master.audio.arrayBuffer()).byteLength === 8, "Portable recovery binary round trip");
    const invalid = new Blob(["SBREC001", new Uint8Array([255,255,255,255])]);
    try { await readRecoveryFile(invalid); throw new Error("unexpected invalid file accepted"); }
    catch (e) { ok(!e.message.startsWith("unexpected"), "Malformed manual recovery rejected"); }
    checks.push("bounded recovery file: exact blob round trip and malformed header refusal");
    const isolatedBridge = await import("/recovery-bridge.js#isolated-budget-test");
    let allocated = false;
    isolatedBridge.connectRecovery({ master: { loaded: true, busy: false, estimate: () => 128 * 1048576 + 1,
        capture: () => { allocated = true; throw new Error("Audio should not allocate after failed preflight"); } },
        studio: { loaded: false, busy: false, estimate: () => 0, capture: () => null } });
    try { await isolatedBridge.workspaceRecovery.capture(); throw new Error("unexpected oversized capture accepted"); }
    catch (e) { ok(e.message.includes("Editing limits are unchanged"), "Oversize recovery has a clear, separate budget"); }
    ok(!allocated, "Oversize recovery rejected before copying any audio");
    checks.push("oversize recovery pauses before allocating PCM and does not change editing/import limits");
    const report = buildSupportReport("Try C:\\Users\\PRIVATE_PERSON\\song.wav\n/home/PRIVATE_PERSON/music.wav\nprivate@example.com https://host.example/token", "unknown-private-category",
        { engineReady: true, filename: "PRIVATE_SONG_CANARY", error: "RAW_ERROR_CANARY", profile: "CUSTOM_PROFILE_CANARY", path: "PATH_CANARY" },
        ["RAW_ERROR_CANARY", "page_error"], { message: "STORAGE_ERROR_CANARY" });
    for (const canary of ["PRIVATE_PERSON", "PRIVATE_SONG_CANARY", "RAW_ERROR_CANARY", "CUSTOM_PROFILE_CANARY", "PATH_CANARY", "STORAGE_ERROR_CANARY", "private@example.com", "host.example", "unknown-private-category"])
        ok(!report.includes(canary), `Report leaked ${canary}`);
    ok(redactDescription("x".repeat(5000)).length === 1500, "Description bounded");
    checks.push("allowlisted reports reject filename/path/profile/raw-log canaries and bound/redact descriptions");
    let release, captured, committed = false, disposed = false;
    const mocked = { state: async () => ({ version: 1, enabled: true, generation: 1 }), save: async (...args) => { captured = args; return true; },
        read: async () => point, list: async () => [] };
    const controller = createRecovery({ store: mocked, capture: () => new Promise((resolve) => { release = resolve; }),
        busy: () => false, hasWork: () => false, prepare: async () => ({ commit() { committed = true; }, dispose() { disposed = true; } }) });
    await controller.initial;
    const saving = controller.save(); controller.changed(); release({ master: { marker: "old" } }); await saving;
    ok(!captured, "Queued stale capture did not publish"); controller.dispose();
    const failing = createRecovery({ store: { ...mocked, save: async () => { throw new DOMException("PRIVATE_PATH", "QuotaExceededError"); } },
        capture: async () => ({ master: { marker: "safe" } }), busy: () => false, hasWork: () => false,
        prepare: async () => ({ commit() {}, dispose() {} }) });
    await failing.initial; await failing.save();
    ok(failing.state.paused && failing.state.message.includes("storage is full"), "Quota disables recovery only");
    ok((await failing.download()).size > 0, "Manual recovery remains usable after quota failure"); failing.dispose();
    let releaseRead;
    const racing = createRecovery({ store: { ...mocked, read: () => new Promise((resolve) => { releaseRead = resolve; }) },
        capture: async () => null, busy: () => false, hasWork: () => false,
        prepare: async () => ({ commit() { committed = true; }, dispose() { disposed = true; } }) });
    await racing.initial;
    const pending = racing.restorePoint(`${racing.owner}:1`).then(() => false, () => true);
    racing.changed(); releaseRead(point);
    ok(await pending && !committed, "Late snapshot read cannot overwrite newer edits"); racing.dispose();
    let finishPrepare;
    const staging = createRecovery({ store: mocked, capture: async () => null, busy: () => false, hasWork: () => false,
        prepare: () => new Promise((resolve) => { finishPrepare = resolve; }) });
    await staging.initial;
    const staged = staging.restorePoint(`${staging.owner}:1`).then(() => false, () => true);
    await new Promise((resolve) => setTimeout(resolve, 0));
    staging.changed(); finishPrepare({ commit() { committed = true; }, dispose() { disposed = true; } });
    ok(await staged && !committed && disposed, "Late staged restore discarded, newer state preserved"); staging.dispose();
    checks.push("controller: capture/read/prepare races, quota fallback, and disposal without publishing");
    await store.configure(false, true);
    return checks;
}
