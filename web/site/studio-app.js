import { StudioClient } from "./studio-client.js";
import { createStudioPlaybackRouter } from "./hybrid/studio-playback-router.js";
import { RemoteProject } from "./hybrid/remote-project.js";
import { RemoteStudioClient } from "./hybrid/remote-studio-client.js";
import { serverProcessing } from "./hybrid/main-controls.js";
import { migrateLocalStudio } from "./hybrid/studio-migration.js";
import { studioTemplate } from "./studio-template.js";
import {
    selection,
    voiceSettings,
    stemInputs,
    mixFaders,
    strengthReadouts,
} from "./studio-controls.js";
import { waveform, spectrum } from "./studio-plots.js";
import { wireSelection } from "./studio-selection.js";
import { wireStudioExports } from "./studio-export-actions.js";
import { wireStemImport } from "./studio-import-actions.js";
import { interleave } from "./dsp-client.js";
import { uploadWav } from "./hybrid/pcm-transfer.js";
import { time } from "./view.js";
import { wireStudioFeedback } from "./studio-feedback.js";
import { wireStudioPreviewStatus, MIX_PENDING_NOTICE } from "./studio-preview-status.js";
import { createStudioTimeline } from "./studio-timeline.js";
import { createArrangementAdapter } from "./arrangement-adapter.js";
import { ARRANGEMENT_UI_ENABLED, createArrangementStudioWorkspace } from "./arrangement-studio-workspace.js";
import { wireStudioTransportControls } from "./studio-transport-controls.js";
import { wireStudioDoctor } from "./studio-doctor.js";
import { changedRecovery } from "./recovery-bridge.js";
import { captureStudioRecovery, prepareStudioRecovery, studioRecoverySize, restoreStudioFields } from "./studio-recovery.js";

const $ = (id) => document.getElementById(`s-${id}`);
const EMPTY = { loaded: false, stems: [], controls: { tracks: [] }, notes: [] };
const PENDING_MIX =
    "Stem changes are live on B. Update mix preview to commit this balance before further edits or saving. The waveform shows the saved preview.";
export function createStudio({
    getMainAudio,
    promote,
    leave,
    pauseMain,
    onStatus,
    onState,
}) {
    document.getElementById("studio-tools").innerHTML = studioTemplate;
    const remoteProject = new RemoteProject({ progress: (_, text) => notice(text) });
    const remoteClient = new RemoteStudioClient(remoteProject);
    let activeClient = new StudioClient();
    const client = { call: (...args) => activeClient.call(...args), stop: (...args) => activeClient.stop(...args),
            get remote() { return activeClient === remoteClient; }, get project() { return remoteProject; } },
        playback = createStudioPlaybackRouter((error) => {
            if (error) notice(error.message, true);
            sync();
        }, pauseMain);
    let info = EMPTY,
        defaults,
        controls,
        busy = false,
        cancelled = false,
        committing = false,
        cancelPending = null,
        migrating = false,
        migrationAbort,
        pendingClient,
        active = false,
        image,
        chart,
        dirty = false,
        pendingMix = false,
        filename = "Studio",
        stemView = "mixer";
    const feedback = wireStudioFeedback(() => active);
    const previewStatus = wireStudioPreviewStatus();
    const timeline = createStudioTimeline($("editor"), {
        getState: () => ({ info, busy: busy || !!cancelPending, pendingMix, active }),
        getStems: () => playback.stems,
        position: () => playback.position(),
        seek: (seconds) => playback.seek(seconds),
        apply: ({ kind, edit }) => job(
            "Editing the selected stem region…",
            () => mutate("region", { kind, edit }),
            true,
        ),
        notice,
    });
    const doctor = wireStudioDoctor({ client,
        getState: () => ({ info, busy: busy || !!cancelPending, pendingMix, filename }),
        job, mutate, seek: seconds => playback.seek(seconds), draw, sync,
        prepare: async () => {
            if (!serverProcessing.enabled || client.remote || !info.loaded) return;
            await job("Preparing this workspace on the server…", migrateCurrent, false, false, false);
            if (!client.remote) throw new Error("The server workspace could not be prepared. Your local work is kept.");
        } });
    const arrangement = createArrangementStudioWorkspace({
        createAdapter: createArrangementAdapter, legacyPlayback: playback, pauseMain,
        legacyBusy: () => busy || !!cancelPending,
        notice, changed: () => {
            sync(); onState();
            if (audition()?.playing && active && !frame) frame = requestAnimationFrame(animate);
        },
    });
    const audition = () => arrangement.transport() || playback;
    function notice(text, error = false) {
        if (pendingMix && !error && text !== MIX_PENDING_NOTICE)
            text = `${text} ${MIX_PENDING_NOTICE}`;
        $("status").textContent = text;
        $("status").classList.toggle("error", error);
        if (error) feedback.error(text);
        if (active) onStatus(text, error);
    }
    function draw() {
        if (!active || arrangement.selected) return;
        waveform($("wave"), playback);
        chart = spectrum($("spectrum-canvas"), image, selection());
        timeline.draw();
    }
    function sync() {
        const locked = busy || !!cancelPending || arrangement.busy;
        document.getElementById("studio-tools").dataset.loaded = String(arrangement.selected ? arrangement.loaded : info.loaded);
        const hasStems = info.stems.length > 0;
        $("stem-views").hidden = !hasStems;
        $("editor-view").hidden = !hasStems || stemView !== "timeline";
        $("mixer-view").hidden = hasStems && stemView === "timeline";
        for (const view of ["mixer", "timeline"]) {
            const button = $("view-" + view);
            button.disabled = locked || !hasStems;
            button.classList.toggle("selected", stemView === view);
            button.setAttribute("aria-pressed", String(stemView === view));
        }
        for (const input of document.querySelectorAll(
            "#studio-tools input, #studio-tools select",
        ))
            input.disabled = locked;
        for (const id of ["source", "use-main", "import"])
            $(id).disabled = locked;
        for (const button of $("stem-files").querySelectorAll("button"))
            button.disabled = locked;
        importer.sync(locked);
        $("separation").hidden = !serverProcessing.enabled && !info.remote;
        $("separate").disabled = locked || !info.loaded || !serverProcessing.enabled;
        $("engine-note").textContent = serverProcessing.enabled || info.remote
            ? "Server Studio keeps full-resolution audio on the server. Your browser streams previews and live faders; heavy tools and exports run remotely. Download a session ZIP to keep your work."
            : "Local Studio keeps audio on this device, with a 128 MiB decoded bank limit. Turn on Server processing for larger files and neural stem separation.";
        $("use-main").disabled ||= !getMainAudio();
        $("close").disabled = locked;
        $("cancel").disabled = (!locked && !doctor.scanning) || committing || (locked && cancelled);
        $("undo").disabled = locked || pendingMix || !info.can_undo;
        $("redo").disabled = locked || pendingMix || !info.can_redo;
        $("spectrum").disabled = locked || !info.loaded;
        for (const id of ["voice", "repair", "export", "export-session", "promote"])
            $(id).disabled = locked || pendingMix || !info.loaded;
        for (const id of ["mix", "export-stems"])
            $(id).disabled = locked || !info.stems.length;
        $("apply-stem").disabled =
            busy || pendingMix || !(info.direct_edit_stems || []).includes($("target").value);
        for (const id of ["play", "stop", "before", "after", "seek"])
            $(id).disabled = locked || !info.loaded;
        $("play").textContent = playback.playing ? "Pause" : "Play";
        $("before").classList.toggle("selected", playback.side === "before");
        $("after").classList.toggle("selected", playback.side === "after");
        $("clock").textContent =
            `${time(playback.position())} / ${time(info.duration_seconds || 0)}`;
        $("seek").max = info.duration_seconds || 1;
        $("seek").value = playback.position();
        $("pending").hidden = !dirty;
        $("pending").textContent = pendingMix
            ? PENDING_MIX
            : "Settings changed. Run the tool to hear them. Export saves the existing preview.";
        previewStatus(pendingMix, playback.monitorMix);
        timeline.sync();
        doctor.sync();
        arrangement.sync();
    }
    function requireUpdatedMix() {
        if (!pendingMix) return true;
        notice(PENDING_MIX, true);
        sync();
        return false;
    }
    async function migrateCurrent() {
        if (!serverProcessing.enabled || client.remote || !info.loaded) return;
        const localClient = activeClient, previousControls = structuredClone(controls), wasPending = pendingMix, wasDirty = dirty;
        migrating = true; migrationAbort = new AbortController();
        try {
            const result = await migrateLocalStudio(localClient, remoteClient, { signal: migrationAbort.signal });
            if (cancelled) throw new DOMException("Migration cancelled", "AbortError");
            const buffers = playback.remote.prepareAudio(result);
            activeClient = remoteClient; playback.useRemote(true); publish(result, buffers, false, filename);
            if (wasPending) { controls = previousControls; pendingMix = true; dirty = wasDirty; playback.setMix(controls); mixFaders(controls, mixChanged, info.channels, info.stem_names); }
            localClient.stop(); playback.local.clear();
        } finally { migrating = false; migrationAbort = null; }
    }
    async function job(text, action, requiresMix = false, keepPlaying = false, migrate = true) {
        if (busy || cancelPending) return;
        feedback.begin();
        if (requiresMix && !requireUpdatedMix()) return;
        busy = true;
        changedRecovery();
        cancelled = false;
        if (!keepPlaying) {
            pauseMain();
            playback.pause();
        }
        notice(text);
        sync();
        onState();
        try {
            defaults ??= (await client.call("init")).defaults;
            if (!cancelled && migrate) await migrateCurrent();
            if (!cancelled) await action();
            if (cancelled)
                notice("Cancelled. Your previous Studio preview is unchanged.");
        } catch (error) {
            if (error.name !== "AbortError")
                notice(error.message || String(error), true);
        } finally {
            busy = false;
            committing = false;
            sync();
            draw();
            onState();
            changedRecovery();
        }
    }
    async function mutate(
        action,
        values,
        transfer = [],
        reset = false,
        label = filename,
        using = activeClient,
    ) {
        // Non-mix revisions must not discard the uncommitted live controls.
        if (action !== "mix" && !requireUpdatedMix()) return false;
        pendingClient = using;
        let result;
        try { result = await using.call(action, values, transfer); }
        finally { pendingClient = null; }
        if (cancelled) {
            await using.call("discard", { proposal: result.proposal });
            return false;
        }
        let buffers;
        try {
            buffers = (result.remote ? playback.remote : playback.local).prepareAudio(result);
        } catch (error) {
            await using.call("discard", { proposal: result.proposal });
            throw error;
        }
        committing = true;
        sync();
        await using.call("commit", { proposal: result.proposal });
        const previousClient = activeClient;
        activeClient = using;
        playback.useRemote(!!result.remote);
        publish(result, buffers, reset, label);
        if (previousClient !== using) {
            previousClient.stop();
            if (result.remote) playback.local.clear();
        }
        changedRecovery();
        return true;
    }
    function publish(result, buffers, reset, label) {
        doctor.invalidate();
        if (reset) stemView = "mixer";
        playback.setAudio(buffers, reset);
        info = result.info;
        filename = label;
        controls = structuredClone(info.controls);
        dirty = false;
        pendingMix = false;
        image = null;
        const target = $("target").value;
        $("target").replaceChildren(
            ...["source", "preview", ...info.stems].map((value) => {
                const option = document.createElement("option");
                option.value = value;
                option.textContent =
                    value === "source"
                        ? "Original source"
                        : value === "preview"
                          ? "Current preview"
                          : `${info.stem_names?.[value] ?? value} stem`;
                return option;
            }),
        );
        $("target").value = ["source", "preview", ...info.stems].includes(
            target,
        )
            ? target
            : "source";
        $("info").textContent =
            `${filename} · ${time(info.duration_seconds)} · ${info.sample_rate} Hz · ${info.channels} channel${info.channels === 1 ? "" : "s"}`;
        $("notes").replaceChildren(
            ...info.notes.map((text) => {
                const li = document.createElement("li");
                li.textContent = text;
                return li;
            }),
        );
        mixFaders(controls, mixChanged, info.channels, info.stem_names);
        timeline.sync({ reset });
        if (reset) {
            document.querySelector(".edit-import").open = false;
            $("end").value = Math.min(1, info.duration_seconds);
            $("high").value = Math.min(8000, info.sample_rate / 2);
            $("low").value = Math.min(2000, info.sample_rate / 4);
        }
        committing = false;
        notice(
            "Preview ready. Compare Before and Preview, then export when ready.",
        );
    }
    function changed() {
        dirty = true;
        sync();
        draw();
        changedRecovery();
    }
    function mixChanged() {
        doctor.invalidate();
        pendingMix = true;
        pauseMain();
        playback.setMix(controls);
        notice(MIX_PENDING_NOTICE);
        changed();
    }
    async function load(file) {
        if (!file) return;
        if (arrangement.selected) return arrangement.importFiles([file]);
        await job(serverProcessing.enabled ? "Uploading Studio audio to the server…" : "Decoding Studio audio locally…", async () => {
            if (serverProcessing.enabled) {
                await mutate("load-file", { file }, [], true, file.name, remoteClient); return;
            }
            const using = client.remote ? new StudioClient() : activeClient;
            const buffer = await playback.local.decode(file);
            if (cancelled) return;
            const samples = interleave(buffer);
            await mutate(
                "load",
                {
                    samples,
                    rate: buffer.sampleRate,
                    channels: buffer.numberOfChannels,
                },
                [samples.buffer],
                true,
                file.name,
                using,
            );
        }, true, false, false);
    }
    $("source").onclick = () => $("file").click();
    $("file").onchange = async () => {
        await load($("file").files[0]);
        $("file").value = "";
    };
    $("use-main").onclick = () =>
        job("Loading the current mix into Studio…", async () => {
            const buffer = getMainAudio();
            if (!buffer)
                throw new Error("Load a mix first, or choose Studio audio.");
            if (serverProcessing.enabled) {
                const action = buffer.remote ? "clone-audio" : "load-file";
                const values = buffer.remote ? { audio: buffer } : { file: uploadWav(buffer) };
                await mutate(action, values, [], true, "Mastering mix", remoteClient); return;
            }
            if (buffer.remote) throw new Error("Turn on Server processing to use this server mix in Studio.");
            const samples = interleave(buffer);
            await mutate(
                "load",
                {
                    samples,
                    rate: buffer.sampleRate,
                    channels: buffer.numberOfChannels,
                },
                [samples.buffer],
                true,
                "Loaded mix",
            );
        }, true, false, false);
    strengthReadouts();
    for (const event of ["input", "change"])
        document.getElementById("studio-tools").addEventListener(event, (event) => {
            // Timeline navigation is transient; applied edits mark recovery in job().
            if (!event.target.closest("#s-editor, #s-doctor-page, #s-arrangement-page, #s-arrangement-server-page")) changedRecovery();
        });
    const importer = wireStemImport({
        client,
        playback,
        job: (text, action, requiresMix = false, keepPlaying = false) => job(text, action, requiresMix, keepPlaying, false),
        mutate: (action, values, transfer = [], reset = false, label = filename) => mutate(action, values, transfer,
            reset, label, !serverProcessing.enabled && client.remote ? new StudioClient() : activeClient),
        notice,
        isCancelled: () => cancelled,
        remote: {
            enabled: () => serverProcessing.enabled,
            async load(entries, preference) { return mutate("import-files", { entries, preference }, [], true, "Imported stems", remoteClient); },
        },
    });
    $("mix").onclick = () =>
        job("Rendering the stem balance…", async () => {
            await mutate("mix", { settings: structuredClone(controls) });
        });
    $("separate").onclick = () => job("Separating stems on the server…", async () => {
        if (!client.remote) throw new Error("Choose Studio audio with Server processing on before separating stems.");
        await mutate("separate", { mode: $("separation-mode").value });
    }, true);
    for (const kind of ["voice", "repair"])
        $(kind).onclick = () =>
            job(
                `Preparing ${kind === "voice" ? "voice cleanup" : "spectral repair"}…`,
                async () => {
                    const target = $("target").value;
                    const values =
                        kind === "voice"
                            ? { settings: voiceSettings(defaults.voice) }
                            : { selection: selection() };
                    await mutate(kind, { target, ...values });
                },
                true,
            );
    $("apply-stem").onclick = () =>
        job("Applying this edit to the selected stem…", async () => {
            await mutate("replace", { kind: $("target").value });
        }, true);
    for (const [id, redo] of [
        ["undo", false],
        ["redo", true],
    ])
        $(id).onclick = () =>
            job(
                redo
                    ? "Restoring the next revision…"
                    : "Restoring the previous revision…",
                async () => {
                    await mutate("history", { redo });
                },
                true,
            );
    $("spectrum").onclick = () =>
        job(
            "Building a time-frequency view of the selected source…",
            async () => {
                const result = await client.call("spectrogram", {
                    target: $("target").value,
                });
                if (!cancelled) {
                    image = result;
                    notice(
                        "Select only the unwanted region. The graph shows the selected source before this repair.",
                    );
                }
            },
        );
    $("target").onchange = () => {
        doctor.invalidate("Target changed. Scan the selected saved source again.");
        image = null;
        draw();
        sync();
    };
    const exportActions = wireStudioExports({ getState: () => ({ busy: busy || !!cancelPending, cancelled, info }),
        client, playback, job, promote, notice });
    $("cancel").onclick = () => {
        if (!busy && doctor.scanning) {
            doctor.cancel();
            if (client.remote) cancelPending = remoteClient.cancel().catch(error => notice(error.message, true)).finally(() => { cancelPending = null; sync(); onState(); });
            return;
        }
        exportActions.cancel();
        cancelled = true;
        if (client.remote || pendingClient === remoteClient || migrating) {
            migrationAbort?.abort(new DOMException("Migration cancelled", "AbortError"));
            cancelPending = remoteClient.cancel().catch(error => notice(error.message, true)).finally(() => { cancelPending = null; sync(); onState(); });
        }
        notice(
            "Cancelling after the current processing step. Keeping your previous preview…",
        );
        sync();
    };
    $("close").onclick = async () => {
        if (arrangement.selected) { arrangement.pause(); leave(); return; }
        if (client.remote) {
            await job("Deleting the temporary Studio project…", () => remoteClient.close(), false, false, false);
            if (remoteProject.id) return;
        }
        doctor.invalidate("Choose a source, then scan.");
        exportActions.cancel();
        cancelled = true;
        client.stop();
        importer.clear();
        playback.clear();
        playback.useRemote(false);
        if (client.remote) activeClient = new StudioClient();
        info = EMPTY;
        controls = null;
        image = null;
        dirty = false;
        pendingMix = false;
        timeline.reset();
        stemView = "mixer";
        defaults = null;
        stemInputs();
        document.querySelector(".edit-import").open = false;
        $("info").textContent = "No Studio audio loaded";
        $("notes").replaceChildren();
        mixFaders({ tracks: [] }, mixChanged);
        notice("Choose audio.");
        leave();
        changedRecovery();
    };
    for (const key of ["mix", "voice", "repair", "doctor"])
        $("tab-" + key).onclick = () => {
            for (const name of ["mix", "voice", "repair", "doctor"]) {
                $(name + "-page").hidden = name !== key;
                $("tab-" + name).classList.toggle("selected", name === key);
            }
            draw();
        };
    for (const view of ["mixer", "timeline"])
        $("view-" + view).onclick = () => {
            if (busy || !info.stems.length) return;
            stemView = view;
            sync();
            draw();
        };
    for (const id of ["voice-page", "repair-page"])
        $(id).addEventListener("input", changed);
    wireStudioTransportControls({ transport: audition, beforePlay: pauseMain, notice });
    wireSelection(
        $("spectrum-canvas"),
        () => (busy ? null : image),
        () => chart,
        changed,
    );
    new ResizeObserver(draw).observe($("wave"));
    new ResizeObserver(draw).observe($("spectrum-canvas"));
    new ResizeObserver(draw).observe($("editor"));
    let tick = 0,
        frame = 0;
    function animate() {
        frame = 0;
        if (active && audition()?.playing) {
            if (++tick % 4 === 0) {
                draw();
                sync();
            }
            frame = requestAnimationFrame(animate);
        }
    }
    const originalChange = playback.onChange;
    playback.onChange = (error) => {
        originalChange(error);
        if (playback.playing && active && !frame)
            frame = requestAnimationFrame(animate);
    };
    mixFaders({ tracks: [] }, mixChanged);
    serverProcessing.subscribe(sync);
    sync();
    return {
        activate(value) {
            active = value;
            document.body.classList.toggle("studio-mode", value);
            if (value) pauseMain();
            else playback.pause();
            arrangement.activate(value);
            if (value && ARRANGEMENT_UI_ENABLED) void arrangement.initialise();
            if (value)
                onStatus(
                    $("status").textContent,
                    $("status").classList.contains("error"),
                );
            sync();
            requestAnimationFrame(draw);
        },
        load,
        pause() { playback.pause(); arrangement.pause(); },
        analyze() {
            if (arrangement.selected) {
                notice("Choose the explicit mix, track, clip or range in Shape this part to measure an arrangement EQ proposal.");
                return;
            }
            return job(
                "Measuring the selected Studio input locally…",
                async () => {
                    const target = $("target");
                    const label = target.selectedOptions[0].textContent;
                    const result = await client.call("analyze", {
                        target: target.value,
                    });
                    const value = (n) =>
                        Number.isFinite(n) ? n.toFixed(1) : "not measurable";
                    if (!cancelled)
                        notice(
                            `${label}: ${value(result.integrated_lufs)} LUFS integrated · ${value(result.true_peak_dbtp)} dBTP reconstructed · ${value(result.crest_db)} dB crest. Measurements do not replace listening.`,
                        );
                },
            );
        },
        get busy() {
            return busy || !!cancelPending || arrangement.busy;
        },
        get loaded() {
            return info.loaded || arrangement.loaded;
        },
        get label() {
            return arrangement.selected ? "Studio arrangement" : info.loaded ? filename : "Studio editor";
        },
        recovery: {
            get busy() { return busy || arrangement.busy; },
            get loaded() { return info.loaded || arrangement.loaded; },
            get stemCount() { return info.stems.length; },
            estimate: () => info.loaded ? studioRecoverySize(playback) : 0,
            capture: () => {
                if (arrangement.loaded) throw new Error("This recovery format does not include arrangement tracks. Keep this page open until you can save an arrangement project.");
                return captureStudioRecovery(client, playback, { info, controls, pendingMix, dirty, filename });
            },
            async prepare(value, current) {
                const staged = await prepareStudioRecovery(StudioClient, value, playback, current);
                let adopted = false;
                let buffers;
                try { buffers = playback.local.prepareAudio(staged.result); }
                catch (error) { staged.client.stop(); throw error; }
                return {
                    commit() {
                        playback.stop(); pauseMain();
                        playback.useRemote(false);
                        client.stop(); activeClient = staged.client; adopted = true;
                        publish(staged.result, buffers, true, typeof value.filename === "string" ? value.filename.slice(0, 200) : "Recovered Studio");
                        restoreStudioFields(value.fields);
                        // Native mix validation runs in the staged worker before these controls reach live playback.
                        controls = structuredClone(value.pendingMix ? value.controls : info.controls);
                        pendingMix = value.pendingMix === true;
                        dirty = value.dirty === true;
                        if (pendingMix) playback.setMix(controls);
                        mixFaders(controls, mixChanged, info.channels, info.stem_names);
                        playback.stop(); playback.side = "after";
                        sync(); draw(); onState();
                    },
                    dispose() { if (!adopted) staged.client.stop(); },
                };
            },
        },
    };
}
