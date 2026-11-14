# UI, UX and learning pass

The interface preserves Analyze → Master → live EQ → explicit Export and the existing SlimeDragon artwork. Basic/Advanced disclosure, first-use learning and contextual Help belong to the interface layer; they do not change the audio engine's processing order.

This note maps the implementation and its extension points. Candidate-specific results belong to validation reports; a successful compilation or a captured screen does not establish general usability or physical-device compatibility.

## Product contract

- Basic is the starting view; Advanced reveals precise or less frequent controls. Switching visibility must never reset or silently change processing settings. Surface active custom settings when hiding their editors.
- Essential actions, playback, job cancellation, export destination/format and errors remain reachable. Keep Analyze at the top right and Master separate from Export.
- Offer a short tutorial once for a new preference profile. Completion and explicit decline/dismissal save immediately and suppress future automatic offers. Replay stays available through Help. Never run processing, download a model or export audio merely to advance a tour.
- Add a bottom-right Help entry point with touch/keyboard access, tutorial replay, plain-language terms, troubleshooting and MCP setup. Web help must explain that the companion is a desktop capability; it must not add an upload or localhost bridge.
- Share educational copy through `rust/assets/learning-guide.json`, embedded by the native GUI and copied to web assets by the static build. Keep controls native and editable.
- Continue using the current preference locations and atomic persistence. Web stores only small UI choices, with clear handling when browser storage is unavailable. No persistent audio cache is added.

## Ownership and verification

Native `user_experience.rs` coordinates interface preferences and view state; `learning_ui.rs` and `help_center.rs` render guidance. `interface_preferences.rs`, `tutorial_history.rs` and `preference_store.rs` separate the saved UI choice from audio settings and provide atomic persistence. The one-way tutorial marker prevents a stale window from restoring an automatic offer. Replay never removes it.

The sidebar coordinator delegates mastering direction, delivery and processing options to `gui/mastering_controls`. EQ has a contextual band inspector. Studio keeps source waveforms and faders in the same workspace, with pending edits distinct from the committed preview. Visibility changes preserve exact parameters: native sliders use edit-only clamping rather than rounding a stored value to its display precision during paint.

Mastering playback reserves its measured panel height before laying out scrollable content. Waveform labels, traces and time ticks have separate space; short requests cannot reduce a trace to a line of dots. Compact Overview makes the frequency spectrum an explicit disclosure and reveals the expanded graph in the scroll area. Diagnostic captures wait for both layout passes and elapsed presentation time, so modal fade-in is not mistaken for settled appearance.

`workspace_tabs.rs` measures the actual native buttons, adapting the report label and decorative icons to the space available with platform fonts. Sidebar disclosures use measured wrapping; scroll content reserves a physical pixel against fractional-DPI rounding. Noninteractive skin artwork uses the shared `theme::DECORATION_ORDER` below popup/modal layers, including when a compact tutorial crosses several pane borders.

`mcp_setup.rs` prepares configuration through the actual companion's bounded `--print-config` process. Generation runs off the UI thread and can be cancelled when Help closes. It does not start a server or modify an AI client's settings. `bundled_resources.rs` locates installed resources and explicit development paths without scanning a music library.

The browser separates UI storage, disclosure, dialogs, viewport reserves, job feedback and local error feedback into dedicated modules. The static build copies canonical guide JSON into web assets. Small interface preferences remain separate from audio buffers; no persistent audio cache or browser-to-desktop bridge is added.

Native accessibility uses the existing eframe AccessKit integration. Dialogs have explicit accessible names, core fields are associated with their labels, and headless semantic-tree checks complement pointer/keyboard tests. These are building blocks, not a claim of screen-reader or accessibility-standard conformance.

Review main tasks with empty, loaded, working, processing, pending-edit, stale-preview and error states. Target native 1100×720 and 1480×940, browser desktop and 320/390/430 CSS-pixel phones, keyboard/touch input, enlarged text, and bottom safe areas. Reviewers must distinguish observed behavior, appearance and remaining physical-device limits. Iterate substantive findings; do not turn a screenshot or compilation into a universal usability claim.

When extending the interface, add controls to the appropriate feature module, keep copy in the shared guide where both platforms use it, preserve terminal tutorial choices across content updates, and test the smallest supported view before adding more chrome. Continue independent review after fixes rather than carrying a score from an older candidate.
