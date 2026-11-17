import init, { BrowserArrangement, BrowserArrangementArchive, defaults, measure_encoded_audio } from "./pkg/dsp.js";

const ready = init();
let session;
const archives = new Map();
const integer = value => {
    if (!Number.isSafeInteger(value) || value < 0) throw new Error("Expected nonnegative safe frame/revision integer");
    return BigInt(value);
};
// All engine construction, seeking, EQ preparation and retirement run here,
// outside the audio callback. Messages execute in arrival order after init.
let queue = Promise.resolve();
self.onmessage = ({ data: m }) => {
    queue = queue.then(async () => {
        try {
            const wasm = await ready;
            let value, transfer = [];
            if (m.action === "init") {
                const next = new BrowserArrangement(m.sample_rate, m.channels, m.epoch);
                session?.free(); session = next;
                for (const archive of archives.values()) archive.free();
                archives.clear();
                value = JSON.parse(session.read());
                const settings = JSON.parse(defaults());
                value.defaults = { bands: settings.settings.manual_eq, profiles: settings.profiles, import_limits: settings.import_limits };
            } else {
                if (!session) throw new Error("Initialize an arrangement first");
                switch (m.action) {
                    case "archive-open": {
                        if (archives.has(m.archive_id)) throw new Error("Archive review ID already exists");
                        const archive = new BrowserArrangementArchive(m.bytes);
                        archives.set(m.archive_id, archive); value = JSON.parse(archive.info()); break;
                    }
                    case "archive-read": {
                        const archive = archives.get(m.archive_id);
                        if (!archive) throw new Error("This ZIP review expired");
                        value = archive.extract_audio(m.index); transfer = [value.buffer]; break;
                    }
                    case "archive-close": archives.get(m.archive_id)?.free(); archives.delete(m.archive_id); value = true; break;
                    case "read": value = JSON.parse(session.read()); break;
                    case "stage-wav": session.stage_wav(m.asset_id, m.name, m.bytes); value = true; break;
                    case "stage-asset": session.stage_asset(m.asset_id, m.name, m.samples, m.sample_rate, m.channels); value = true; break;
                    case "commit-import": value = JSON.parse(session.commit_import(integer(m.expectedRevision), JSON.stringify(m.edits))); break;
                    case "discard-import": session.discard_import(); value = true; break;
                    case "edit": value = JSON.parse(session.edit(integer(m.expectedRevision), JSON.stringify(m.edits))); break;
                    case "history": value = JSON.parse(session.history(integer(m.expectedRevision), m.redo)); break;
                    case "waveform": value = JSON.parse(session.waveform(m.asset_id, integer(m.start_frame), integer(m.end_frame), m.bins)); break;
                    case "eq-response": value = JSON.parse(session.eq_response(JSON.stringify(m.effect))); break;
                    case "suggest-eq": value = JSON.parse(session.suggest_eq(integer(m.expectedRevision), JSON.stringify(m.target), m.profile || "neutral", m.exclude_eq_id || "")); break;
                    case "prepare": value = JSON.parse(session.prepare(integer(m.expectedRevision), m.side, integer(m.frame))); break;
                    case "playback-info": value = JSON.parse(session.playback_info()); break;
                    case "render-block": {
                        const frames = session.render_block(2048);
                        const length = session.block_len();
                        const buffer = m.buffer?.byteLength >= length * 4 ? m.buffer : new ArrayBuffer(length * 4);
                        new Float32Array(buffer, 0, length).set(new Float32Array(wasm.memory.buffer, session.block_ptr(), length));
                        value = { buffer, frames }; transfer = [buffer]; break;
                    }
                    case "export-wav": value = session.export_wav(integer(m.expectedRevision), m.format || "wav24", !!m.dither); transfer = [value.buffer]; break;
                    case "asset-wav": value = session.asset_wav(m.asset_id); transfer = [value.buffer]; break;
                    case "measure-encoded": value = JSON.parse(measure_encoded_audio(m.samples, m.rate, m.channels, m.profile || "neutral")); break;
                    case "project-manifest": value = JSON.parse(session.project_manifest(integer(m.expectedRevision))); break;
                    case "open-project": {
                        value = JSON.parse(session.open_project_zip(integer(m.expectedRevision), m.bytes, m.epoch));
                        for (const archive of archives.values()) archive.free(); archives.clear(); break;
                    }
                    default: throw new Error(`Unknown arrangement worker action: ${m.action}`);
                }
            }
            self.postMessage({ id: m.id, value }, transfer);
        } catch (error) { self.postMessage({ id: m.id, error: error?.message || String(error) }); }
    });
};
