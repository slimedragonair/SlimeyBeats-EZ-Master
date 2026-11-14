# Stem live monitoring

This source hotfix keeps desktop **0.5.0-beta.3**, browser Studio **0.3.0-beta.3** and MCP companion **0.2.0-beta.3** unchanged. It separates immediate stem listening from preparing a measured revision for saving. See the [Studio workflow](STUDIO.md#desktop-workflow) and [user guide](USER_GUIDE.md#reading-the-controls).

## Listening and saved previews

- Gain, mute, solo and stereo balance update running playback from the immutable stem bank. A control change automatically selects Preview/B. Mute wins over solo; when any part is soloed, other parts are excluded.
- A plays the revision's Before input. Settled B plays only the live bank mix or prepared preview; Before is not layered underneath it. Listening volume does not change saved audio.
- **Update mix preview** explicitly renders and measures the balance before preview export, desktop project saving, mastering handoff or subsequent voice/repair/plugin processing. Moving a fader does not save audio or create a project. Raw stem exports retain bank audio and relative levels, independent of mix faders.
- Native **Level match** is disabled while the live mix is unmeasured, until the update completes. The comparison waveform is labelled **saved preview** while live fader changes are not plotted. New voice/repair/plugin settings still require their own explicit **Preview** action.

## Native data flow and ownership

[Mix controls](../rust/src/gui/studio/mix_controls.rs) call [live_mix.rs](../rust/src/gui/studio/live_mix.rs), which marks the mix pending and sends channel-gain targets through the latest-update mailbox in [live_stem_path.rs](../rust/src/gui/live_stem_path.rs). [player.rs](../rust/src/gui/player.rs) connects the bank and mailbox to the running [audio_source.rs](../rust/src/gui/audio_source.rs). [realtime_mix.rs](../rust/src/studio/realtime_mix.rs) reads immutable stem samples, ramps gain changes over **5 ms** and passes the sum through the [streaming peak guard](../rust/src/dsp/stream_peak_guard.rs). The callback polls updates without waiting for the UI; fader gestures do not render full-track PCM.

[transport.rs](../rust/src/gui/studio/transport.rs) owns comparison, level-match gating and saved-waveform labels. [preview_actions.rs](../rust/src/gui/studio/preview_actions.rs) keeps materialization and export explicit. Materialization uses the existing [stem_mix.rs](../rust/src/studio/stem_mix.rs) renderer and revision flow; it does not mutate the source bank.

[playback_owner.rs](../rust/src/gui/playback_owner.rs) gives only the active workspace permission to sound. It closes and pauses the outgoing player's gate before opening the incoming gate. [shell.rs](../rust/src/gui/shell.rs), [workspace_tabs.rs](../rust/src/gui/workspace_tabs.rs) and [learning_ui.rs](../rust/src/gui/learning_ui.rs) use this ownership rule, including ZIP-drop and tutorial navigation. An inactive player cannot leak underneath the visible workspace, including during fades or late job completion.

## Browser data flow

[studio.rs](../web/dsp/src/studio.rs) and [studio-worker.js](../web/site/studio-worker.js) own immutable PCM and proposed saved revisions. [studio-player.js](../web/site/studio-player.js) retains reusable stem AudioBuffers and transport; [studio-stem-mixer.js](../web/site/studio-stem-mixer.js) connects aligned sources through smoothed per-channel Web Audio gains. [studio-app.js](../web/site/studio-app.js) routes control changes directly to playback and separately guards operations requiring an updated preview. A fader gesture does not run an offline mix job or copy the bank again.

Browser monitoring uses conservative **sample headroom**, rather than the native streaming peak guard or the final offline true-peak checks. The saved render/export retains those final checks and may differ from monitoring attenuation. [studio-preview-status.js](../web/site/studio-preview-status.js) identifies live B and the pending saved preview; [studio-plots.js](../web/site/studio-plots.js) continues to draw saved audio.

## Invariants and extension points

Keep live controls separate from committed audio/revisions. Add future mix controls through both live gain calculation and the saved renderer, preserving mute/solo/balance semantics. Keep all native navigation on the central ownership path. Never apply a saved preview's loudness measurement or waveform to an unmeasured live balance.

This remains an aligned mixer: at least two parts must have matching sample rate, channels and frame count. There is no fixed imported-part ceiling, automatic alignment or resampling. Native input defaults remain **512 MiB per input** plus a **4 GiB aggregate decoded-bank guard**; browser Studio retains **128 MiB combined decoded input** and **15 minutes per track**. Archive, metadata and project safeguards remain; playback/processing/history/export need additional RAM. Model separation remains two/four stems, with no new separation-quality promise. MCP continues its explicit preview/project/export workflow.

## Verification boundary

Candidate-specific checksums, source commit, native/browser regressions and package verification are recorded in the existing beta.3 release's **HOTFIX-VALIDATION.json** and **HOTFIX-SHA256SUMS.txt** assets. Earlier [beta.3 qualification](STUDIO.md#beta3-qualification) remains historical and does not substitute for that evidence. The regression tests inspect final native output and captured Web Audio PCM, including muted/soloed parts, Before exclusion, transport retention and pending resume cancellation. Physical-device listening, universal plugin compatibility and improved neural separation quality are not established by these checks.
