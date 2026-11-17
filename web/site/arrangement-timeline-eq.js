import { frameSeconds, secondsToFrame } from "./arrangement-timeline-state.js";
import { finiteDraft } from "./arrangement-timeline-commands.js";
import { eqTargetKey, matchingEffects, newScopedEq, ArrangementEqProposal } from "./arrangement-timeline-eq-state.js";
import { paintArrangementEq } from "./arrangement-timeline-eq-curves.js";
import { setText, setOptions, setInputValue } from "./arrangement-timeline-fields.js";

const BAND_NAMES = ["Sub", "Bass", "Warmth", "Body", "Presence", "Clarity", "Air"];
const PROPERTIES = { gain: ["eq-gain", -18, 18], freq: ["eq-frequency", 20, 20000], q: ["eq-q", 0.1, 18] };

export function wireArrangementEq(host, { model, getState, adapter, defaults, command, run, repaint, notice }) {
    const $ = name => host.querySelector(`[data-arr="${name}"]`), proposal = new ArrangementEqProposal();
    const entries = new Map(), choices = new Map();
    let band = 0, draftRevision = 0, curveRequest = 0, lastCurveKey = "", currentCurve = null, pendingCurve = null, rangeDraft = false, rangeError = "", disposed = false;
    let lastGraph = "", noteContext = "", lastSelection = "";
    setOptions($("eq-band"), BAND_NAMES.map((label, index) => [String(index), `${index + 1} · ${label}`]), "0");
    function selected() {
        const target = model().eqTarget(), key = eqTargetKey(target);
        if (!key) return { target, key, error: target.error };
        const saved = matchingEffects(model().arrangement?.eq || [], target);
        let choice = choices.has(key) ? choices.get(key) : saved[0]?.id || "";
        if (choice && !saved.some(effect => effect.id === choice)) choice = saved[0]?.id || "";
        choices.set(key, choice);
        const current = saved.find(effect => effect.id === choice), entryKey = `${key}:${choice || "new"}`;
        let entry = entries.get(entryKey);
        if (!entry || (!entry.dirty && entry.base !== JSON.stringify(current || null))) {
            entry = { effect: current ? structuredClone(current) : newScopedEq(crypto.randomUUID(), target, defaults()),
                base: JSON.stringify(current || null), raw: new Map(), dirty: false };
            entries.set(entryKey, entry);
        }
        return { target, key, saved, choice, current, entryKey, entry,
            conflict: entry.dirty && entry.base !== JSON.stringify(current || null) };
    }
    function readEffect(value, ignoreBands = false) {
        if (value.conflict) throw new Error("The saved EQ changed. Keep this draft or reselect the effect to reload before applying.");
        const effect = structuredClone(value.entry.effect);
        for (const [key, raw] of value.entry.raw) {
            if (key === "output") effect.output_gain_db = finiteDraft(raw, "EQ output", -24, 12);
            else if (key === "bypass") effect.bypass = raw;
            else if (!ignoreBands) {
                const [index, property] = key.split(":");
                effect.bands[index][property] = property === "on" ? raw : property === "type" ? raw
                    : finiteDraft(raw, `${BAND_NAMES[index]} ${property}`, PROPERTIES[property][1], PROPERTIES[property][2]);
            }
        }
        return effect;
    }
    function context(value) { return JSON.stringify([getState().revision, value.key, value.choice, draftRevision]); }
    function graphContext(value) { return JSON.stringify([getState().revision, value.key, value.choice]); }
    function clearNotes() { noteContext = ""; setText($("eq-notes"), ""); }
    function draw() { paintArrangementEq($("eq-plot"), currentCurve, pendingCurve, model().rate); }
    function checkedCurve(value) {
        if (!Array.isArray(value?.frequencies) || !Array.isArray(value?.gain_db) || value.frequencies.length < 2 ||
            value.frequencies.length !== value.gain_db.length || !value.frequencies.every(value => Number.isFinite(value) && value > 0) || !value.gain_db.every(Number.isFinite))
            throw new Error("The EQ processor returned an invalid response curve.");
        return value;
    }
    async function curves(value) {
        if (!value.entry || typeof adapter.eqResponse !== "function") return;
        let draft;
        try { draft = readEffect(value); } catch { return; }
        const candidate = proposal.current(context(value));
        const edited = candidate ? proposal.effect(draft, context(value)) : value.entry.dirty ? draft : null;
        const base = value.current || newScopedEq(value.entry.effect.id, value.target, defaults());
        const key = JSON.stringify([getState().revision, base, edited]);
        if (key === lastCurveKey) { draw(); return; }
        lastCurveKey = key;
        const token = ++curveRequest;
        try {
            const [current, pending] = await Promise.all([adapter.eqResponse(base), edited ? adapter.eqResponse(edited) : null]);
            if (token !== curveRequest || disposed) return;
            currentCurve = checkedCurve(current); pendingCurve = pending ? checkedCurve(pending) : null; draw();
        } catch (error) {
            if (token === curveRequest && !disposed) { currentCurve = pendingCurve = null; draw(); setText($("eq-notes"), error.message || String(error)); }
        }
    }
    function editRaw(property, raw) {
        try {
            const value = selected(); if (!value.entry) return;
            value.entry.raw.set(property, raw); value.entry.dirty = true;
            ++draftRevision; proposal.clear(); pendingCurve = null;
            clearNotes();
            setText($("eq-note"), "EQ changes are pending. Update EQ preview to hear them.");
            sync();
        } catch (error) { notice(error.message || String(error), true); }
    }
    for (const [property, [name]] of Object.entries(PROPERTIES)) $(name).oninput = () => editRaw(`${band}:${property}`, $(name).value);
    $("eq-shape").onchange = () => editRaw(`${band}:type`, $("eq-shape").value);
    $("eq-enabled").onchange = () => editRaw(`${band}:on`, $("eq-enabled").checked);
    $("eq-output").oninput = () => editRaw("output", $("eq-output").value);
    $("eq-bypass").onchange = () => editRaw("bypass", $("eq-bypass").checked);
    $("eq-band").onchange = () => { band = Number($("eq-band").value); sync(); };
    $("eq-effect").onchange = () => {
        const value = selected();
        choices.set(value.key, $("eq-effect").value);
        ++draftRevision; proposal.clear(); currentCurve = pendingCurve = null; lastCurveKey = ""; sync();
    };
    function changeTarget() {
        model().scope = $("eq-scope").value; model().rangeTarget = $("eq-range-target").value;
        ++draftRevision; proposal.clear(); pendingCurve = null; lastCurveKey = ""; sync();
    }
    $("eq-scope").onchange = changeTarget; $("eq-range-target").onchange = changeTarget;
    function commitRange() {
        try {
            model().setRange(secondsToFrame($("range-start").value, model().rate), secondsToFrame($("range-end").value, model().rate));
            rangeDraft = false; rangeError = ""; ++draftRevision; proposal.clear(); repaint();
        } catch (error) { rangeError = error.message || String(error); sync(); }
    }
    for (const name of ["range-start", "range-end"]) {
        $(name).oninput = () => {
            try { rangeDraft = secondsToFrame($("range-start").value, model().rate) !== model().start || secondsToFrame($("range-end").value, model().rate) !== model().end; }
            catch { rangeDraft = true; }
            rangeError = rangeDraft ? "Finish both range times before applying EQ." : "";
            ++draftRevision; proposal.clear(); pendingCurve = null; clearNotes(); sync(); };
        $(name).onchange = commitRange;
    }
    async function applyEffect(effect, value) {
        const notes = $("eq-notes").textContent;
        if (await command({ kind: "set_eq", effect })) {
            entries.delete(value.entryKey); choices.set(value.key, effect.id);
            proposal.clear(); ++draftRevision; lastCurveKey = "";
            setText($("eq-note"), "EQ applied to this target. Listen on B, or Undo to compare the earlier revision.");
            setText($("eq-notes"), notes); noteContext = graphContext(selected()); sync();
        }
    }
    $("eq-reload").onclick = () => {
        if ($("eq-reload").disabled) return;
        const value = selected(); entries.delete(value.entryKey); proposal.clear(); ++draftRevision;
        clearNotes(); lastCurveKey = ""; sync();
    };
    $("eq-update").onclick = async () => {
        if ($("eq-update").disabled) return;
        try { const value = selected(); await applyEffect(readEffect(value), value); }
        catch (error) { notice(error.message || String(error), true); }
    };
    $("eq-remove").onclick = async () => {
        if ($("eq-remove").disabled) return;
        const value = selected();
        if (value.current && await command({ kind: "remove_eq", eq_id: value.current.id })) {
            entries.delete(value.entryKey); choices.delete(value.key); proposal.clear(); ++draftRevision; lastCurveKey = ""; sync();
        }
    };
    $("eq-suggest").onclick = () => {
        if ($("eq-suggest").disabled) return;
        return run("Measuring the selected EQ target…", async signal => {
            const value = selected(), token = context(value);
            const next = await adapter.suggestEq({ scope: value.target.scope, range: value.target.range }, {
                expectedRevision: getState().revision, exclude_eq_id: value.current?.id || "", signal,
            });
            if (token !== context(selected()) || disposed) return;
            proposal.stage(next, token);
            noteContext = graphContext(value);
            const high = next.high_frequency;
            setText($("eq-note"), high?.excess_candidate ? "Possible upper-treble excess. Inspect the dashed proposal before applying it." : "A gentle EQ proposal is ready; it has not been applied.");
            setText($("eq-notes"), `${next.notes.join(" ")} The plot shows this EQ effect; other track, clip or mix effects can add to it.`);
            $("eq-details").open = false; lastCurveKey = ""; sync();
        });
    };
    $("eq-apply-proposal").onclick = async () => {
        if ($("eq-apply-proposal").disabled) return;
        try {
            const value = selected(), effect = proposal.effect(readEffect(value, true), context(value));
            await applyEffect(effect, value);
        } catch (error) { notice(error.message || String(error), true); }
    };
    function sync() {
        if (disposed) return;
        const state = model(), { busy, capabilities = {} } = getState();
        $("eq").hidden = !capabilities.scoped_eq;
        $("range-target-label").hidden = state.scope !== "range";
        const selection = JSON.stringify([state.start, state.end]);
        if (selection !== lastSelection && ![$("range-start"), $("range-end")].includes(document.activeElement)) {
            rangeDraft = false; rangeError = "";
        }
        lastSelection = selection;
        if (!rangeDraft) { setInputValue($("range-start"), frameSeconds(state.start, state.rate)); setInputValue($("range-end"), frameSeconds(state.end, state.rate)); }
        let value;
        try { value = selected(); }
        catch (error) { value = { error: error.message || String(error) }; }
        const error = (state.scope === "range" ? rangeError : "") || value.error;
        const graph = graphContext(value);
        if (graph !== lastGraph) { ++curveRequest; currentCurve = pendingCurve = null; lastCurveKey = ""; lastGraph = graph; }
        if (noteContext && noteContext !== graph) {
            clearNotes(); setText($("eq-note"), "The EQ target or revision changed. Review a new proposal before applying.");
        }
        setText($("eq-target"), error || value.target.label);
        for (const name of ["eq-scope", "eq-range-target", "range-start", "range-end"]) $(name).disabled = busy;
        const fields = ["eq-effect", "eq-band", "eq-gain", "eq-frequency", "eq-q", "eq-shape", "eq-output", "eq-enabled", "eq-bypass"];
        for (const name of fields) $(name).disabled = busy || !!error || !capabilities.scoped_eq;
        let invalid = !!error;
        if (value.entry) {
            setOptions($("eq-effect"), [["", "New EQ for this target"], ...value.saved.map((effect, index) => [effect.id, `Saved EQ ${index + 1}${effect.bypass ? " · bypassed" : ""}`])], value.choice);
            const current = value.entry.effect.bands[band];
            for (const [property, [name]] of Object.entries(PROPERTIES)) setInputValue($(name), value.entry.raw.get(`${band}:${property}`) ?? String(current[property]));
            setInputValue($("eq-shape"), value.entry.raw.get(`${band}:type`) ?? current.type);
            setInputValue($("eq-enabled"), value.entry.raw.get(`${band}:on`) ?? current.on);
            setInputValue($("eq-output"), value.entry.raw.get("output") ?? String(value.entry.effect.output_gain_db));
            setInputValue($("eq-bypass"), value.entry.raw.get("bypass") ?? value.entry.effect.bypass);
            try { readEffect(value); } catch { invalid = true; }
            void curves(value);
        }
        const hasProposal = value.entry && !!proposal.current(context(value));
        $("eq-apply-proposal").hidden = !hasProposal;
        $("eq-apply-proposal").disabled = busy || invalid;
        $("eq-update").disabled = busy || invalid || !value.entry?.dirty || !capabilities.scoped_eq;
        $("eq-reload").disabled = busy || !value.entry?.dirty;
        $("eq-remove").disabled = busy || !!error || !value.current;
        $("eq-suggest").disabled = busy || invalid || !capabilities.suggest_eq || !state.frames;
        if (!hasProposal && !value.entry?.dirty) pendingCurve = null;
        draw();
    }
    const resize = new ResizeObserver(draw); resize.observe($("eq-plot"));
    return { sync,
        capture: () => structuredClone({ entries, choices, band, draftRevision, rangeDraft, rangeError,
            rangeStart: $("range-start").value, rangeEnd: $("range-end").value }),
        restore(value) {
            entries.clear(); choices.clear();
            for (const [key, entry] of value.entries) entries.set(key, entry);
            for (const [key, choice] of value.choices) choices.set(key, choice);
            band = value.band; draftRevision = value.draftRevision + 1;
            rangeDraft = value.rangeDraft; rangeError = value.rangeError;
            lastSelection = JSON.stringify([model().start, model().end]);
            $("eq-band").value = String(band); $("eq-scope").value = model().scope; $("eq-range-target").value = model().rangeTarget;
            $("range-start").value = value.rangeStart; $("range-end").value = value.rangeEnd;
            proposal.clear(); clearNotes(); ++curveRequest; currentCurve = pendingCurve = null; lastCurveKey = ""; sync();
            setText($("eq-note"), "Applied EQ and pending fields were continued on this engine. Measure a new proposal before applying one.");
        },
        reset() { entries.clear(); choices.clear(); proposal.clear(); ++draftRevision; ++curveRequest;
        currentCurve = pendingCurve = null; lastCurveKey = ""; rangeDraft = false; rangeError = ""; sync(); },
        dispose() { disposed = true; ++curveRequest; resize.disconnect(); entries.clear(); choices.clear(); proposal.clear(); } };
}
