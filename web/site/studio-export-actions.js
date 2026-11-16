import { verifiedMp3 } from "./audio/verified-mp3-export.js";
import { analyzeSavedAudio } from "./audio/saved-audio-analysis.js";
import { packageSession } from "./session-export-client.js";
import { stemZip } from "./studio-zip.js";
import { download } from "./view.js";

export function wireStudioExports({ getState, client, playback, job, promote, notice }) {
    const $ = (id) => document.getElementById(`s-${id}`);
    let abort;
    for (const id of ["export", "export-stems", "export-session", "promote"])
        $(id).onclick = () => {
            if (getState().busy) return;
            return job(id === "promote" ? "Preparing the mastering source…" : "Preparing a private download…", async () => {
                abort = new AbortController();
                const signal = abort.signal, state = getState(), format = $("format").value;
                const current = () => {
                    if (getState().cancelled || signal.aborted) throw new DOMException("Export cancelled", "AbortError");
                };
                try {
                    if (client.remote) {
                        const project = client.project, name = id === "export-stems" ? "Studio-stems.zip" : id === "export-session" ? "Studio-session.zip"
                            : `Studio-preview.${format === "mp3" ? "mp3" : "wav"}`;
                        if (id === "promote") {
                            const audio = await project.audio(project.manifest.preview_id);
                            current(); await promote({ ...audio, name: "Studio mix.wav" });
                            notice("Studio preview loaded for server mastering. Your Studio workspace is retained."); return;
                        }
                        await project.export(id === "export" ? "export" : "bundle", {
                            ...(id === "export-stems" ? {} : { audio_id: project.manifest.preview_id }),
                            format, ...(id !== "export" ? { kind: "studio", include_stems: true } : {}),
                        }, name, { signal });
                        current(); notice("Server download started from the full-resolution saved audio. Listening volume is excluded."); return;
                    }
                    if (id === "export-stems") {
                        const result = await client.call("stem-export"); current();
                        download(stemZip(result.files, result.manifest), "Studio-stems.zip", "application/zip");
                    } else if (id === "promote") {
                        const result = await client.call("working"); current();
                        await promote(new File([result.bytes], "Studio mix.wav", { type: "audio/wav" }));
                        notice("Studio preview loaded for mastering. Your Studio workspace is retained.");
                        return;
                    } else {
                        const result = await client.call("export", { format: format === "mp3" ? "wav32f" : format }); current();
                        let bytes = result.bytes, analysis, encodedMetadata;
                        const extension = format === "mp3" ? "mp3" : "wav";
                        if (format === "mp3") {
                            const encoded = await verifiedMp3(bytes, { context: playback.context(), client,
                                ceiling: -1, signal, onProgress: (_, text) => notice(text) });
                            bytes = encoded.bytes; analysis = encoded.analysis; encodedMetadata = encoded.metadata;
                        }
                        current();
                        if (id === "export-session") {
                            if (!analysis) analysis = (await analyzeSavedAudio(bytes, { context: playback.context(), client, signal })).analysis;
                            current();
                            const bank = state.info.stems.length ? await client.call("stem-export") : { files: [] };
                            current();
                            const blob = await packageSession("studio", { audio: { name: `mix.${extension}`, bytes },
                                ...bank, analysis: { output: analysis, exports: encodedMetadata ? [encodedMetadata]
                                    : [{ format, sample_rate: state.info.sample_rate, decoded_metrics: analysis.metrics }] },
                                info: state.info }, { signal });
                            current();
                            download(blob, "Studio-session.zip", "application/zip");
                        } else download(bytes, `Studio-preview.${extension}`, format === "mp3" ? "audio/mpeg" : "audio/wav");
                    }
                    notice("Download prepared from the current committed preview. Listening volume is not applied.");
                } finally { abort = null; }
            }, id !== "export-stems");
        };
    return { cancel: () => abort?.abort() };
}
