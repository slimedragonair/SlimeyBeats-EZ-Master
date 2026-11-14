import init, {
    BrowserStudio,
    BrowserStemArchive,
    plan_stem_files,
} from "./pkg/dsp.js";
const ready = init();
function liveStems(candidate, previous, wasm) {
    const previousKinds = new Set(previous ? JSON.parse(previous.info()).stems : []);
    return JSON.parse(candidate.info()).stems.map((kind) => {
        const pointer = candidate.stem_ptr(kind), length = candidate.stem_len(kind);
        const unchanged = previousKinds.has(kind) && pointer === previous.stem_ptr(kind) && length === previous.stem_len(kind);
        return { kind, length, identity: `${pointer}:${length}`,
            ...(unchanged ? {} : { samples: new Float32Array(wasm.memory.buffer, pointer, length).slice() }),
        };
    });
}
let studio;
let proposal;
let archive;
let archiveId;
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
            case "import-plan":
                result = JSON.parse(plan_stem_files(JSON.stringify(m.files)));
                break;
            case "archive-open": {
                const next = new BrowserStemArchive(m.bytes);
                archive?.free();
                archive = next;
                archiveId = m.id;
                result = {
                    plan: JSON.parse(archive.info()),
                    archive_id: archiveId,
                };
                break;
            }
            case "archive-read":
                if (!archive || m.archive_id !== archiveId)
                    throw new Error(
                        "The pending ZIP changed. Choose the archive again.",
                    );
                result = { bytes: archive.extract_audio(m.index) };
                break;
            case "archive-close":
                archive?.free();
                archive = null;
                archiveId = null;
                result = true;
                break;
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
                for (const stem of m.stems)
                    if (stem.display_name !== undefined)
                        candidate.set_stem_display_name(stem.kind, stem.display_name);
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
                const info = JSON.parse(studio.info());
                const stems = [];
                for (const kind of info.stems) {
                    const name = `${kind}.wav`;
                    result.files.push({ name, bytes: studio.stem_wav(kind) });
                    stems.push({
                        file: name,
                        kind,
                        display_name: info.stem_names?.[kind] ?? kind,
                    });
                }
                result.manifest = { format: "wav32f", stems };
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
                stems: liveStems(candidate, studio, wasm),
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
        for (const stem of result?.stems || [])
            if (stem.samples) transfer.push(stem.samples.buffer);
        if (result?.bytes) transfer.push(result.bytes.buffer);
        for (const file of result?.files || [])
            transfer.push(file.bytes.buffer);
        self.postMessage({ id: m.id, result }, transfer);
    } catch (error) {
        candidate?.free();
        self.postMessage({ id: m.id, error: String(error) });
    }
};
