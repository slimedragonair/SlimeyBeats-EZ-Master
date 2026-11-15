import { assert, equal, checkEdit, wave, base64 } from "./region_edit_pcm_checks.js";

// Observe the actual application client/playback. DSP and proposal handling are untouched.
export async function installUiProbe() {
    const resource = name => {
        const item = performance.getEntriesByType("resource").find(e => new URL(e.name).pathname.endsWith(`/${name}`));
        assert(item, `Application did not load ${name}`);
        return item.name;
    };
    const { StudioClient } = await import(resource("studio-client.js"));
    const { StudioPlayback } = await import(resource("studio-player.js"));
    const probe = window.regionUi = { calls: [], replies: [], errors: [] };
    probe.layout = () => {
        const shape = selector => {
            const el = document.querySelector(selector), rect = el.getBoundingClientRect(), style = getComputedStyle(el);
            return { left: rect.left, right: rect.right, width: rect.width, scroll_width: el.scrollWidth,
                client_width: el.clientWidth, display: style.display, overflow_x: style.overflowX,
                grid_columns: style.gridTemplateColumns, appearance: style.appearance, text_overflow: style.textOverflow };
        };
        return { viewport: innerWidth, document: document.documentElement.scrollWidth,
            client_width: document.documentElement.clientWidth, body_scroll_width: document.body.scrollWidth,
            body_client_width: document.body.clientWidth, scroll_x: scrollX,
            stem_label: shape('.s-time-stem'), stem_select: shape('#s-editor-stem') };
    };
    const call = StudioClient.prototype.call, setAudio = StudioPlayback.prototype.setAudio;
    StudioClient.prototype.call = async function(action, values = {}, ...rest) {
        probe.client = this;
        probe.calls.push({ action, ...(action === "region" ? { kind: values.kind, edit: structuredClone(values.edit) } : {}) });
        try {
            const result = await call.call(this, action, values, ...rest);
            probe.replies.push({ action, ...(result?.proposal ? { proposal: result.proposal } : {}) });
            return result;
        } catch (error) { probe.errors.push({ action, error: String(error) }); throw error; }
    };
    StudioPlayback.prototype.setAudio = function(...args) {
        probe.playback = this;
        return setAudio.apply(this, args);
    };
    probe.snapshot = async () => {
        assert(probe.client && probe.playback, "UI did not publish a real Studio revision");
        const info = (await probe.client.call("init")).info;
        const bank = await probe.client.call("stem-export");
        const preview = (await probe.client.call("working")).bytes;
        return { info, bank: bank.files.map(f => ({ name: f.name, bytes: f.bytes })), preview };
    };
    probe.remember = async () => { probe.before = await probe.snapshot(); };
    probe.unchanged = async () => {
        const after = await probe.snapshot();
        equal(after.preview, probe.before.preview, "UI: retained preview");
        assert(after.bank.length === probe.before.bank.length, "UI: bank size changed");
        for (const file of after.bank) equal(file.bytes, probe.before.bank.find(f => f.name === file.name).bytes, `UI unchanged ${file.name}`);
    };
    probe.checkEdit = async () => {
        const action = probe.calls.findLast(c => c.action === "region"), after = await probe.snapshot();
        assert(action, "Tool click issued no region worker request");
        let result;
        for (const file of after.bank) {
            const before = probe.before.bank.find(f => f.name === file.name);
            assert(before, "UI changed the stem identities");
            if (file.name === `${action.kind}.wav`) {
                const original = wave(before.bytes), edited = wave(file.bytes);
                assert(edited.rate === original.rate && edited.channels === original.channels, "UI edited stem layout changed");
                result = checkEdit(edited.samples, original.samples, action.edit, original.channels, original.rate, `UI ${action.edit.operation.kind}`);
            } else equal(file.bytes, before.bytes, `UI untouched ${file.name}`);
        }
        assert(result, "UI edited a stem that is absent from the bank");
        assert(after.info.preview_is_mix === true, "Region UI did not publish the edited bank's mix");
        probe.edited = after;
        return { kind: action.kind, operation: action.edit.operation.kind, ...result };
    };
    probe.sameEdited = async () => {
        const after = await probe.snapshot();
        equal(after.preview, probe.edited.preview, "UI redo preview");
        after.bank.forEach(f => equal(f.bytes, probe.edited.bank.find(old => old.name === f.name).bytes, `UI redo ${f.name}`));
    };
    probe.encodedBank = async () => Object.fromEntries((await probe.snapshot()).bank.map(f => [f.name, base64(f.bytes)]));
    probe.mutations = () => probe.calls.filter(c => ["region", "mix", "history", "voice", "repair", "replace"].includes(c.action)).length;
    return true;
}
