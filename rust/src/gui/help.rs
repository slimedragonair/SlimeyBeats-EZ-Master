//! Beginner-facing descriptions shared by labels and their controls.
use eframe::egui;
pub const ANALYZE: &str = "Measure the original files and flag possible problems. This does not change the audio or save a master. Analyze again after adding new files.";
pub const MASTER: &str = "Build a mastered preview from the original file using your current settings. Nothing is exported yet. Compare it, adjust EQ, or re-master before saving.";
pub const REMASTER: &str = "Rebuild from the original file using the current settings. Processing never stacks on the previous master. Your earlier exported files are preserved.";
pub const EXPORT: &str = "Save the selected master with its current EQ settings. This does not re-run automatic mastering. If final EQ measurements are still preparing, export resolves them first. Existing files receive a numbered name instead of being replaced.";
pub const PROFILE: &str = "A starting point for tone and dynamics. Choose a style close to your music, then listen. Built-in profiles are heuristic presets, not validated genre standards. Neutral still processes audio; it is not bypass.";
pub const MODE: &str = "Gentler uses the profile's lower loudness target. Louder aims higher and may reduce punch. Services apply their own playback normalization; no single mastering target suits every destination. Peak and gain limits may leave the result quieter than requested.";
pub const LUFS: &str = "Integrated LUFS measures whole-track perceived loudness. A number closer to zero is louder: -10 is louder than -14. Choose for your sound and delivery needs, then compare at matched volume. Chasing loudness can flatten drum peaks; platform playback normalization is separate.";
pub const CEILING: &str = "The highest allowed reconstructed peak, measured in dBTP. -1 dBTP leaves headroom below digital full scale. Lower it for more headroom; a strict ceiling may prevent a very loud target. Encoded exports are checked again.";
pub const INTENSITY: &str = "Scales the strength of automatic tone and dynamics corrections. Start at 1.00. Reduce it if the master changes the character too much; increase gently. Cleanup and peak protection remain separate.";
pub const SAFETY: &str = "Rechecks low-band punch and selected tonal measurements. It may retry with gentler processing, taking longer. A passing measurement does not replace listening; unresolved concerns are listed in the report.";
pub const REFERENCE: &str = "Choose a finished song whose overall tone you like, preferably in a similar style. Its broad frequency balance guides the master. It does not copy the arrangement or turn an unrelated mix into the reference.";
pub const MATCH: &str = "How strongly the reference's broad tone is followed. Start below 1.00 for a subtle match. Large differences between the songs can still sound unnatural even within the EQ limits.";
pub const MATCH_LOUDNESS: &str = "Use the reference song's measured LUFS as the target. A very loud reference can require heavy limiting; the gain and peak limits still apply.";
pub const MATCH_WIDTH: &str = "Gently move the stereo width toward the reference while keeping low bass centered. This applies only to stereo audio. Compare in mono if a wider setting weakens important sounds.";
pub const DITHER: &str = "Adds a tiny amount of noise when converting to 16- or 24-bit integer audio, reducing low-level quantization distortion. Normally leave it on for a final integer export. Float WAV and MP3 do not use this checkbox's dither.";
pub const SAMPLE_RATE: &str = "Samples per second in the exported file. Keep source rate avoids unnecessary conversion. Choose 44.1 kHz or 48 kHz if your destination requires it. A higher rate cannot restore detail missing from the recording.";
pub const FREQUENCY: &str = "Where the EQ band acts, in Hz. Low numbers shape bass; middle frequencies shape body and vocals; high numbers shape presence and air. Move gradually and listen to the instrument you want to change.";
pub const EQ_GAIN: &str = "How much the selected band is boosted or cut, in dB. Positive adds energy; negative removes it. Try 1–2 dB moves first. Large boosts can consume headroom and trigger more peak reduction.";
pub const Q: &str = "How narrow the EQ band is. A low Q makes a broad, gentle change; a high Q targets a narrow area. Start near 0.7–1.0 for tone shaping. Q also changes the slope/shape of shelves and cuts.";
pub const SHAPE: &str = "Bell boosts or cuts around one frequency. Low/high shelves adjust everything below/above it. Low cut removes deep lows; high cut removes upper highs. Cut filters do not use the gain value.";
pub const HOLD: &str = "Offsets the level change caused by EQ. Uses an estimate while editing, then a full-track loudness measurement. This compensation affects the master and export; listening-only A/B matching does not. Output gain and peak protection can still change final loudness.";
pub const EQ_OUTPUT: &str = "Overall level after EQ and loudness compensation. It does not change the EQ shape. Start at 0 dB. Raising it may cause the peak guard to turn the whole result back down.";
pub const AB: &str = "A plays the original; B plays the rendered master. Switch at the same timeline position to compare tone and punch. Use loudness matching so a louder version does not win just by being louder.";
pub const AB_EQ: &str = "On Manual EQ, A plays the automatic master before manual EQ; B plays the live EQ revision. This isolates the EQ change. Overview compares the original song with the final master. Timeline position is kept when switching views.";
pub const LEVEL_MATCH: &str = "Turns down whichever version is louder. While editing live EQ it uses an immediate estimate, then measured LUFS after the export preparation finishes. It never boosts the quieter track. This affects listening only, not the export.";
pub const VOLUME: &str = "Listening volume only. It does not change the master or export level. Begin quietly and raise to a comfortable level.";

pub fn label(ui: &mut egui::Ui, text: &str, help: &str) -> egui::Response {
    ui.label(text).on_hover_text(help)
}
pub fn module(name: &str) -> &'static str {
    match name {
        "headroom" => {
            "Temporarily lowers the signal before processing so intermediate stages have room to work. The final loudness stage adjusts level afterward."
        }
        "declip" => {
            "Estimates the shape of short flat-topped peaks. It cannot recover information that is permanently lost or repair every kind of distortion."
        }
        "cleanup" => {
            "Removes DC offset and deep rumble. A strong stereo polarity problem may also be corrected. Very low musical bass may change near the high-pass cutoff."
        }
        "denoise" => {
            "Gently reduces high-frequency noise only when quiet, stationary hiss is detected. Leave off if it softens wanted texture; it is not a universal vocal or background-noise remover."
        }
        "hf_rebuild" => {
            "Adds synthetic upper harmonics to band-limited material. It creates a possible impression of brightness; it does not restore the original missing detail. Off by default."
        }
        "static_eq" => {
            "Broad automatic tone correction toward the profile. Cuts and boosts change the balance throughout the song. Lower intensity if the result loses its original character."
        }
        "resonance" => {
            "Looks for narrow upper-mid spectral peaks and applies limited cuts. Musical notes can resemble resonances, so compare before and after."
        }
        "low_end" => {
            "Can add low-bass harmonics when the bass measures weak. These harmonics can improve audibility on small speakers, but they are newly generated color."
        }
        "low_punch" => {
            "Shapes kick attack and bass sustain, and briefly ducks sub-bass around detected hits. It can make rhythm clearer; reduce intensity if the bass pumps or becomes too lean."
        }
        "dynamic_eq" => {
            "Reduces selected frequency regions only when their level rises. It can tame intermittent mud, harshness, or fizz more selectively than a fixed cut."
        }
        "deesser" => {
            "Tames sharp high-frequency bursts such as vocal S and SH sounds. Strong processing may also soften cymbals, so listen for a dull top end."
        }
        "multiband" => {
            "Controls dynamics in separate frequency bands when the profile provides a multiband setup. Can stabilize bass or mids, but may alter punch. It does nothing for profiles without that setup."
        }
        "glue" => {
            "Gentle full-mix compression that brings louder and quieter moments closer together. It can feel cohesive, but too much reduces contrast and impact."
        }
        "saturation" => {
            "Adds a small amount of harmonic color with oversampling. It can feel denser or warmer; turn it off if the mix is already gritty or distorted."
        }
        "stereo" => {
            "Centers low bass and makes limited changes to side information. It does not use a delay-based widening trick. Width changes apply only to stereo input."
        }
        "clipper" => {
            "Rounds the very highest peaks before limiting when the profile calls for it, mainly in Loud mode. This can reduce limiter work but also adds distortion."
        }
        "reference_match" => REFERENCE,
        "limiter" => {
            "Controls peaks while searching for the loudness target. More limiting can flatten transients. Turning this off keeps a final peak-scaling guard, so the loudness target may no longer be reached."
        }
        _ => "Enable or disable this processing stage, then re-master to hear the change.",
    }
}
pub fn format(name: &str) -> &'static str {
    match name {
        "wav24" => {
            "Uncompressed 24-bit WAV: a useful default for a finished high-quality master. Larger than MP3, but lossless."
        }
        "wav16" => {
            "Uncompressed 16-bit WAV: useful for CD-style delivery. Leave dither on when reducing to 16 bits."
        }
        "wav32f" => {
            "32-bit floating-point WAV: useful for further editing. Usually larger and not the most convenient sharing format."
        }
        "flac24" | "flac16" => {
            "Lossless compressed audio. Decodes back to its chosen integer bit depth without MP3-style loss. Smaller than WAV, but not every destination accepts it."
        }
        _ => {
            "320 kbps MP3: smaller and convenient for sharing, with lossy compression. Encoding can change peaks, so the saved file is decoded and checked again."
        }
    }
}
