import { buildMasterBundle, buildStudioBundle } from "./session-export.js";
self.onmessage = ({ data }) => {
    try {
        const blob = data.workspace === "mastering" ? buildMasterBundle(data.payload)
            : data.workspace === "studio" ? buildStudioBundle(data.payload)
            : (() => { throw new Error("Unknown session download workspace"); })();
        self.postMessage({ blob });
    } catch (error) { self.postMessage({ error: error.message || String(error) }); }
};
