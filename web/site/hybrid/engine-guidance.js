import { limitsSummary } from "../audio/import-validation.js";

// Admission and export guidance must describe the selected engine rather than
// showing the local 15-minute ceiling above a larger server upload.
export function renderEngineGuidance({ server, remoteSource, localLimits }) {
    const text = (id, value) => { const node = document.getElementById(id); if (node) node.textContent = value; };
    text("processing-privacy-note", server
        ? "One track at a time. New imports upload to the server for processing."
        : "One track at a time. New imports process on this device.");
    text("import-limits", server
        ? "Server: up to 1 hour per source · mono/stereo · 2 GiB upload and decoded admission. Tool, memory and storage limits apply."
        : localLimits ? limitsSummary(localLimits) : "Local: up to 15 minutes per track; available device memory may limit large tracks.");
    text("drop-limits", server ? "Mono / stereo · Server limits apply" : "Mono / stereo · Up to 15 min");
    text("export-help", remoteSource
        ? "Exports render from full-resolution server audio; preview streams use compressed audio. MP3 is lossy; use WAV for aligned stems and exact loops." +
            (server ? "" : " Turn on Server processing to render this project's exports.")
        : "Export uses the decoded sample rate. MP3 is lossy and can add short padding; use WAV for aligned stems and exact loops. Browser decoding may resample your input.");
}
