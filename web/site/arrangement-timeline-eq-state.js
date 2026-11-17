export function eqTargetKey(target) {
    if (!target?.scope || target.error) return "";
    const { scope, range } = target;
    return JSON.stringify([scope.kind, scope.lane_id || scope.clip_id || null, range?.start_frame ?? null, range?.end_frame ?? null]);
}

export function matchingEffects(effects, target) {
    const key = eqTargetKey(target);
    return key ? effects.filter(effect => eqTargetKey(effect) === key) : [];
}

export function newScopedEq(id, target, bands) {
    if (!eqTargetKey(target)) throw new Error(target?.error || "Choose an EQ target.");
    if (!Array.isArray(bands) || bands.length !== 7) throw new Error("Core EQ defaults are not ready.");
    return { id, scope: structuredClone(target.scope), range: target.range ? { ...target.range } : null,
        bands: structuredClone(bands), output_gain_db: 0, bypass: false };
}

export class ArrangementEqProposal {
    constructor() { this.clear(); }
    clear() { this.value = null; this.context = null; }
    stage(proposal, context) {
        if (!proposal || !Array.isArray(proposal.bands) || proposal.bands.length !== 7 ||
            proposal.bands.some(band => !band || ![band.freq, band.gain, band.q].every(Number.isFinite) || typeof band.on !== "boolean") ||
            !Array.isArray(proposal.notes) || proposal.notes.some(note => typeof note !== "string"))
            throw new Error("The scoped EQ proposal is incomplete. Your existing EQ was kept.");
        this.value = structuredClone(proposal); this.context = context;
    }
    current(context) {
        if (this.context !== context) this.clear();
        return this.value;
    }
    effect(draft, context) {
        const proposal = this.current(context);
        if (!proposal) throw new Error("The EQ target or settings changed. Suggest again before applying.");
        return { ...structuredClone(draft), bands: structuredClone(proposal.bands) };
    }
}
