import { wireDialogFocus } from "./dialog-focus.js";
import { download } from "./view.js";
export function wireRecoveryDialog(recovery) {
    const $ = (id) => document.getElementById(id), dialog = $("recovery-dialog"), modal = wireDialogFocus(dialog);
    let working = false;
    function render() {
        const state = recovery.state;
        $("recovery-status").textContent = state.message;
        $("recovery-enable").disabled = working || state.enabled;
        $("recovery-disable").disabled = working || !state.enabled;
        $("recovery-enabled").textContent = state.enabled ? "Enabled on this device" : "Off by default";
        for (const id of ["recovery-delete", "recovery-save", "recovery-download", "recovery-import", "recovery-refresh"])
            $(id).disabled = working || state.restoring;
        $("recovery-save").disabled ||= !state.enabled || state.paused;
    }
    async function run(action) {
        if (working) return;
        working = true; render();
        try { await action(); }
        catch (error) { $("recovery-action-status").textContent = error.message || "Recovery action failed. Current work was kept."; }
        finally { working = false; render(); }
    }
    async function list() {
        const rows = await recovery.list(), host = $("recovery-points");
        host.replaceChildren();
        if (!rows.length) host.textContent = "No recovery points saved on this site.";
        for (const row of rows) {
            const item = document.createElement("li"), label = document.createElement("span"), button = document.createElement("button");
            label.textContent = `${row.label || "Audio session"} · ${row.kind || "Recovery"} · ${new Date(row.saved).toLocaleString()} · ${(row.bytes / 1048576).toFixed(1)} MiB${row.owner === recovery.owner ? " · This tab" : ""}`;
            button.textContent = "Restore point";
            button.onclick = () => run(async () => {
                await recovery.restorePoint(row.id);
                $("recovery-action-status").textContent = "Restored with playback stopped. Close this panel to review your work.";
            });
            item.append(label, button); host.append(item);
        }
    }
    $("recovery-enable").onclick = () => run(async () => { await recovery.configure(true); await list(); });
    $("recovery-disable").onclick = () => run(() => recovery.configure(false));
    $("recovery-delete").onclick = () => { $("recovery-delete-confirm").hidden = false; $("recovery-keep").focus(); };
    $("recovery-keep").onclick = () => { $("recovery-delete-confirm").hidden = true; };
    $("recovery-delete-now").onclick = () => run(async () => {
        await recovery.configure(false, true); await list(); $("recovery-delete-confirm").hidden = true;
    });
    $("recovery-save").onclick = () => run(async () => { await recovery.save(); await list(); });
    $("recovery-refresh").onclick = () => run(list);
    $("recovery-download").onclick = () => run(async () => {
        download(await recovery.download(), "SlimeyBeats-session.sbrecovery", "application/octet-stream");
        $("recovery-action-status").textContent = "Recovery download requested. Keep this file private: it contains your audio and project settings.";
    });
    $("recovery-import").onclick = () => $("recovery-file").click();
    $("recovery-file").onchange = () => run(async () => {
        const file = $("recovery-file").files[0]; $("recovery-file").value = "";
        if (file) await recovery.importFile(file);
    });
    $("recovery-close").onclick = () => dialog.close();
    return { render, open() {
        $("recovery-action-status").textContent = "";
        modal.open(); $("recovery-title").focus({ preventScroll: true }); render(); run(async () => { await recovery.refresh(); await list(); });
    } };
}
