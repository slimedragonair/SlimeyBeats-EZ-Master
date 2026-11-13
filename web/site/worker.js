import init, { Session, defaults, response } from "./pkg/dsp.js";
let session;
const ready = init();
self.onmessage = async ({ data: m }) => {
    try {
        const wasm = await ready;
        let result;
        switch (m.action) {
            case "init":
                result = JSON.parse(defaults());
                break;
            case "load":
                session?.free();
                session = new Session(m.samples, m.rate, m.channels);
                result = true;
                break;
            case "reference":
                session.set_reference(m.samples, m.rate, m.channels);
                result = true;
                break;
            case "analyze":
                result = JSON.parse(session.analyze(m.settings));
                break;
            case "master": {
                result = JSON.parse(
                    session.master(m.settings, (value, text) =>
                        self.postMessage({ id: m.id, progress: value, text }),
                    ),
                );
                // Copy once into the transferable buffer, without a second full
                // preview allocation in the WASM heap. Read before any mutation.
                result.samples = new Float32Array(
                    wasm.memory.buffer,
                    session.preview_ptr(),
                    session.preview_len(),
                ).slice();
                break;
            }
            case "suggest":
                result = JSON.parse(session.suggest(m.settings));
                break;
            case "response":
                result = response(m.settings, m.rate);
                break;
            case "export":
                result = {
                    bytes: session.export_wav(m.settings, m.format),
                    report: JSON.parse(session.report()),
                };
                break;
            default:
                throw new Error("Unknown browser DSP job");
        }
        const transfer = [];
        if (result?.samples) transfer.push(result.samples.buffer);
        if (result?.bytes) transfer.push(result.bytes.buffer);
        self.postMessage({ id: m.id, result }, transfer);
    } catch (error) {
        self.postMessage({ id: m.id, error: String(error) });
    }
};
