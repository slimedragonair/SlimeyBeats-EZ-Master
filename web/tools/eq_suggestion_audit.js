window.eqSuggestionAudit = { actions: [], proposals: [], hold: false, held: null, analysers: [] };
const NativeWorker = window.Worker;
window.Worker = class extends NativeWorker {
    constructor(...args) {
        super(...args);
        const requests = new Map(), post = this.postMessage.bind(this);
        this.postMessage = (message, ...rest) => {
            if (message.action) {
                requests.set(message.id, message.action);
                window.eqSuggestionAudit.actions.push(message.action);
            }
            return post(message, ...rest);
        };
        this.addEventListener("message", event => {
            if (event.data.progress !== undefined) return;
            const action = requests.get(event.data.id);
            requests.delete(event.data.id);
            if (action !== "suggest_detailed") return;
            const audit = window.eqSuggestionAudit;
            if (event.data.result) audit.proposals.push(event.data.result);
            if (audit.hold) {
                event.stopImmediatePropagation();
                audit.held = { worker: this, data: event.data };
            }
        });
    }
};
window.eqSuggestionAudit.release = () => {
    const audit = window.eqSuggestionAudit, held = audit.held;
    audit.hold = false;
    audit.held = null;
    held.worker.dispatchEvent(new MessageEvent("message", { data: held.data }));
};
const createAnalyser = AudioContext.prototype.createAnalyser;
AudioContext.prototype.createAnalyser = function (...args) {
    const analyser = createAnalyser.apply(this, args);
    window.eqSuggestionAudit.analysers.push(analyser);
    return analyser;
};
window.eqSuggestionAudit.peak = () => {
    let peak = 0;
    for (const analyser of window.eqSuggestionAudit.analysers) {
        const samples = new Float32Array(analyser.fftSize);
        analyser.getFloatTimeDomainData(samples);
        for (const sample of samples) peak = Math.max(peak, Math.abs(sample));
    }
    return peak;
};
