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
        "By default, your files, decoded audio, measurements, and master stay in this tab’s memory. Local mastering does not upload audio or use server DSP. Loading this site still requests static files from its host. Use server processing is off on every fresh page. Turning it on or choosing Check server reads availability without audio; checks repeat every 30 seconds only while the switch is on and the page is visible. Where guest access is enabled, opting in or choosing Check & connect requests a guest credential kept only in this tab and reads authorized server capabilities. Operator address and token fields remain in advanced settings. Public server availability is not guaranteed.\n\nIn server mode, the current main file can be reused; WAV or MP3 selections bypass local decoding and use the displayed server limits. Master opens server review. Only Upload & master on server, after review of destination and temporary-storage policy and explicit consent, sends the selected audio and its filename. Local settings are paused, and server downloads remain separate from the old local player and exports. No local or server failure silently switches engines. [Server disabled] means the server responded but public processing is disabled. [Setup needed] means the server responded with an incompatible API or connection setup. [Server down] means a network failure, timeout or server error. An online health check alone does not authorize processing: choose Check & connect if prompted. Checks and recovery never upload or resume audio.\n\nSwitching server processing off stops pending upload and status requests but keeps access to remembered jobs for explicit downloads and deletion. Tokens, job references and consent are never saved in preferences, recovery or links. Advanced Disconnect, reloading or leaving clears credentials; a new guest token cannot recover the old guest job. Download or request Cancel & delete before leaving if you need confirmed cleanup. An expiry time reported by the server does not itself confirm deletion.\n\nReloading or closing also discards the local editing session unless you explicitly enabled local audio recovery in Help. That separate opt-in stores recovery audio and settings in IndexedDB on this device. You can disable autosaves, delete saved points, or download a private recovery file yourself. Recovery is best effort, not a replacement for exporting your audio.\n\nLocal browser preview: up to 15 minutes per mono/stereo track, with 384 MiB limits for both source files and decoded PCM. Available device memory can impose a lower limit. There is one active mix plus an optional reference; a new mix replaces the previous one. Supported inputs depend on your browser’s decoder; WAV, MP3, AAC/M4A and FLAC support can vary. Decoding uses a 48 kHz audio context. Export WAV or locally encoded 320 kbps MP3 at that decoded rate. MP3 is lossy and adds short encoder padding; use WAV for exact loops and aligned stems. Report offers a desktop-compatible EQ preset and a ZIP with the rendered master, analysis and settings. Studio ZIP includes its committed mix and available raw stems. These downloads are separate from editing-workspace recovery.\n\nLive EQ needs a current browser with WebAssembly and AudioWorklet, served over HTTPS or localhost. Desktop browser engines have been tested; physical iPhone/Android playback and long-track memory capacity still need device testing. For tracks over 15 minutes, larger banks or FLAC export, use the desktop app. An MP3 exported at the exact 15-minute boundary can exceed the reimport cap because of encoder padding; keep WAV for that boundary. The app original code/art is MIT; the bundled MP3 encoder retains its own LGPL notices and source alongside the site.\n\nOriginal software and artwork: MIT. Your music remains yours; no royalty, watermark, or software credit is required on your output. Rights in any third-party source material still apply.",
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
