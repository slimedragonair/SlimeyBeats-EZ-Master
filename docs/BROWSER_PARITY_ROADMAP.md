# Browser parity and tester feedback

Desktop remains the main workstation. The browser should offer the same understandable import → edit → compare → export workflow wherever device resources permit, with all audio processing on the user's device. A responsive browser view is not a qualified native phone app.

This roadmap ranks the November 15, 2026 suggestions by approximate implementation difficulty, rather than promising dates. Existing features need extension and testing, not duplicate implementations. Preview fidelity takes priority even though its difficulty depends on reproducing the problem.

The first development pass implements portable EQ JSON, local MP3 and separate mastering/Studio session ZIP downloads, plus bounded live EQ preparation and clearer startup/export feedback. It does not ship the later issue detector, browser neural separation or restoration research. See [workflows](WEB_EXPORTS.md) and [validation](WEB_EXPORT_VALIDATION.md); published binaries and Pages deployment remain separate.

The latest development checkpoint ports selected desktop stem-editor tools to the
browser: a shared timeline, precise selection and undoable silence/fade/gain/reverse
edits with recovery replay. See [the browser editor's scope and current checks](WEB_STEM_EDITOR.md).
Issue overlays, activity classification, movable clips and optional models remain
separate work; this checkpoint does not claim complete desktop parity.

| Order | Fix or addition | Work and acceptance boundary |
| --- | --- | --- |
| First | Investigate browser live EQ quality | Compare actual worklet samples with shared Rust, rapid edits, callback work, A/B routing and decoder behavior. Matching offline PCM alone cannot rule out playback glitches. Actual tester audio/device reproduction remains needed. |
| 1 · Small | Optional donation link | Add the owner's real Ko-fi address when supplied; never invent an account, require payment or restrict editing. |
| 2 · Small | Clearer browser status and help | Identify saved preview versus live edits, explain formats, show pending work and keep errors beside the action. |
| 3 · Small | Portable EQ/settings JSON | Export a versioned recipe with units and an explicit distinction between settings and measurements. Keep private local paths out. |
| 4 · Small–medium | Karaoke from supplied stems | Mute or attenuate identified vocal parts. Separate lead/backing controls require separate stems; this does not isolate vocals from a mixed recording. |
| 5 · Small–medium | Stem activity overview | Plot measured energy over time, with visible thresholds. Energy does not prove an instrument is present or absent. |
| 6 · Medium | Browser MP3 export | Encode locally in a worker, allow cancellation, decode and check the saved file, and keep listening volume out of it. Retain WAV. |
| 7 · Medium | Clipping and silence regions | Add timestamped markers and a legend with measurable thresholds. Sample clipping and reconstructed true peak are different findings. Never trim automatically. |
| 8 · Medium | Possible false endings | Flag a quiet gap followed by resumed music for review. Deliberate pauses and hidden sections are valid music. |
| 9 · Medium | Session download ZIP | Include the current rendered mix, available imported stems, analysis, EQ/settings and a versioned manifest. A bundle cannot contain separated stems unless they exist. A download bundle is not automatically an editable native project. |
| 10 · Medium–large | Browser editor parity | Initial timeline, selection, fades, gain, silence, reverse and undo are on `current-work`, with touch/keyboard checks and recovery. Loop auditioning, movable clips and larger arrangements remain separate work. |
| 11 · Medium–large | Tonal issue overlays | Offer conservative, explainable mud/harshness/rumble hypotheses. High band energy alone is not distortion or an objective defect. |
| 12 · Large | Larger browser projects | Incremental decoding, bounded rendering/export and storage-aware editing before raising duration/memory guards. Desktop stays the option for larger sessions. |
| 13 · Large | Drum clarity assistance | Audition modest transient/tone changes on a drum stem. Missing cymbal definition cannot be diagnosed from a text description, and a treble boost cannot recreate missing detail. |
| 14 · Large | Chord/key analysis | Estimate chords over time with confidence and abstention for ambiguous, percussive or dense sections. |
| 15 · Large | Audacity integration | Investigate a compatible effect/plugin around reusable Rust processing, then validate actual Audacity import, parameters and renders. Native plugin support is separate from a browser feature. |
| 16 · Very large | Browser neural stem separation | Optional local models need tested licenses, explicit download sizes, supported backends, progress/cancel, quality review and real mobile memory measurements. No hidden server fallback. |
| 17 · Very large | Lead/backing vocal separation | Depends on a suitable model or separately supplied parts, with leakage/artifact evaluation rather than a universal removal promise. |
| 18 · Experimental | Emotional peak overlay | Start with dynamics/activity observations. Emotional labels are subjective estimates that need a suitable evaluated model. |
| 19 · Research | Instrument spectra and harmonic reconstruction | Instrument/timbre context matters. Estimated synthesis cannot establish what an original missing harmonic was; preserve the original and require explicit audition. |

## Delivery sequence

1. Protect preview fidelity; add private MP3, EQ/settings and complete session downloads.
2. The first timeline/selection tools are implemented; follow with measurable issue markers and activity views while extending practical editor parity.
3. Expand practical mobile support with physical-device checks and bounded project processing.
4. Evaluate optional models, plugin integration and restoration separately, after the basic editor is dependable.

Keep MCP discovery/help synchronized when desktop engine tools are added. Browser-only packaging controls do not imply a new native DSP operation. Released installers and the main hosted site follow their release/main workflows. The separately labeled `current-work` web preview publishes development checkpoints after its own checks; it does not promote them into main or a desktop installer.
