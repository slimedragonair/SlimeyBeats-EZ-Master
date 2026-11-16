import { uploadWav } from "./pcm-transfer.js";

// Called only by the ordinary Master button. Merely selecting the server or
// importing/analysing a file never creates an audio job.
export async function runServerMaster(session, audio, settings, reference, { signal, progress = () => {} } = {}) {
    const current = () => signal?.throwIfAborted();
    current();
    if (!session.processingEnabled || !session.connected || !session.available)
        throw new Error("Server unavailable. Retry the server connection, or turn the switch off to master locally.");
    if (!session.caps.formats.includes("wav32f"))
        throw new Error("This server needs an update to return an editable master. Your track and settings are unchanged.");
    if (session.job) {
        // A preceding interrupted attempt must be cleaned up before another
        // source is sent; keep its credential until deletion is acknowledged.
        await session.delete(); current();
        if (session.job) throw session.lastFailure || new Error("Previous server files could not be deleted. Retry, or use local processing.");
    }
    const file = uploadWav(audio);
    if (file.size > session.caps.limits.max_upload_bytes)
        throw new Error("The decoded track exceeds the server upload limit. Use local processing or desktop.");
    session.select(file);
    // The explicit server switch plus this Master action authorizes this job.
    // This is protocol authorization, not another checkbox or review screen.
    session.consentToUpload(true);
    if (!session.canSubmit) throw new Error("The server cannot start this master. Check its status or use local processing.");
    const notify = session.onChange;
    session.onChange = (value) => {
        notify(value);
        const job = value.job;
        if (job) progress(job.state === "uploading" ? .2 * job.uploaded_bytes / job.total_bytes : .2 + .6 * job.progress,
            job.state === "uploading" ? "Sending your track to the server…" : `Server · ${job.message}`);
    };
    const abort = () => session.cancel();
    signal?.addEventListener("abort", abort, { once: true });
    try {
        await session.submit({ profile: settings.profile, format: "wav32f", mode: settings.mode,
            target_lufs: settings.target_lufs, ceiling_dbtp: settings.ceiling_dbtp,
            intensity: settings.intensity, safety: settings.safety, reference_analysis: reference });
        current();
        if (session.lastFailure) throw session.lastFailure;
        if (session.job?.state !== "completed")
            throw new Error("Server mastering did not complete. Your original and settings are kept; retry or master locally.");
        const job = session.job;
        const audioArtifact = job.artifacts.find((a) => a.content_type === "audio/wav");
        const reportArtifact = job.artifacts.find((a) => a.content_type === "application/json");
        if (!audioArtifact || !reportArtifact || reportArtifact.bytes > 65536)
            throw new Error("The server did not return compatible master audio and measurements.");
        progress(.85, "Loading the server master into your editor…");
        const samples = await session.client.masterAudio(job.id, audioArtifact, {
            frames: audio.length, channels: audio.numberOfChannels, rate: audio.sampleRate,
        });
        current();
        const result = await session.client.artifact(job.id, reportArtifact);
        current();
        const metadata = JSON.parse(await result.blob.text());
        current();
        // The local PCM now carries the result. Confirm remote cleanup, but do
        // not throw away a completed master if that cleanup request fails.
        await session.delete(); current();
        return { samples, metadata, cleanupPending: !!session.job };
    } finally {
        session.onChange = notify;
        signal?.removeEventListener("abort", abort);
    }
}
