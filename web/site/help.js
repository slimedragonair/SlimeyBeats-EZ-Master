import { wireDialogFocus } from "./dialog-focus.js";
let helpModal;
const tips = {
    profile: [
        "Tonal profile",
        "A broad tonal direction, not a rule about your genre. Neutral is a gentle starting point. Other profiles adjust the balance of bass, mids, and treble. Profile changes need Master again. Suggestions use the profile lightly and never apply themselves.",
    ],
    mode: [
        "Loudness mode",
        "Gentler aims for the profile’s more relaxed loudness target. Louder asks for more level and may reduce dynamics. A loudness target is a direction, not a promise: gain limits and punch protection can stop short of it.",
    ],
    lufs: [
        "Integrated LUFS",
        "LUFS measures perceived loudness over the whole track. More negative numbers are quieter. Try −14 LUFS as a starting point, then listen. Streaming services can turn a loud track down; a louder master does not automatically sound better.",
    ],
    ceiling: [
        "True-peak ceiling",
        "The maximum reconstructed peak level, in dBTP. A little space below 0 helps avoid clipping after playback or conversion. −1 dBTP is a useful starting point. The exported WAV is decoded and measured again before download.",
    ],
    intensity: [
        "Correction intensity",
        "Controls how strongly the automatic mastering chain responds. 1.00 is normal; lower values make gentler adjustments. It is not a volume knob. Change this and Master again to hear the result.",
    ],
    safety: [
        "Protect punch & tone",
        "Checks whether processing loses too much transient punch or moves the tone too far. The engine can retry more gently. These checks are heuristics; audition the result and keep the version you prefer.",
    ],
    hold: [
        "Hold loudness",
        "Offsets the overall loudness change caused by EQ, making tonal differences easier to judge. While you drag, the Rust engine uses a fast estimate. Export resolves the whole-track measurement; peak limits can prevent an exact match.",
    ],
    output: [
        "EQ output",
        "A final gain adjustment for the EQ stage. It affects the exported file. The peak guard can turn excessive boosts back down. Listening Volume below the waveform affects your speakers only.",
    ],
    match: [
        "Level match",
        "Turns the louder audition down so A / Original and B / Master are easier to compare fairly. It never changes the exported file. Live EQ uses an estimated loudness change; the final export is measured in full.",
    ],
    dither: [
        "Dither",
        "Adds an extremely quiet noise floor when converting to 16-bit or 24-bit integer audio. This reduces quantization distortion. It is deterministic here and is not used for 32-bit float exports.",
    ],
    bass: [
        "Sub & bass",
        "The lowest part of the sound: weight, kick, and bass. A shelf affects a broad region below the selected frequency. Small moves are usually enough. Excessive bass can use up headroom without adding clarity.",
    ],
    lowmid: [
        "Low mids",
        "Body and warmth live here, but too much can sound muddy or boxy. Try a small cut and compare at matched level. Frequency chooses the center; Q controls how narrow a bell is.",
    ],
    mid: [
        "Mids",
        "The core of voices and many instruments. A bell raises or lowers a region around its frequency. A higher Q makes a narrower change; a lower Q spreads the change over more of the sound.",
    ],
    presence: [
        "Presence",
        "Upper mids help detail and intelligibility. Boost carefully: this area can become sharp or tiring. Cuts can soften harshness without turning the entire track down.",
    ],
    air: [
        "Treble & air",
        "Brightness, cymbal shimmer, and air. The high shelf changes a broad area above its frequency. It cannot recover detail missing from a low-quality source. A high cut removes sound above its frequency; its Gain setting has no effect.",
    ],
    privacy: [
        "Local by default · optional server preview",
        "Local processing is the default and keeps your audio on this device. Server processing is off on every fresh page. Turning on the top switch checks availability and obtains a temporary guest session; no audio is sent until you press the normal Master button. Public server availability is not guaranteed.\n\nUse the same Add audio → Analyze → Master → tweak → Export workflow in either mode. Import and Analyze run locally. With Server processing on, Master sends a float WAV made from the current decoded original, along with your mastering settings and optional reference measurements. Your source filename and reference audio are not uploaded. The server master loads into the same A/B player and real-time EQ. WAV, MP3, reports, EQ presets and session ZIP exports use the normal buttons and run locally. Studio tools remain local. The normal 15-minute and memory limits still apply.\n\nA small info icon beside the switch shows the processing destination and temporary-storage policy. Server files are deleted after the result is received. If deletion fails or you cancel, the job stays available in this tab for a deletion retry; otherwise it expires within one hour. Turning the switch off keeps the loaded master editable. Server failure never silently changes engines or replays audio: Retry only reconnects, and Master starts processing. [Server down], [Server busy], [Server disabled] and [Access needed] explain availability.\n\nCredentials and job references are kept only in tab memory, never preferences, recovery or links. Closing or reloading forgets server access and does not confirm deletion. Local recovery can restore a downloaded server master and its settings without reconnecting or uploading it. Recovery never starts server work.\n\nReloading or closing also discards the local editing session unless you explicitly enabled local audio recovery in Help. That separate opt-in stores recovery audio and settings in IndexedDB on this device. You can disable autosaves, delete saved points, or download a private recovery file yourself. Recovery is best effort, not a replacement for exporting your audio.\n\nLocal browser preview: up to 15 minutes per mono/stereo track, with 384 MiB limits for both source files and decoded PCM. Available device memory can impose a lower limit. There is one active mix plus an optional reference; a new mix replaces the previous one. Supported inputs depend on your browser’s decoder; WAV, MP3, AAC/M4A and FLAC support can vary. Decoding uses a 48 kHz audio context. Export WAV or locally encoded 320 kbps MP3 at that decoded rate. MP3 is lossy and adds short encoder padding; use WAV for exact loops and aligned stems. Report offers a desktop-compatible EQ preset and a ZIP with the rendered master, analysis and settings. Studio ZIP includes its committed mix and available raw stems. These downloads are separate from editing-workspace recovery.\n\nLive EQ needs a current browser with WebAssembly and AudioWorklet, served over HTTPS or localhost. Desktop browser engines have been tested; physical iPhone/Android playback and long-track memory capacity still need device testing. For tracks over 15 minutes, larger banks or FLAC export, use the desktop app. An MP3 exported at the exact 15-minute boundary can exceed the reimport cap because of encoder padding; keep WAV for that boundary. The app original code/art is MIT; the bundled MP3 encoder retains its own LGPL notices and source alongside the site.\n\nOriginal software and artwork: MIT. Your music remains yours; no royalty, watermark, or software credit is required on your output. Rights in any third-party source material still apply.",
    ],
};
export function openHelp(key) {
    const tip = tips[key];
    if (!tip) return;
    document.querySelector("#help-title").textContent = tip[0];
    const body = document.querySelector("#help-body");
    body.replaceChildren(
        ...tip[1].split("\n\n").map((text) => {
            const p = document.createElement("p");
            p.textContent = text;
            return p;
        }),
    );
    helpModal.open();
}
export function wireHelp() {
    document.addEventListener("click", (event) => {
        const button = event.target.closest("[data-help]");
        if (button) {
            event.preventDefault();
            openHelp(button.dataset.help);
        }
    });
    for (const button of document.querySelectorAll("[data-help]"))
        button.title = tips[button.dataset.help]?.[1] || "";
    document.querySelector("#privacy").onclick = () => openHelp("privacy");
    const dialog = document.querySelector("#help-dialog");
    helpModal = wireDialogFocus(dialog);
    dialog.addEventListener("click", (event) => {
        if (event.target === dialog) {
            const r = dialog.getBoundingClientRect();
            if (
                event.clientX < r.left ||
                event.clientX > r.right ||
                event.clientY < r.top ||
                event.clientY > r.bottom
            )
                dialog.close();
        }
    });
}
