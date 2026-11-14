import { StudioClient } from "./studio-client.js";
import { StudioPlayback } from "./studio-player.js";
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
import { stemZip } from "./studio-zip.js";
import { wireStemImport } from "./studio-import-actions.js";
import { interleave } from "./dsp-client.js";
import { download, time } from "./view.js";

const $ = (id) => document.getElementById(`s-${id}`);
const EMPTY = { loaded: false, stems: [], controls: { tracks: [] }, notes: [] };
const PENDING_MIX =
    "Mix controls have pending changes. Go to Stem mix → Update mix preview before further edits or saving.";
export function createStudio({
    getMainAudio,
    promote,
    leave,
    pauseMain,
    onStatus,
    onState,
}) {
    document.getElementById("studio-tools").innerHTML = studioTemplate;
    const client = new StudioClient(),
        playback = new StudioPlayback((error) => {
            if (error) notice(error.message, true);
            sync();
        });
    let info = EMPTY,
        defaults,
        controls,
        busy = false,
        cancelled = false,
        committing = false,
        active = false,
        image,
        chart,
        dirty = false,
        pendingMix = false,
        filename = "Studio";
    function notice(text, error = false) {
        $("status").textContent = text;
        $("status").classList.toggle("error", error);
        if (active) onStatus(text, error);
    }
    function draw() {
        if (!active) return;
        waveform($("wave"), playback);
        chart = spectrum($("spectrum-canvas"), image, selection());
    }
    function sync() {
        for (const input of document.querySelectorAll(
            "#studio-tools input, #studio-tools select",
        ))
            input.disabled = busy;
        for (const id of ["source", "use-main", "import"])
            $(id).disabled = busy;
        for (const button of $("stem-files").querySelectorAll("button"))
            button.disabled = busy;
        importer.sync(busy);
        $("use-main").disabled ||= !getMainAudio();
        $("close").disabled = busy;
        $("cancel").disabled = !busy || committing || cancelled;
        $("undo").disabled = busy || pendingMix || !info.can_undo;
        $("redo").disabled = busy || pendingMix || !info.can_redo;
        $("spectrum").disabled = busy || !info.loaded;
        for (const id of ["voice", "repair", "export", "promote"])
            $(id).disabled = busy || pendingMix || !info.loaded;
        for (const id of ["mix", "export-stems"])
            $(id).disabled = busy || !info.stems.length;
        $("apply-stem").disabled =
            busy || pendingMix || !(info.direct_edit_stems || []).includes($("target").value);
        for (const id of ["play", "stop", "before", "after", "seek"])
            $(id).disabled = busy || !info.loaded;
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
    }
    function requireUpdatedMix() {
        if (!pendingMix) return true;
        notice(PENDING_MIX, true);
        sync();
        return false;
    }
    async function job(text, action, requiresMix = false) {
        if (busy) return;
        if (requiresMix && !requireUpdatedMix()) return;
        busy = true;
        cancelled = false;
        playback.pause();
        notice(text);
        sync();
        onState();
        try {
            defaults ??= (await client.call("init")).defaults;
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
        }
    }
    async function mutate(
        action,
        values,
        transfer = [],
        reset = false,
        label = filename,
    ) {
        // Every non-mix revision replaces controls with the committed settings.
        if (action !== "mix" && !requireUpdatedMix()) return false;
        const result = await client.call(action, values, transfer);
        if (cancelled) {
            await client.call("discard", { proposal: result.proposal });
            return false;
        }
        let buffers;
        try {
            buffers = playback.prepareAudio(result);
        } catch (error) {
            await client.call("discard", { proposal: result.proposal });
            throw error;
        }
        committing = true;
        sync();
        await client.call("commit", { proposal: result.proposal });
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
        return true;
    }
    function changed() {
        dirty = true;
        sync();
        draw();
    }
    function mixChanged() {
        pendingMix = true;
        changed();
    }
    async function load(file) {
        if (!file) return;
        await job("Decoding Studio audio locally…", async () => {
            const buffer = await playback.decode(file);
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
            );
        }, true);
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
        }, true);
    strengthReadouts();
    const importer = wireStemImport({
        client,
        playback,
        job,
        mutate,
        notice,
        isCancelled: () => cancelled,
    });
    $("mix").onclick = () =>
        job("Rendering the stem balance…", async () => {
            await mutate("mix", { settings: controls });
        });
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
        image = null;
        draw();
        sync();
    };
    for (const id of ["export", "export-stems", "promote"])
        $(id).onclick = () =>
            job(
                id === "promote"
                    ? "Preparing the mastering source…"
                    : "Preparing a local download…",
                async () => {
                    const result = await client.call(
                        id === "export"
                            ? "export"
                            : id === "export-stems"
                              ? "stem-export"
                              : "working",
                        { format: $("format").value },
                    );
                    if (cancelled) return;
                    if (id === "promote") {
                        await promote(
                            new File([result.bytes], "Studio mix.wav", {
                                type: "audio/wav",
                            }),
                        );
                        notice(
                            "Studio preview loaded for mastering. Your Studio workspace is retained.",
                        );
                    } else {
                        download(
                            id === "export-stems"
                                ? stemZip(result.files, result.manifest)
                                : result.bytes,
                            id === "export-stems"
                                ? "Studio-stems.zip"
                                : "Studio-preview.wav",
                            id === "export-stems"
                                ? "application/zip"
                                : "audio/wav",
                        );
                        notice(
                            "Download prepared from the current preview. Listening volume is not applied.",
                        );
                    }
                },
                id !== "export-stems",
            );
    $("cancel").onclick = () => {
        cancelled = true;
        notice(
            "Cancelling after the current processing step. Keeping your previous preview…",
        );
        sync();
    };
    $("close").onclick = () => {
        cancelled = true;
        client.stop();
        importer.clear();
        playback.clear();
        info = EMPTY;
        controls = null;
        image = null;
        dirty = false;
        pendingMix = false;
        defaults = null;
        stemInputs();
        document.querySelector(".edit-import").open = false;
        $("info").textContent = "No Studio audio loaded";
        $("notes").replaceChildren();
        mixFaders({ tracks: [] }, mixChanged);
        notice("Choose a source or aligned stems to begin.");
        leave();
    };
    for (const key of ["mix", "voice", "repair"])
        $("tab-" + key).onclick = () => {
            for (const name of ["mix", "voice", "repair"]) {
                $(name + "-page").hidden = name !== key;
                $("tab-" + name).classList.toggle("selected", name === key);
            }
            draw();
        };
    for (const id of ["voice-page", "repair-page"])
        $(id).addEventListener("input", changed);
    $("play").onclick = () => {
        pauseMain();
        if (playback.playing) playback.pause();
        else playback.play().catch((error) => notice(error.message, true));
    };
    $("stop").onclick = () => playback.stop();
    $("before").onclick = () => playback.choose("before");
    $("after").onclick = () => playback.choose("after");
    $("volume").oninput = () => playback.setVolume(Number($("volume").value));
    $("seek").oninput = () => playback.seek(Number($("seek").value));
    wireSelection(
        $("spectrum-canvas"),
        () => (busy ? null : image),
        () => chart,
        changed,
    );
    new ResizeObserver(draw).observe($("wave"));
    new ResizeObserver(draw).observe($("spectrum-canvas"));
    let tick = 0,
        frame = 0;
    function animate() {
        frame = 0;
        if (active && playback.playing) {
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
    sync();
    return {
        activate(value) {
            active = value;
            document.body.classList.toggle("studio-mode", value);
            if (value) pauseMain();
            else playback.pause();
            if (value)
                onStatus(
                    $("status").textContent,
                    $("status").classList.contains("error"),
                );
            sync();
            requestAnimationFrame(draw);
        },
        load,
        analyze() {
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
            return busy;
        },
        get loaded() {
            return info.loaded;
        },
        get label() {
            return info.loaded ? filename : "Studio editor";
        },
    };
}
