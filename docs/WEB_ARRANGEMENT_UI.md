# Web arrangement interface

The `web/site/arrangement-timeline*` modules provide the local arrangement
interface over the shared [arrangement core](ARRANGEMENT_CORE.md). They do not
weaken `StemSet`, dispatch clips as stem kinds, mix PCM in JavaScript, or invent
another project format. Their DOM/gesture tests use mocked replies and do not
qualify audio playback. Product exposure requires a qualified adapter and a
successfully installed real preview source.

## Responsibilities

- `arrangement-timeline-controller.js`: adapter calls, revision/source guards,
  exposure gate, bounded waveform requests, preview installation and lifecycle.
- `arrangement-timeline-state.js`: selected stable IDs, project/source geometry,
  navigation, uncommitted gestures and explicit EQ targets.
- `arrangement-timeline-view.js` and `arrangement-timeline-waveform.js`: virtual
  lane rows and bounded numeric asset envelopes. Playhead updates never scan PCM.
- `arrangement-timeline-interaction.js`: pointer/keyboard gestures. One edit is
  submitted on release; cancellation restores UI selection and sends no edit.
- `arrangement-timeline-inspector.js`, `arrangement-timeline-commands.js` and
  `arrangement-timeline-drafts.js`: precise typed fields, core edit payloads,
  stable-ID drafts and explicit resolution after conflicting metadata.
- `arrangement-timeline-import.js`: selected/omitted part and encoding review.
  One file, repeated labels and differing durations are supported by the
  separate adapter; the aligned-stem importer remains unchanged.
- `arrangement-timeline-eq*.js`: explicit target, draft settings, current/proposed
  core response curves, unapplied suggestions and stale-result guards.
- `arrangement-timeline-template.js`, `arrangement-timeline.css` and
  `arrangement-timeline-fields.js`: scoped markup, existing theme variables,
  responsive controls and stable DOM field updates.
- `arrangement-studio-workspace.js` and `studio-transport-controls.js`: Studio
  pane selection and routing of the existing pinned controls to the selected
  real transport. Legacy assets and positions remain separate.
- `arrangement-timeline-project.js`: explicit canonical project Save/Open and
  whole-arrangement WAV/MP3 delivery. These operations pause audition at its current
  position and never accept pending fields or EQ proposals.

## Adapter boundary

`createArrangementTimeline(host, options)` receives an adapter created by the
separate `createArrangementAdapter` transport facade. It calls `read`,
`reviewImport`, `addImport`, `discardImport`, `edit`, `undo`, `redo`, `waveform`,
`suggestEq`, `eqResponse` and `previewSource`. `edit` accepts one core `Edit` or
an atomic array of edits with an expected revision. Imports and metadata edits
never trigger an audio export.

Snapshots contain `revision`, `arrangement`, asset metadata, history labels,
capabilities and prepared-preview metadata. `previewSource` must provide the
actual arrangement transport for the requested prepared revision. The controller
checks its identity, rate, channel count and duration before `installPreview`
routes the pinned player to it. Failure retains the earlier playable source and
reports the saved/preview distinction. Parent metadata notifications call
`refresh`; playback position updates use the transport without polling full
project snapshots each audio quantum.

The injected `position()` and `seek(frame)` use project sample frames. The
transport itself also exposes second-based methods for existing pinned controls.
`defaults()` supplies the shared core's seven default EQ bands, not UI filter
coefficients. Leaving the pane pauses its transport without seeking; entering it
does not autoplay.

A is the document before the latest accepted edit, including Undo/Redo; B is
the current arrangement. Running A/B preserves playback, and paused A/B stays
paused. Listening volume is excluded from processing. Comparison levels are
not matched. Selected-side duration can differ from the current document's
duration, and a zero-length A must still allow returning to B.

The integration gate is `ARRANGEMENT_UI_ENABLED` in
`arrangement-studio-workspace.js`. It is enabled after local worker/worklet and
full responsive UI qualification. The entry still stays hidden when the
runtime lacks working playback or cannot prepare a matching source.
Only an adapter advertising working ZIP import exposes Add ZIP. Project saving
and server playback have separate adapter qualifications. Canonical project
ZIPs retain applied metadata, immutable assets and Undo/Redo history. Invalid
projects preserve the current workspace. Legacy recovery cannot silently claim
to include arrangement tracks; use Save project for those tracks.

## Coordinate and interaction rules

Clip `start_frame`/exclusive `end_frame` are song coordinates.
`source_start_frame` is an immutable asset coordinate. A trim or move never
changes source samples. Mix/track EQ ranges use song frames; clip EQ ranges use
source frames:

```
source = clip.source_start_frame + selected_song_frame - clip.start_frame
```

The target panel shows the selected track/clip and exact bounds. A missing
selection disables that scope instead of falling back to the whole mix. Time
limited EQ needs at least 20 ms; whole-target EQ and clip editing retain their
core support for single-frame clips. The plot shows the selected EQ effect;
other active effects may combine with it.

Mouse clip moves can cross tracks vertically. A vertical touch gesture cancels
before a horizontal move locks so the lane list can scroll. Exact timing,
destination-track fields and nudge buttons provide a touch/keyboard alternative.
Track reordering has an explicit grip plus up/down actions. Pointer capture
belongs to the stable viewport because virtualization can replace clip DOM.
Releasing outside that viewport still submits one edit. Fit selection frames
the chosen time range or clip, with a minimum 10 ms view for single-frame parts.

## Checks

Run the pure state/command/EQ tests and the separate DOM fixture:

```sh
node --test web/site/arrangement-timeline-*.test.mjs
python web/tools/verify_arrangement_ui.py --out <scratch>/arrangement-ui
python web/tools/verify_arrangement_browser.py --site <matching-build> --out <scratch>/arrangement-browser
python web/tools/verify_arrangement_project_ui.py --site <matching-build> --out <scratch>/arrangement-project-ui
python web/tools/verify_arrangement_mp3_ui.py --site <matching-build> --out <scratch>/arrangement-mp3-ui
```

The DOM fixture checks 320/390/1100px, adapter-unavailable hiding, 1000-lane
virtualization, pointer release/cancellation and exact numeric payloads. It
deliberately does not claim real core playback, export or physical mobile
acceptance. Those require the actual WASM/server arrangement adapter workflow
before the new pane is made available in the app.

The separate browser workflow imports real synthetic mono/stereo WAVs of
different durations, exercises exact typed core commands, and captures actual
AudioWorklet output. It checks live gain, scoped EQ, running/paused A/B, Stop,
stable effect IDs, source-coordinate ranges, unapplied proposals, ZIP review
and clip/track history. It requests no audio exports or downloads. Use the
matching worker/client/bindings/WASM as a frozen unit while overlaying UI files;
an in-progress worker API cannot use a previous build's WASM.

The focused project UI probe exercises Save/Open, corrupt-project preservation,
restored history and source-frame EQ, plus an explicit Float32 WAV download. It
independently reads saved PCM and compares the scoped filter gain with captured
live output after accounting for listening volume. It supports both ordinary
IEEE-float and WAVE_FORMAT_EXTENSIBLE float headers. These explicit synthetic
downloads are separate from the no-download editing workflow.

## Qualification recorded November 16, 2026

| Scope | Result |
| --- | --- |
| State, commands, drafts and EQ context | 13 Node tests passed |
| DOM/gesture fixture | Chromium 143, Firefox 144, Linux WebKit 26 at 320/390/1100px passed |
| Actual editing, ZIP review, scoped EQ and A/B PCM | All three engines at 320/390px passed |
| Actual Save/Open, corrupt preservation and live/render Float32 WAV parity | All three engines at 320/390px passed |
| Actual MP3 delivery, independent FFmpeg decode and first-import overview | All three engines at 320/390px passed |
| Existing aligned-stem live-player regression | Chromium 143 passed after shell routing changes |

The editing build used frozen adapter runtime `fa8f8584c9b60c9f` with UI overlay
`9732b697b1a509c4`; project checks used frozen adapter `611095e5ced8155c` with UI
overlay `a3d0fe6922074889`. These use synthetic sources and headless audio
devices. Physical mobile audio, subjective listening and server arrangement
streaming remain unqualified. Windows bundled WebKit without AudioWorklet is
unsupported and must not expose the arrangement entry. Earlier failures remain
separate from the passing reports.

The matching checkpoint build `d951872d1d310049` was tested directly without UI
overlays: 18 browser cases and 78 grouped checks passed across those three
engines and two widths. Reports are `checkpoint-<engine>-browser`,
`checkpoint-<engine>-project_ui` and `checkpoint-<engine>-mp3_ui` in the private
arrangement qualification folder. MP3 delivery reuses the existing verified
320 kbps encoder from the applied Float32 render; it adds no encoder dependency
or mastering pass. Independent decode measured 48 kHz stereo, correct applied
track gain and an unchanged second track. Project Save remains the canonical
asset/history ZIP regardless of delivery format. First import fits the entire
song; later metadata revisions retain the chosen timeline view.

## C-owned checkpoint files

These are the complete local UI and qualification files owned by C. Adapter,
WASM, shared Rust and server backend changes are owned by the other contributors.

```text
docs/WEB_ARRANGEMENT_UI.md
web/site/studio-app.js
web/site/studio-template.js
web/site/studio-transport-controls.js
web/site/arrangement-studio-workspace.js
web/site/arrangement-timeline-commands.js
web/site/arrangement-timeline-commands.test.mjs
web/site/arrangement-timeline-controller.js
web/site/arrangement-timeline-drafts.js
web/site/arrangement-timeline-eq-curves.js
web/site/arrangement-timeline-eq-state.js
web/site/arrangement-timeline-eq-state.test.mjs
web/site/arrangement-timeline-eq.js
web/site/arrangement-timeline-fields.js
web/site/arrangement-timeline-import.js
web/site/arrangement-timeline-inspector.js
web/site/arrangement-timeline-interaction.js
web/site/arrangement-timeline-project.js
web/site/arrangement-timeline-state.js
web/site/arrangement-timeline-state.test.mjs
web/site/arrangement-timeline-template.js
web/site/arrangement-timeline-view.js
web/site/arrangement-timeline-waveform.js
web/site/arrangement-timeline.css
web/tools/arrangement_audio_audit.js
web/tools/arrangement_ui_probe.js
web/tools/verify_arrangement_browser.py
web/tools/verify_arrangement_mp3_ui.py
web/tools/verify_arrangement_project_ui.py
web/tools/verify_arrangement_ui.py
```

The additive server media foundation changes are
`web/site/hybrid/remote-media-policy.js`, `remote-media-policy.test.mjs` and
`server-client.js`: twelve existing/new policy and client checks passed. The
new `arrangement-stream-contract.js`, `arrangement-stream-preview.js` and
`arrangement-stream-preview.test.mjs` in that folder are currently unwired;
their five clock/state tests do not qualify server playback. The server adapter
and real HTTP/audio workflow remain a separate integration phase.
