// A small bridge shared by the independent server UI and the local app. It has
// no audio decoder, player, preferences or recovery-store dependency.
const listeners = new Set();
let controller, reveal, mainFile, localFile, localLocked = false;
const localSettingIds = ["analyze", "profile", "blend-profile", "blend", "custom-target", "target",
    "ceiling", "intensity", "safety", "gentler", "louder", "format", "dither", "suggest", "reference"];
const hasUploadBytes = (file) => file && Number.isSafeInteger(file.size) && file.size > 0 && typeof file.slice === "function";

export const serverProcessing = {
    get enabled() { return !!controller?.enabled; },
    attach(value, showReview) {
        controller = value; reveal = showReview;
        if (mainFile) controller.selectMainFile(mainFile);
    },
    subscribe(listener) { listeners.add(listener); },
    changed() {
        for (const listener of listeners) listener();
        this.renderMainControls();
    },
    rememberLocalFile(file) {
        const changed = localFile !== file;
        localFile = file;
        if (!this.enabled && changed) {
            // Recovery restores decoded PCM and a display name, not the original
            // encoded File. Keep that name for local playback; require a real
            // picker selection before adopting a source for server upload.
            mainFile = hasUploadBytes(file) ? file : null;
            controller?.selectMainFile(mainFile);
        }
    },
    select(file) { mainFile = file; controller?.selectMainFile(file); },
    review() { if (this.enabled) reveal?.(); },
    renderMainControls(options) {
        if (options) localLocked = !!options.locked;
        const locked = localLocked;
        if (typeof document === "undefined") return;
        for (const id of ["hybrid-local-note", "hybrid-playback-note"]) {
            const note = document.getElementById(id);
            if (!note) continue;
            note.hidden = !this.enabled;
            note.textContent = localFile
                ? `Local player and local exports still use ${localFile.name}. Server results are separate downloads below.`
                : "Server results are separate downloads below. The local player has no server audio loaded.";
        }
        if (!this.enabled) {
            if (!listeners.size) {
                for (const id of ["master", "mobile-master", "add"]) {
                    const control = document.getElementById(id);
                    if (control) {
                        control.disabled = true;
                        if (id !== "add") control.textContent = "◇ Master track";
                    }
                }
            }
            return;
        }
        for (const id of localSettingIds) {
            const control = document.getElementById(id);
            if (control) control.disabled = true;
        }
        for (const id of ["master", "mobile-master"]) {
            const control = document.getElementById(id);
            if (control) { control.disabled = locked; control.textContent = "Review server master"; }
        }
        const add = document.getElementById("add");
        if (add) add.disabled = locked;
        for (const id of ["export", "mobile-export"]) {
            const control = document.getElementById(id);
            if (control) control.textContent = control.textContent.replace(/Export (?:local )?/, "Export local ");
        }
    },
};
