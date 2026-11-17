# Shared arrangement transport

The existing aligned stem bank remains separate. The core `arrangement` module
owns metadata, DSP and rendering; `arrangement_session` owns metadata admission,
immutable source ownership, revisions, history labels and source queries.

`ArrangementSession::edit` applies an `Edit` batch atomically through shared
history. It returns metadata and never renders a song. `add_assets` retains
immutable `Arc<Audio>` sources, allows differing durations and mono-to-stereo
use, and rejects rate mismatches until an explicit conform step. Invalid or
stale commands preserve the prior workspace. Snapshots expose revision/epoch,
the arrangement, ID-based asset metadata and actual Undo/Redo labels.

Waveforms use immutable source frame bounds, including both channels.
`eq_response` returns actual shared-filter `{frequencies,gain_db}` values.
Suggestions are explicit worker operations and measure the selected scope
through appropriate routed effects, excluding only a named effect being replaced. Mix/lane ranges use project frames; clip ranges use
immutable source frames. Ends are exclusive, and ranged EQ requires 20 ms.

## Adapter API

The browser facade is `createArrangementAdapter({mode,remoteProject,onChange,
beforePlay,onError})`. Its interface is `read`, `reviewImport`, `discardImport`,
`addImport`, `edit(Edit|Edit[])`, `undo`, `redo`, `waveform`, `suggestEq`,
`eqResponse`, `previewSource`, `exportAudio`, `saveProject` and `openProject`.
Import selection uses owned review/part/version IDs. Names are display labels.

`previewSource({expectedRevision,signal})` must return only an actually prepared
current revision. It supplies a real transport facade, not aligned-bank PCM
metadata. Source identities, epoch and layout guard late results. Metadata
changes use `onChange`; per-clock playback events use transport subscriptions.
Core players run on a normal worker feeding bounded audio blocks; construction,
seek preparation and disposal stay off the audio callback. The audio context
rate must match the project rate before playback begins. Playback is not an
advertised capability until the actual worker/audio path is qualified.

## Canonical projects

Use `format: slimeybeats-studio`, `version: 2` for native project directories,
restart recovery and web ZIPs. One manifest describes current/original/before
documents, actual retained history and labels, render settings and stable asset
IDs. Immutable Float32 WAV pool entries have content hashes and exact geometry.
Source IDs are distinct from stored pool filenames and human labels. Loaders
verify hashes, layout, metadata budgets and every history reference before
publishing a replacement workspace. The v1 Studio reader remains available via
version dispatch; its aligned-source requirements are preserved.

Native adapter API: `studio_native::arrangement_project::{save,load}`
over the shared snapshot, with a common v1/v2 project dispatcher. WASM uses the
same typed manifest and PCM/WAV verification for ZIPs rather than a separate
frontend format. MCP uses owner-scoped project/asset IDs; browser-supplied or
MCP-supplied filesystem paths cannot select sources or overwrite output files.

`ArrangementSession::{project_snapshot,from_project,restore_project}` bridges
the same manifest to owned editing sessions. Browser `saveProject` returns a
canonical ZIP `{bytes,name,type}`; `openProject(File|Uint8Array)` validates the
entire manifest, checksummed Float32 pool, geometry and history before replacing
the workspace. It changes epoch and increments revision, clears old playback,
and does not autoplay. Failed imports preserve the existing workspace.

## Browser implementation and qualification

`web/dsp/src/arrangement/` owns the WASM bindings. Browser modules separate
`arrangement-adapter.js`, worker/client RPC, import/review and canonical projects
from `arrangement-preview.js` and `audio/arrangement-sink.js`. Metadata responses
contain no PCM. Explicit preview preparation installs requested geometry
immediately, including an empty Before side while paused. The persistent facade
keeps B identity/duration stable and exposes `side_duration_frames` separately.
A is **Before last edit**, without loudness matching. Playing A/B preserves
running state; paused selection remains paused. Stop invalidates late resumes.

Fast faders and fixed-scope existing EQ update the running shared player without
seeking or rendering a song. Structural changes use bounded worker preparation,
with at most 1.5 seconds of filter warmup, an approximation of older IIR history.
Explicit offline exports replay the core renderer. The sink holds at most three
2048-frame blocks and only copies PCM into audio outputs. Context/project rates
must match. Unsupported AudioWorklet environments do not advertise playback.

Loose WAV/MP3 and safe stored/deflated WAV/MP3 ZIPs support one or more independent
parts, repeated display labels and different durations. Source rates must match
without implicit conforming. Browser source PCM totals 128 MiB; retained ZIP
reviews total 128 MiB. Project ZIP input/output is capped at 256 MiB. The legacy
aligned bank keeps its original requirements.

On November 16, 2026, `web/tools/verify_arrangement_audio.py` passed actual
generated WASM/AudioWorklet PCM checks in Windows Chromium 143 and Firefox 144:
live -6 dB gain without restart, A/B, exact Stop silence, frame seeks, paused
empty-A and changed-duration geometry, immutable source bytes, Undo/Redo,
single-source ZIP/path rejection, Float32 export, canonical project roundtrip,
corrupt-project preservation and fresh-worker recovery with restored Undo.
Planck's separate actual full UI checks passed 320/390 widths in Chromium,
Firefox and Linux WebKit, including scoped EQ, source-frame ranges, positive
HF proposals, ZIP selection, Save/Open and explicit export. See
[browser qualification](WEB_ARRANGEMENT_UI.md). These are browser/PCM checks,
not physical-device listening or hard realtime deadline acceptance. Windows'
bundled WebKit lacks AudioWorklet; that environment remains unsupported.

## MCP and remote status

`mcp/server/src/arrangement_{arguments,commands,state,operations,protocol,help}.rs`
exposes typed metadata commands, ID-based source admission, scoped suggestions,
waveforms/response, explicit preview render and existing native export formats.
`get_help(topic=arrangement)` specifies SOURCE versus PROJECT units. Metadata
edits preserve the legacy bank and do not render; all audio jobs use the existing
job/cancellation machinery. Three focused adapter tests passed for atomic/stale
and busy admission, source retention, actual preview/export PCM and clearing.

Server arrangement transport and remote live preview are **pending**. The
current facade rejects unavailable server arrangement mode explicitly. Existing
server/mastering and aligned-bank APIs remain unchanged. No service restart or
production rollout is part of this checkpoint.
