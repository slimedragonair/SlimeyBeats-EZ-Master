# Web arrangement interface

The `web/site/arrangement-timeline*` modules provide the arrangement
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
- `arrangement-studio-workspace.js`, `arrangement-studio-pane.js` and `studio-transport-controls.js`: Studio
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

Browser EQ fields are drafts: use **Update EQ preview** to hear a manual change
on the running arrangement. Suggested curves use a separate **Apply curve**
action. Neither operation needs a whole-song export, and saving/exporting keeps
unapplied fields out of the result. Native sliders audition during adjustment.

The final gated runtime `e44d3ddac99e5bf3` also passed actual Chromium 143 clicks
at 320/390 pixels: eight checks, no errors, no server project/upload/download,
retained local drafts, and identical captured local PCM after Server Off. Legacy
server source/import and mix/voice controls remained reachable.

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

## Low-level AudioWorklet probe QA

The November 16, 2026 rerun of `web/tools/verify_arrangement_audio.py` recorded:

| Check | Tested scope | Result |
| --- | --- | --- |
| Generated WASM, actual worklet PCM and delivery checks | Linux Chromium 143, Firefox 144 and WebKit 26 | Passed |
| Real capture delayed by 1.2 seconds | Linux Chromium 143 | Original probe failed; updated probe passed |
| Worklet running without PCM delivery | Linux and Windows Chromium 143 | Failed at the configured 1-second deadline, as required |

Capture uses tagged, contiguous PCM blocks after 200 ms of audio-clock settling,
with a default 10-second deadline. It waits for actual samples, not a desired RMS.
Numeric assertions remain unchanged: baseline RMS 0.06–0.08, −6 dB gain ratio
0.47–0.54, Before-to-baseline RMS ratio 0.95–1.05, and exact zero RMS after Stop.
Source immutability and WAV/MP3 delivery assertions also remain in place.

Repeat the delayed-arrival positive check with `--capture-start-delay-ms 1200`.
Use `--capture-suppress-pcm --pcm-timeout-ms 1000` for the negative check; its
nonzero exit is expected. Failure reports retain context-clock, capture-window
and sink diagnostics. These results do not establish that the failed hosted CI
job has passed; that requires a run containing the updated probe.

The later `cf11c55` CI run recorded a WebKit gain ratio of 0.573886, outside
the unchanged 0.47–0.54 assertion, after a low baseline measurement. A focused
WebKit rerun passed at 0.501236 with Stop RMS zero; this does not explain the
CI failure. The probe now saves raw Float32 PCM, block geometry and zero-run
diagnostics so another failure can be inspected without guessing from RMS.
The separate browser workflow also timed out downloading a restored stem ZIP
in Firefox. Its unchanged focused and full Firefox recovery reruns passed
16 groups on the final gated build. No cause or product fix is claimed for
either intermittent failure; subsequent CI results remain separate evidence.

## Local editor files

These files implement and validate the local interface. Adapter, WASM, shared
Rust and server backend modules provide the processing and persistence layers.

```text
docs/WEB_ARRANGEMENT_UI.md
web/site/studio-app.js
web/site/studio-template.js
web/site/studio-transport-controls.js
web/site/arrangement-studio-workspace.js
web/site/arrangement-studio-pane.js
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

## Server adapter and qualification

`hybrid/arrangement-remote-adapter.js` checks the actual server arrangement and
playback capabilities before creating a project. Server On routes the existing
Arrange tracks action and pinned transport to that adapter. When the server
workspace is fresh, that explicit action continues a loaded local document via
its canonical Project Save/Open path, retaining source IDs, history, selection,
paused position and raw unfinished fields. The switch itself sends no audio.
An existing server document remains separate and is never implicitly merged or
replaced. While Server On
awaits an available server workspace, the retained local pane stays paused and
locked; turning Server Off restores it. Server projects above the local 128 MiB
decoded-source budget remain on the server, with an explicit budget message
before any download. Failure never silently switches engines.

`arrangement-remote-import.js` uses chunked owned uploads. A ZIP is uploaded once;
the server supplies opaque review/part/version IDs and performs selected source
extraction. The phone does not read or decode the whole archive. The frontend
adds no fixed part-count policy; bounded metadata replies and advertised server
upload, decoded-memory, metadata and storage budgets still apply. A successful selection is one atomic
history edit, preserving existing tracks and different source durations.

`arrangement-remote-jobs.js` polls exact owned job IDs with epoch/revision guards
for scoped suggestions, delivery export and canonical Project Save/Open. Cancel
targets the acknowledged job. `arrangement-timeline-project.js` accepts either
local bytes or a server-owned artifact; remote results use the existing
authenticated streaming download bridge. Save/Open remains separate from WAV or
MP3 delivery, and neither action accepts unapplied fields or proposals.

`arrangement-stream-contract.js` and `arrangement-stream-preview.js` map the
48 kHz compressed preview clock to exact project frames after explicit encoder
priming. Applied server edits can take a few seconds to reach buffered audio;
the pane states that delay. Qualification keeps strict PCM ratios and a bounded
four-second settling deadline. Backend encoder pacing is a different bound.

Short compressed previews expose `preview_padding_frames`, an integer from 0
to 5,760 in 48 kHz preview frames. A nonempty source suffix shorter than 120 ms
is padded with transport-only zeros after successful real EOF; empty and longer
previews report zero. This applies to MP3 and MP4, with the existing priming
reported separately. Container playback duration includes padding, while the
project clock stays clamped to the real source end. Arrangement metadata,
immutable audio, processing and delivery exports never receive this padding.
Cancellation or a failed source read must not flush it as successful audio.

The configurable public guest defaults are 16 heavy jobs per session, 32 per
IP address per hour and 128 globally per hour. ZIP review and commit use the
separate admission-command budget (120 per session), while Open, suggestions
and delivery exports consume heavy jobs. Native concurrency, CPU, memory and
storage limits remain enforced. Coded quota failures preserve the open document
and identify the allowance rather than claiming temporary server busy. Ordinary
guest acceptance uses one real guest session for ZIP, suggestions, edits,
MP3/WAV delivery, project saving and a further tweak. The operator stress route
is additional evidence; it does not substitute for the guest workflow.

Additional integration files are the `hybrid/arrangement-remote-*`
modules/tests, `hybrid/arrangement-stream-*`, and the focused changes in
`hybrid/server-client.js`, `hybrid/main-controls.js`, `hybrid/bounded-json.js` and
`hybrid/remote-media-policy.js`. Browser tooling is
`verify_arrangement_remote.py`, `arrangement_remote_{audit.js,files.py,archive.py,diagnostics.py}`,
`test_arrangement_remote_diagnostics.py` and `remote_browser_fixture.py`.
Canonical continuation also uses `arrangement-workspace-transfer.js` and its
tests. The ordinary guest probe is `verify_arrangement_guest_ui.py`, with
`arrangement_remote_migration.py` and `arrangement_remote_budget.py` helpers.
Fixtures freeze both native binaries into their private directory before launch
and record their paths and hashes; they do not keep mutable Cargo images open.

Run the actual isolated HTTP/UI workflow with matching native binaries:

```sh
python web/tools/verify_arrangement_remote.py --site <matching-build> --cli <cli> --worker <worker> --ffmpeg <ffmpeg> --browser chromium --owner --files --archive --out <scratch>/remote
python web/tools/verify_arrangement_remote.py --site <matching-build> --cli <cli> --worker <worker> --unavailable --out <scratch>/unavailable
python web/tools/verify_arrangement_remote.py --site <matching-build> --cli <cli> --worker <worker> --guest-limit --out <scratch>/guest-quota
python web/tools/verify_arrangement_guest_ui.py --site <matching-build> --cli <cli> --worker <worker> --ffmpeg <ffmpeg> --browser chromium --out <scratch>/guest
```

Use `--archive-mib 303` for the synthetic large-archive boundary. The fixture
can check real server sources above the local budget with `--large-budget` on
the guest probe. The operator stress fixture credential exchange supplies only
its private operator token; application
controls, audio processing and download authentication remain real. All sources
are synthetic. Physical mobile playback, real-device memory and deployed remote
availability require separate acceptance. The production server arrangement
feature stays opt-in and off by default.

### Matching remote qualification, November 16-17, 2026

Private browser qualification passed for frontend/WASM runtime
`8486186ea3f9155f` paired with backend SHA-256
`3bbce76b0ba301b166e5b37af3ebc8f6533428ca32e196e6acef0ef2f73da79a`.
The site was built directly from the frozen working source, including rejected
Play cleanup and the padding-duration contract. No post-build gate patch was
used. Production rollout and deployed public acceptance remain separate;
production arrangement support is an explicit server opt-in. Physical mobile
playback is not established by these desktop-engine checks.

Server arrangements use `audio/native-arrangement-player.js`: plain native
HTMLAudio, with no MediaElementAudioSourceNode or Web Audio filter graph.
Chromium and Firefox use progressive MP3; WebKit's MP4 MIME capability selects
the bounded MP3-in-MP4 MSE reader. The facade maps encoder priming to project
frames and preserves scoped edits, A/B and listening volume. Native pause at
EOF does not retire the stream before ended, and a rejected start releases its
owned descriptor and media reader. Local editing and legacy server playback
retain their existing players.

All UI cases below ran against the same isolated owned HTTP backend at 320 and
390 logical pixels. There were 144 passing grouped checks and zero application
errors across 24 UI contexts. Short-source contexts include an additional
normal-control seek to the final 20 ms of the imported 1.25-second clip.

| Check | Cases and grouped checks | Result |
| --- | --- | --- |
| Ordinary guest continuation, ZIP, unapplied Suggest, scoped EQ, live gain, A/B, volume, Stop/seek, MP3/WAV, Project Save and another tweak/export | Chromium 143, Firefox 144, WebKit 26; six contexts, 72 groups | Passed with actual native sink PCM and independent delivery decode |
| Normal UI short clips and paused near-EOF seeks | Same engines and widths; 0.12 and 1.25 seconds, then a 20 ms suffix; 12 contexts, 18 playbacks, 54 groups | Passed; all head/tail markers audible, normal marker spacing within two frames, Play and exact 1.25-second endpoint restored |
| Matching local MP3 regression | Same engines and widths; six contexts, 18 groups | Passed; independent 320 kbps decode, applied gain, unchanged other track and separate canonical Project Save |
| Native decoder 20 ms and meaningful one-frame suffixes | Same three engines; six cases | Passed; true source endpoint held through padding, gain 0.69996755-0.70002752, silent padding and post-EOF tails |
| Focused native-player, stream-facade and rejected-start regressions | 18 Node tests | Passed |

The normal UI cases assert zero padding for 0.12-second and 1.25-second starts.
Seeking to source frame 59,040 of a clip ending at 60,000 yields 4,800 preview
padding frames and plays the actual source marker at 59,744. The one-frame
decoder case uses start 59,744 and end 59,745, with 5,759 padding frames.
Independent decoders locate those markers at frames 1,809 and 1,105 respectively,
including the unchanged 1,105-frame priming. All six decoder captures measured
exactly zero in their distant final 20 ms of encoded padding and post-EOF sink
windows. Source clocks remained clamped to 60,000 or 59,745 while transport
padding drained. No PCM or timing tolerance was widened.

`native_media_capture.py` captures the owned Pulse sink outside the browser.
The remote audit observes media and API ownership without connecting audio
nodes. Passing server cases require zero Web Audio media connections, zero
client PCM decodes and no full preview-media reads. The helper refuses a
desktop/WSLg sink, scopes the private daemon's default and alternate rates to
48 kHz before startup, and restores the environment afterward. Headless Chromium
removes only Playwright's default `--mute-audio` argument. Recordings, bounded
failed windows and process-reaping evidence are retained; all owned capture
processes were reaped after the passing matrix.

The earlier muted Chromium capture and a separate 44.1-to-48 kHz resampling gain
failure are preserved. Neither fixture correction changed ALSA, product audio
or numeric thresholds. Failed harness attempts that selected a Stop reply as a
prepare descriptor, or sought while Playback details was collapsed, also remain
separate from the passing normal-control reruns. The unpadded WebKit 20 ms case
had valid independently decoded PCM but an empty MSE range and zero sink PCM;
the approved transport-only padding closed that reproduced failure.

The large-archive and Server Off budget proof is reused from runtime
`c3e701a510ecef7f`: Chromium passed both widths with a 322,103,609-byte
synthetic ZIP uploaded once per session and 155,136,000 decoded source bytes
checked before any Server Off download. Admission and download guards did not
change with this player fix; the large fixture is manual CI work.
The older foundation verifier was also migrated to native capture and passed
eight groups in Chromium at both widths before the padding change. The current
ordinary guest matrix covers the same transport and scoped-EQ controls with
the final backend.

The unchanged public native source fingerprint is
`669ed07dd12261e01f3f1c3d77972f6e337057057a77cd0eb221216467bca1e6`.
Every HTTP fixture records frozen native paths and hashes; the Linux worker
hash is
`a210a6d97d3fe0f1bd6d89a01571ce62b2befcdd7597e3e7114b112180c6e520`.
Frontend, backend and verification-tool receipts accompany the private
qualification reports. No production service was restarted for these checks.

The low-level probe is `verify_arrangement_stream_browser.py`, with
`arrangement_stream_browser_probe.js`. It checks actual PCM, independently
decoded encoded controls, source clocks, bounded read-ahead and cancellation.
The normal UI counterpart is `verify_arrangement_short_ui.py`.
Linux native capture needs the existing PulseAudio utilities `parec` and
`pactl`. Use a fresh output directory for every attempt; optionally pin a frozen
backend with `--server-source <source-root>`:

```sh
python web/tools/verify_arrangement_short_ui.py --site <matching-build> --cli <cli> --worker <worker> --ffmpeg <ffmpeg> --browser webkit --out <scratch>/short-webkit
python web/tools/verify_arrangement_stream_browser.py --site <matching-build> --cli <cli> --worker <worker> --ffmpeg <ffmpeg> --browser chromium --preview-container mp3 --out <scratch>/native-chromium
python web/tools/verify_arrangement_stream_browser.py --site <matching-build> --cli <cli> --worker <worker> --ffmpeg <ffmpeg> --browser firefox --preview-container mp3 --out <scratch>/native-firefox
python web/tools/verify_arrangement_stream_browser.py --site <matching-build> --cli <cli> --worker <worker> --ffmpeg <ffmpeg> --browser webkit --preview-container mp4 --out <scratch>/native-webkit
```

An independent negative cleanup control attributed a historical terminal 404
to Stop issued 107 ms after the fixture began deleting its own playing project.
Current cleanup stops playback and waits for owned metadata requests to settle
before deleting the project. That expected failure is preserved separately.
Current guest cases have clean canonical continuation with applied source-range
EQ, stable IDs/history, paused position and raw unfinished fields, without such
application errors.
