import test from "node:test";
import assert from "node:assert/strict";
import { renderEngineGuidance } from "./engine-guidance.js";

test("engine selection changes import limits without mislabeling an existing streamed source", () => {
    const ids = ["processing-privacy-note", "import-limits", "drop-limits", "export-help"];
    const nodes = new Map(ids.map(id => [id, { textContent: "" }]));
    const previous = globalThis.document;
    globalThis.document = { getElementById: id => nodes.get(id) };
    try {
        const localLimits = { max_seconds: 900, max_file_bytes: 384 * 1048576 };
        renderEngineGuidance({ server: false, remoteSource: false, localLimits });
        assert.match(nodes.get("import-limits").textContent, /15:00/);
        assert.match(nodes.get("export-help").textContent, /Browser decoding/);
        renderEngineGuidance({ server: true, remoteSource: true, localLimits });
        assert.match(nodes.get("import-limits").textContent, /1 hour/);
        assert.doesNotMatch(nodes.get("drop-limits").textContent, /15 min/);
        assert.match(nodes.get("export-help").textContent, /full-resolution server audio/);
        renderEngineGuidance({ server: false, remoteSource: true, localLimits });
        assert.match(nodes.get("import-limits").textContent, /15:00/);
        assert.match(nodes.get("export-help").textContent, /Turn on Server processing/);
    } finally { globalThis.document = previous; }
});
