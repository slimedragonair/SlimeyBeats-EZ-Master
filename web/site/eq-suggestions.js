const DEFAULT_NOTE = "Suggestions are starting points, gently influenced by your tonal profile. Review a curve before applying it, then trust your ears.";

function sameContext(a, b) {
    return a.source === b.source && a.original === b.original &&
        a.trackRevision === b.trackRevision && a.eqRevision === b.eqRevision && a.profile === b.profile;
}

function copyProposal(value) {
    if (!value || !Array.isArray(value.bands) || value.bands.length !== 7 ||
        value.bands.some(band => !band || typeof band.on !== "boolean" ||
            ![band.freq, band.gain, band.q].every(Number.isFinite) ||
            !["bell", "low_shelf", "high_shelf", "low_cut", "high_cut"].includes(band.type)) ||
        !Array.isArray(value.notes) || value.notes.some(note => typeof note !== "string"))
        throw new Error("The EQ suggestion could not be read.");
    return structuredClone(value);
}

function summary(proposal) {
    const high = proposal.high_frequency;
    if (high?.excess_candidate && [high.band_low_hz, high.band_high_hz].every(Number.isFinite))
        return `Possible excess at ${high.band_low_hz / 1000}–${Number((high.band_high_hz / 1000).toFixed(1))} kHz. Proposed Air cut: ${proposal.bands[6].gain.toFixed(2)} dB.`;
    return high ? "Review these gentle tone changes; no extra upper-treble cut was identified."
        : "Review these gentle tone changes; the extra upper-treble check is unavailable.";
}

// Reuse the existing action: proposing is read-only, and a second click accepts it.
export function wireEqSuggestions({ button, note, details, explanation, getContext, request, apply, runJob, onStatus }) {
    let pending = null;
    let appliedContext = null;
    function setNote(text) {
        if (note.textContent !== text) note.textContent = text;
    }
    function setExplanation(text) {
        if (explanation.textContent !== text) explanation.textContent = text;
        details.open = false;
    }
    function sync() {
        const shownContext = pending?.context || appliedContext;
        if (shownContext && !sameContext(shownContext, getContext())) {
            pending = null;
            appliedContext = null;
            setNote("Your audio, profile or EQ changed. Suggest EQ again to review a new curve. Your current EQ was kept.");
            setExplanation(DEFAULT_NOTE);
        }
        const label = pending ? "Apply curve" : "✦ Suggest EQ";
        if (button.textContent !== label) button.textContent = label;
        button.title = pending
            ? "Replace the current EQ bands with this proposal. No audio is exported."
            : "Propose small tone changes without changing your EQ or exporting audio.";
    }
    function clear() {
        pending = null;
        appliedContext = null;
        setNote(DEFAULT_NOTE);
        setExplanation("A proposal leaves your EQ unchanged. Apply curve replaces its seven bands, without exporting audio. Listen on B · Master; A compares the full master with the original mix.");
        sync();
    }
    button.onclick = async () => {
        if (button.disabled || !getContext().source) return;
        const hadPending = !!pending;
        sync();
        if (hadPending && !pending) return;
        if (pending) {
            const proposal = pending.proposal;
            pending = null;
            try {
                apply(structuredClone(proposal.bands));
                appliedContext = getContext();
                setNote(`${summary(proposal)} Applied. Play B to hear it; expand for details.`);
                setExplanation(`${proposal.notes.join(" ")} Choose B · Master and Play to hear your EQ. A compares with the original mix.`);
                onStatus("Suggested EQ applied. Listen on B, adjust freely, and export when ready.");
            } catch (error) {
                onStatus(error.message || String(error), true);
            }
            sync();
            return;
        }
        await runJob(async ensureCurrent => {
            const context = getContext();
            let proposal;
            try {
                proposal = copyProposal(await request(ensureCurrent));
                ensureCurrent();
            } catch (error) {
                if (error.name !== "AbortError" && sameContext(context, getContext())) {
                    setNote("No EQ proposal is available. Your current EQ was kept; expand for details.");
                    setExplanation(error.message || String(error));
                }
                throw error;
            }
            if (!sameContext(context, getContext())) {
                setNote("Your audio, profile or EQ changed while measuring. Suggest EQ again; your current EQ was kept.");
                return;
            }
            pending = { context, proposal };
            appliedContext = null;
            const basis = context.source === context.original ? "the original mix" : "the automatic master, before your EQ";
            setNote(`${summary(proposal)} Not applied; plot shows current EQ. Expand for why.`);
            setExplanation(`Proposed from ${basis}. ${proposal.notes.join(" ")} Apply curve replaces the current seven EQ bands. No audio is exported.`);
            sync();
            onStatus("EQ suggestion ready. Review the note, then Apply curve to hear it on B.");
        });
    };
    clear();
    return { sync, clear };
}
