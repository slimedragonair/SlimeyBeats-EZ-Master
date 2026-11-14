import { DspClient, interleave, toBuffer } from "./dsp-client.js";
import { Player } from "./audio/player.js";
import { limitsSummary } from "./audio/import-validation.js";
import { wireHelp } from "./help.js";
import {
    $,
    time,
    status,
    page,
    metrics,
    reportView,
    buildBands,
    download,
} from "./view.js";
import { Plots } from "./plots.js";
import { createStudio } from "./studio-app.js";

let settings,
    defaults,
    busy = false,
    restoring = false,
    ready = false,
    currentFile,
    analysis,
    report,
    referenceFile,
    masterKey = "",
    revision = 0,
    trackRevision = 0,
    referenceRevision = 0,
    jobRevision = 0,
    curveTimer;
const player = new Player(syncControls, (error) => {
    status(error.message, true);
});
const plots = new Plots(player);
const dsp = new DspClient((value, text) => {
    $("progress").value = value;
    status(text);
});
const studio = createStudio({
    getMainAudio: () => player.original,
    pauseMain: () => player.pause(),
    promote: async (file) => {
        if (busy || restoring)
            throw new Error(
                "Wait for the mastering job before sending Studio audio.",
            );
        await loadFile(file);
        if (currentFile !== file)
            throw new Error(
                "The mastering source could not be loaded. Your Studio preview is still available.",
            );
    },
    leave: () => page("overview"),
    onStatus: (text, error) => {
        $("status").textContent = `Studio · ${text}`;
        $("status").classList.toggle("status-error", error);
    },
    onState: syncControls,
});
document.addEventListener("studio-page", (event) => {
    studio.activate(event.detail === "studio-tools");
    if (event.detail !== "studio-tools") {
        $("track-title").textContent = currentFile
            ? currentFile.name.replace(/\.[^.]+$/, "")
            : "Shape your sound.";
        $("session-state").textContent = currentFile
            ? "Track loaded · Analyze or Master to begin"
            : "Your next master starts here";
        status(
            player.original
                ? "Mastering · analyze, prepare a preview and export when ready."
                : "Ready for a mix. Add audio or try the example.",
        );
    }
    syncControls();
});
wireHelp();
syncControls();
function masterSignature() {
    if (!settings) return "";
    const {
        manual_eq,
        eq_output_db,
        eq_hold_loudness,
        dither,
        formats,
        ...auto
    } = settings;
    return JSON.stringify({ ...auto, reference_revision: referenceRevision });
}
function syncControls() {
    const locked = busy || restoring;
    const loaded = Boolean(player.original),
        mastered = Boolean(player.master),
        stale = mastered && masterKey !== masterSignature();
    for (const id of ["add", "demo", "reference", "clear"])
        $(id).disabled = locked || !ready;
    $("analyze").disabled = document.body.classList.contains("studio-mode")
        ? studio.busy || !studio.loaded
        : locked || !loaded;
    $("master").disabled = locked || !loaded;
    $("export").disabled = locked || !mastered || stale;
    $("mobile-master").disabled = $("master").disabled;
    $("mobile-export").disabled = $("export").disabled;
    $("mobile-master").textContent = mastered
        ? "◇ Re-master"
        : "◇ Master track";
    for (const id of [
        "profile",
        "blend-profile",
        "blend",
        "custom-target",
        "target",
        "ceiling",
        "intensity",
        "safety",
        "gentler",
        "louder",
        "format",
        "dither",
    ])
        $(id).disabled = busy || !ready;
    $("suggest").disabled = locked || !loaded;
    $("play").disabled = locked || !loaded;
    $("stop").disabled = !loaded;
    $("master-source").disabled = !mastered;
    $("seek").disabled = !loaded;
    $("play").textContent = player.playing ? "Ⅱ Pause" : "▶ Play";
    $("original").classList.toggle("selected", player.source === "original");
    $("master-source").classList.toggle("selected", player.source === "master");
    $("job").hidden = !busy;
    $("master").textContent = mastered ? "◇ Re-master track" : "◇ Master track";
    $("live-state").textContent = mastered
        ? player.playing && player.source === "master"
            ? "● EQ live"
            : "EQ ready · Play B to hear"
        : "Master a track to begin";
    if (document.body.classList.contains("studio-mode")) {
        $("track-title").textContent = studio.label;
        $("session-state").textContent =
            "Studio editing · preview, compare, then export";
    } else if (stale)
        $("session-state").textContent =
            "Direction changed · Master again to apply";
    else if (mastered)
        $("session-state").textContent =
            "Master preview ready · Listen, tweak, then export";
}
async function job(action) {
    if (busy || restoring) return;
    const current = ++jobRevision;
    busy = true;
    $("progress").value = 0;
    syncControls();
    try {
        await action(() => {
            if (current !== jobRevision)
                throw new DOMException("Processing cancelled", "AbortError");
        });
    } catch (error) {
        if (current === jobRevision && error.name !== "AbortError")
            status(error.message || String(error), true);
    } finally {
        if (current === jobRevision) {
            busy = false;
            syncControls();
        }
    }
}
function eqChanged() {
    if (!settings) return;
    revision++;
    player.updateSettings(settings, revision);
    clearTimeout(curveTimer);
    const version = revision;
    curveTimer = setTimeout(async () => {
        try {
            const curve = await dsp.call("response", {
                settings: JSON.stringify(settings),
                rate: player.ctx?.sampleRate || 48000,
            });
            if (version === revision) plots.curve = curve;
        } catch (error) {
            if (error.name !== "AbortError") status(error.message, true);
        }
    }, 45);
    $("eq-output-value").textContent = `${settings.eq_output_db.toFixed(1)} dB`;
    if (player.master)
        status(
            "Live EQ updated. Export resolves whole-track loudness and verifies the WAV peak.",
        );
}
function clearMaster() {
    player.pause();
    player.master = null;
    player.analysis = null;
    player.source = "original";
    player.delta = 0;
    masterKey = "";
    report = null;
    $("output-section").hidden = true;
    reportView(null);
    syncControls();
}
async function loadWorker(buffer = player.original) {
    const owner = jobRevision;
    const ensureOwner = () => {
        if (owner !== jobRevision)
            throw new DOMException("Processing cancelled", "AbortError");
    };
    const samples = interleave(buffer);
    await dsp.call(
        "load",
        {
            samples,
            rate: buffer.sampleRate,
            channels: buffer.numberOfChannels,
        },
        [samples.buffer],
    );
    ensureOwner();
    if (referenceFile) {
        const decoded = await player.decode(referenceFile);
        ensureOwner();
        const samples = interleave(decoded);
        await dsp.call(
            "reference",
            {
                samples,
                rate: decoded.sampleRate,
                channels: decoded.numberOfChannels,
            },
            [samples.buffer],
        );
        ensureOwner();
    }
}
async function loadFile(file) {
    if (!file) return;
    await job(async (ensureCurrent) => {
        status("Decoding your track locally…");
        const buffer = await player.decode(file);
        ensureCurrent();
        await loadWorker(buffer);
        ensureCurrent();
        trackRevision++;
        clearMaster();
        player.setOriginal(buffer);
        analysis = null;
        currentFile = file;
        plots.setAudio(buffer);
        $("welcome").hidden = true;
        $("measurements").hidden = true;
        $("track-card").hidden = false;
        $("drop").hidden = true;
        $("filename").textContent = file.name;
        $("track-title").textContent = file.name.replace(/\.[^.]+$/, "");
        $("session-state").textContent =
            "Track loaded · Analyze or Master to begin";
        $("filemeta").textContent =
            `${time(Math.round(buffer.duration))} · ${buffer.numberOfChannels === 2 ? "Stereo" : "Mono"} · decoded ${buffer.sampleRate.toLocaleString()} Hz`;
        buildBands(settings, eqChanged, buffer.sampleRate);
        page("overview");
        status(
            "Track ready. Analyze to inspect the mix, or Master to create a preview.",
        );
    });
}
async function analyze() {
    if (document.body.classList.contains("studio-mode"))
        return studio.analyze();
    await job(async () => {
        const source = trackRevision;
        const result = await dsp.call("analyze", {
            settings: JSON.stringify(settings),
        });
        if (source !== trackRevision) return;
        analysis = result;
        $("steps").hidden = true;
        metrics("input-metrics", result);
        $("measurements").hidden = false;
        $("welcome").hidden = true;
        $("session-state").textContent =
            "Input analyzed · Choose your mastering direction";
        const issues = result.issues
            .filter((i) => i.severity >= 20)
            .sort((a, b) => b.severity - a.severity);
        $("insight").textContent = issues.length
            ? issues
                  .slice(0, 3)
                  .map((i) => i.detail)
                  .join(". ")
            : "No strong detector warnings. Start with a gentle master, then compare at matched level.";
        status(
            "Analysis complete. These measurements guide choices; your ears make the final call.",
        );
    });
}
async function master() {
    await job(async () => {
        player.pause();
        status("Mastering locally in the Rust worker…");
        const source = trackRevision,
            key = masterSignature();
        const result = await dsp.call("master", {
            settings: JSON.stringify(settings),
        });
        if (source !== trackRevision) return;
        const buffer = toBuffer(
            player.context(),
            result.samples,
            player.original.numberOfChannels,
            player.original.sampleRate,
        );
        report = result.report;
        analysis = report.input;
        $("steps").hidden = true;
        player.setMaster(
            buffer,
            result.raw_analysis,
            settings,
            analysis.metrics.integrated_lufs,
            report.output.metrics.integrated_lufs,
        );
        masterKey = key;
        player.select("master");
        metrics("input-metrics", analysis);
        metrics("output-metrics", report.output);
        $("measurements").hidden = false;
        $("welcome").hidden = true;
        $("output-section").hidden = false;
        $("session-state").textContent =
            "Master preview ready · Listen, tweak, then export";
        $("insight").textContent =
            "Your original is intact. Compare A / B at matched loudness, or open Manual EQ to refine the master in real time.";
        reportView(report);
        eqChanged();
        status(
            "Master ready. Play B to listen. Nothing is exported until you choose Export WAV.",
        );
    });
}
async function exportWav() {
    await job(async () => {
        const source = trackRevision,
            version = revision,
            snapshot = JSON.stringify(settings);
        status("Resolving final EQ and verifying the encoded WAV…");
        const result = await dsp.call("export", {
            settings: snapshot,
            format: $("format").value,
        });
        if (source !== trackRevision || version !== revision)
            throw new Error(
                "EQ changed during export. Export again to save your latest settings.",
            );
        report = result.report;
        report.file = currentFile.name;
        reportView(report);
        metrics("output-metrics", report.output);
        const stem = currentFile.name
            .replace(/\.[^.]+$/, "")
            .replace(/[<>:"/\\|?*\x00-\x1f]/g, "_");
        download(
            result.bytes,
            `${stem}_master_${$("format").value}.wav`,
            "audio/wav",
        );
        status(
            `WAV downloaded · ${report.output.metrics.integrated_lufs.toFixed(1)} LUFS · ${report.output.metrics.true_peak_dbtp.toFixed(2)} dBTP. Your original is unchanged.`,
        );
    });
}
function directionChanged() {
    settings.profile = $("profile").value;
    const amount = Number($("blend").value) / 100;
    if (amount > 0)
        settings.profile += `:${1 - amount}+${$("blend-profile").value}:${amount}`;
    settings.target_lufs = $("custom-target").checked
        ? Number($("target").value)
        : null;
    settings.ceiling_dbtp = Number($("ceiling").value);
    settings.intensity = Number($("intensity").value);
    settings.safety = $("safety").checked;
    $("ceiling-value").textContent = `${settings.ceiling_dbtp.toFixed(1)} dBTP`;
    $("intensity-value").textContent = settings.intensity.toFixed(2);
    $("blend-value").textContent = `${$("blend").value}%`;
    $("target-label").hidden = !$("custom-target").checked;
    syncControls();
    if (player.master)
        status(
            masterKey === masterSignature()
                ? "Master preview ready. Listen, tweak, or export your WAV."
                : "Mastering direction changed. Re-master from the original to apply it.",
        );
}
for (const id of ["overview", "eq", "report", "studio-tools"])
    $(`tab-${id}`).onclick = () => page(id);
$("add").onclick = $("drop").onclick = () => $("file").click();
$("file").onchange = () => {
    loadFile($("file").files[0]);
    $("file").value = "";
};
$("demo").onclick = () =>
    job(async (ensureCurrent) => {
        status("Loading the included example…");
        const response = await fetch("./assets/demo-input.wav");
        if (!response.ok)
            throw new Error("The example track could not be loaded.");
        const file = new File([await response.blob()], "CC0 Example Mix.wav", {
            type: "audio/wav",
        });
        ensureCurrent();
        busy = false;
        await loadFile(file);
    });
$("analyze").onclick = analyze;
$("master").onclick = $("mobile-master").onclick = master;
$("export").onclick = $("mobile-export").onclick = exportWav;
$("play").onclick = async () => {
    try {
        if (player.playing) player.pause();
        else await player.play();
    } catch (error) {
        player.pause();
        status(error.message, true);
    }
};
$("stop").onclick = () => player.stop();
$("original").onclick = () => player.select("original");
$("master-source").onclick = () => player.select("master");
$("seek").oninput = () => {
    plots.seeking = true;
};
$("seek").onchange = async () => {
    try {
        await player.seek(
            (Number($("seek").value) / 1000) * (player.original?.duration || 0),
        );
    } catch (error) {
        status(error.message, true);
    } finally {
        plots.seeking = false;
    }
};
$("volume").oninput = () => {
    player.volume = Number($("volume").value);
    player.updateGains();
};
$("match").onchange = () => {
    player.match = $("match").checked;
    player.updateGains();
};
$("hold").onchange = () => {
    settings.eq_hold_loudness = $("hold").checked;
    eqChanged();
};
$("eq-output").oninput = () => {
    settings.eq_output_db = Number($("eq-output").value);
    eqChanged();
};
$("reset-eq").onclick = () => {
    if (!settings) return;
    settings.manual_eq = structuredClone(defaults.manual_eq);
    settings.eq_output_db = 0;
    $("eq-output").value = 0;
    buildBands(settings, eqChanged, player.ctx?.sampleRate || 48000);
    eqChanged();
};
$("suggest").onclick = () =>
    job(async () => {
        const result = await dsp.call("suggest", {
            settings: JSON.stringify(settings),
        });
        settings.manual_eq = result;
        buildBands(settings, eqChanged, player.ctx?.sampleRate || 48000);
        eqChanged();
        $("eq-note").textContent =
            "Suggested curve applied: modest, repeatable adjustments based on this audio and a light tonal-profile influence. Reset or tweak freely.";
    });
for (const id of [
    "profile",
    "blend-profile",
    "blend",
    "custom-target",
    "target",
    "ceiling",
    "intensity",
    "safety",
])
    $(id).oninput = () => {
        if (settings) directionChanged();
    };
for (const [id, mode] of [
    ["gentler", "streaming_safe"],
    ["louder", "loud"],
])
    $(id).onclick = () => {
        if (!settings) return;
        settings.mode = mode;
        $("gentler").classList.toggle("selected", mode === "streaming_safe");
        $("louder").classList.toggle("selected", mode === "loud");
        directionChanged();
    };
$("dither").onchange = () => {
    settings.dither = $("dither").checked;
};
$("reference").onclick = () => $("reference-file").click();
$("reference-file").onchange = () =>
    job(async (ensureCurrent) => {
        if (!player.original)
            throw new Error("Add your mix before choosing a reference.");
        const file = $("reference-file").files[0];
        if (!file) return;
        status("Decoding the reference locally…");
        const buffer = await player.decode(file);
        ensureCurrent();
        const samples = interleave(buffer);
        status(
            "Measuring the reference locally. Long tracks can take a while…",
        );
        await dsp.call(
            "reference",
            {
                samples,
                rate: buffer.sampleRate,
                channels: buffer.numberOfChannels,
            },
            [samples.buffer],
        );
        referenceFile = file;
        referenceRevision++;
        settings.reference = "browser-reference";
        $("reference-name").textContent = `Reference: ${file.name}`;
        directionChanged();
        status(
            "Reference measured locally. Master again to use its tonal direction.",
        );
        $("reference-file").value = "";
    });
$("save-report").onclick = () => {
    if (report)
        download(
            JSON.stringify(report, null, 2),
            `${currentFile.name.replace(/\.[^.]+$/, "")}_master.report.json`,
            "application/json",
        );
};
$("cancel").onclick = async () => {
    restoring = true;
    jobRevision++;
    busy = false;
    dsp.cancel();
    clearMaster();
    trackRevision++;
    status("Processing cancelled. Restoring the original track…");
    try {
        if (player.original) await loadWorker();
        status(
            "Cancelled. Your original is ready; Master again to create a new preview.",
        );
    } catch (error) {
        status(error.message, true);
    } finally {
        restoring = false;
        syncControls();
    }
};
$("clear").onclick = () => {
    jobRevision++;
    dsp.cancel();
    trackRevision++;
    player.stop();
    player.original = null;
    clearMaster();
    plots.setAudio(null);
    analysis = null;
    currentFile = null;
    referenceFile = null;
    settings.reference = null;
    $("reference-name").textContent =
        "Optional. Borrow a finished track’s tonal direction.";
    $("track-card").hidden = true;
    $("drop").hidden = false;
    $("measurements").hidden = true;
    $("welcome").hidden = false;
    $("steps").hidden = false;
    $("track-title").textContent = "Shape your sound.";
    $("session-state").textContent = "Your next master starts here";
    status("Ready for a new track.");
    syncControls();
};
for (const type of ["dragenter", "dragover"])
    document.addEventListener(type, (event) => {
        event.preventDefault();
        document.body.classList.add("dragging");
    });
document.addEventListener("dragleave", (event) => {
    if (!event.relatedTarget) document.body.classList.remove("dragging");
});
document.addEventListener("drop", (event) => {
    event.preventDefault();
    document.body.classList.remove("dragging");
    if (!busy && ready) {
        if (document.body.classList.contains("studio-mode"))
            studio.load(event.dataTransfer.files[0]);
        else loadFile(event.dataTransfer.files[0]);
    }
});
window.addEventListener("beforeunload", (event) => {
    if (player.original || studio.loaded) {
        event.preventDefault();
        event.returnValue = "";
    }
});
try {
    if (!isSecureContext)
        throw new Error(
            "Open this app over HTTPS or localhost to use private audio processing.",
        );
    const initial = await dsp.call("init");
    player.importLimits = Object.freeze(initial.import_limits);
    $("import-limits").textContent = limitsSummary(player.importLimits);
    settings = initial.settings;
    defaults = structuredClone(settings);
    for (const id of ["profile", "blend-profile"]) {
        for (const [key, value] of Object.entries(initial.profiles)) {
            const o = document.createElement("option");
            o.value = key;
            o.textContent = value.display_name || key.replaceAll("_", " ");
            $(id).append(o);
        }
        $(id).value = id === "profile" ? "neutral" : "edm";
    }
    $("profile").disabled = false;
    buildBands(settings, eqChanged);
    ready = true;
    $("engine-status").textContent = "RUST · LOCAL";
    status("Your private studio is ready. Add a mix or try the example.");
    syncControls();
    eqChanged();
} catch (error) {
    $("engine-status").textContent = "Engine unavailable";
    status(error.message, true);
}
