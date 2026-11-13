import init, { BrowserStudio } from "./pkg/dsp.js";
const ready = init();
let studio;
let proposal;
self.onmessage = async ({ data: m }) => {
    let candidate;
    try {
        const wasm = await ready;
        let result;
        let audio = false;
        const mutating = [
            "load",
            "stems",
            "mix",
            "voice",
            "repair",
            "replace",
            "history",
        ].includes(m.action);
        if (mutating && proposal)
            throw new Error("Finish the previous Studio revision first");
        if (mutating)
            candidate = ["load", "stems"].includes(m.action)
                ? new BrowserStudio()
                : studio.fork();
        switch (m.action) {
            case "init":
                studio ??= new BrowserStudio();
                result = {
                    info: JSON.parse(studio.info()),
                    defaults: JSON.parse(BrowserStudio.parameter_defaults()),
                };
                break;
            case "load": {
                candidate.load(m.samples, m.rate, m.channels);
                audio = true;
                break;
            }
            case "stems": {
                for (const stem of m.stems)
                    candidate.stage_stem(
                        stem.kind,
                        stem.samples,
                        stem.rate,
                        stem.channels,
                    );
                candidate.commit_stems();
                audio = true;
                break;
            }
            case "mix":
                candidate.mix(JSON.stringify(m.settings));
                audio = true;
                break;
            case "voice":
                candidate.voice(m.target, JSON.stringify(m.settings));
                audio = true;
                break;
            case "repair":
                candidate.repair(m.target, JSON.stringify(m.selection));
                audio = true;
                break;
            case "replace":
                candidate.replace_stem(m.kind);
                audio = true;
                break;
            case "history":
                candidate.history(m.redo);
                audio = true;
                break;
            case "spectrogram":
                result = JSON.parse(studio.spectrogram(m.target));
                break;
            case "analyze":
                result = JSON.parse(studio.analyze(m.target));
                break;
            case "export":
                result = { bytes: studio.export(m.format) };
                break;
            case "working":
                result = { bytes: studio.current_wav32() };
                break;
            case "stem-export": {
                result = { files: [] };
                for (const kind of JSON.parse(studio.info()).stems)
                    result.files.push({
                        name: `${kind}.wav`,
                        bytes: studio.stem_wav(kind),
                    });
                break;
            }
            case "clear":
                proposal?.value.free();
                proposal = null;
                studio?.free();
                studio = new BrowserStudio();
                result = { info: JSON.parse(studio.info()) };
                break;
            case "commit":
                if (!proposal || proposal.id !== m.proposal)
                    throw new Error("Studio proposal is stale");
                studio?.free();
                studio = proposal.value;
                proposal = null;
                result = true;
                break;
            case "discard":
                if (proposal?.id === m.proposal) {
                    proposal.value.free();
                    proposal = null;
                }
                result = true;
                break;
            default:
                throw new Error("Unknown Studio DSP operation");
        }
        if (audio)
            result = {
                info: JSON.parse(candidate.info()),
                samples: new Float32Array(
                    wasm.memory.buffer,
                    candidate.preview_ptr(),
                    candidate.preview_len(),
                ).slice(),
                before: new Float32Array(
                    wasm.memory.buffer,
                    candidate.before_ptr(),
                    candidate.before_len(),
                ).slice(),
            };
        if (mutating) {
            proposal = { id: m.id, value: candidate };
            candidate = null;
            result.proposal = m.id;
        }
        const transfer = [];
        if (result?.samples) transfer.push(result.samples.buffer);
        if (result?.before) transfer.push(result.before.buffer);
        if (result?.bytes) transfer.push(result.bytes.buffer);
        for (const file of result?.files || [])
            transfer.push(file.bytes.buffer);
        self.postMessage({ id: m.id, result }, transfer);
    } catch (error) {
        candidate?.free();
        self.postMessage({ id: m.id, error: String(error) });
    }
};
