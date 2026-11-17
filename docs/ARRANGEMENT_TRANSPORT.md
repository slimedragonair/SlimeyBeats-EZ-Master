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
Explicit offline exports replay the core renderer. The sink uses rate-aware scheduling headroom of approximately 320 ms, rounded
up to whole 2048-frame blocks (minimum three, hard maximum 32). At 48 kHz this
is eight blocks, 341 ms, and 128 KiB of stereo PCM; at all rates the queue is
capped at 512 KiB of stereo PCM. This bounds memory while tolerating ordinary
worker/main-thread scheduling stalls. Live DSP edits can take up to that queued
audio duration to become audible; it is a listening-preview latency tradeoff,
not a change to saved PCM. Stop/seek discard queued audio immediately. The
worklet only copies PCM and reports underrun counters without skipping source
frames or hiding missing output. Qualification waits a fixed documented queue
drain interval before measuring settled gain; deterministic tests separately
assert the live-edit latency bound and a 250 ms delayed-producer regression. Context/project rates
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

Server arrangement transport is opt-in (`--enable-arrangement`), disabled by
default. Its persistent native actor edits metadata and feeds a continuous MP3
stream from shared-core frame blocks. Preview output is 48 kHz; project-frame
mapping subtracts 1,105 encoded priming frames. The encoder runs at most 150 ms
ahead of GET wall time. That bound does not include browser media buffering.
Explicit raw-PCM probe limits prevent FFmpeg retaining seconds of input before
its first packet; no frame-dropping `nobuffer` mode is used.

Heavy operations use `POST /v2/projects/{id}/arrangement/jobs`, then GET the
returned job ID or POST its `/cancel` endpoint. Every request supplies epoch and
expected revision. Scoped suggestions remain unapplied; export selects one of
the six existing native formats. Save returns a canonical v2 project ZIP; Open
accepts a completed owned `arrangement_project` upload and adopts only a fully
validated snapshot. Heavy work runs in a disposable native shadow. Failed or
cancelled jobs preserve the primary workspace, and successful Open restores
paused with a new epoch. Downloads retain separate project-owned artifact IDs.

Linux persistent actors follow the backend process lifetime through a pidfd
guard (parent-process check fallback), retaining the supervisor's limits and
descendant reaping. They survive the creating HTTP/job thread's exit, including
after a shadow actor is promoted. Delayed empty-project preparation and actual
parent-process death with a detached helper have separate regressions.

Server ZIP import uploads the original once with role `arrangement_archive`.
The `review_archive` job takes `upload_id` and returns owned `review_id`, opaque
part/version IDs and a WAV-preferred default per part. The `commit_import` job
takes that review ID and `selection: [{part_id, version_id}]`. Shared Rust
`stem_archive::inspect_files` and `stem_import_plan::plan_audio` perform review;
only selected members are extracted to generated contained filenames. Native
`AddFiles` admits them in one metadata/history transaction. One source and
different durations are allowed without changing aligned-bank rules. No ZIP
decode or full-song render runs on the phone. Successful import consumes the
review/upload while retaining immutable selected files. Failure or cancellation
preserves the current arrangement and review. Direct `discard_import` accepts
the owned review ID, is idempotent, and releases only an unconsumed upload.

Archive admission is capped at the smaller of the operator upload limit and
1 GiB; each entry is capped at 512 MiB. Pending reviews share an 8 MiB metadata
budget and a four-review limit. There is no separate instrument/part-count cap;
choose one encoding per included part. Selection is bounded by its 8 MiB metadata
budget, source PCM, project bytes and the existing 4,096-entry storage quota
(files and directories, including retained sources, artifacts and temporary job
files). Admission reserves the actual entries required for selected extraction
and rechecks before writing. Arrangement sources do not occupy legacy
`Project.assets` slots. Review names are labels; public requests never supply
extraction paths.

Public guests can finish an editing workflow in one session. ZIP review and
selected import use the existing independent command/admission budget: 120 per
session, 480 per IP/hour and 1,920 globally/hour. Heavy arrangement jobs default
to 16 starts per session, 32 per IP/hour and 128 globally/hour; operators can
configure `--guest-jobs-per-session`, `--guest-jobs-per-ip-hour` and
`--guest-jobs-per-hour`. Suggestions, WAV/MP3 exports, project Save and project
Open (including explicit local-to-server migration) count as heavy starts.
Capability limits advertise both budgets. CPU, concurrency, disk, source and
time limits remain enforced. HTTP quota errors retain specific safe codes such
as `guest_job_limit`, with `Retry-After`; record expiry never resets rate quotas.

Job history retains at most 32 records. Admission expires the oldest completed,
failed or cancelled records only after cleanup has finished. Active, unfinished
and unreaped jobs remain protected. The most recent 128 expired IDs return
`410 arrangement_job_expired` to their owner; unknown or older discarded IDs
return 404. Foreign owners receive 404. Expiring a record does not delete its
artifact or promoted source cache. The separate artifact-count and storage
limits still apply.

On November 16, 2026, actual private native/HTTP checks passed scoped suggestions,
six exports with independent decoding, canonical project hashes, atomic Open
with restored Undo, corrupt-Open preservation, cancellation/reaping and foreign
ownership. Focused backend tests cover 164 successive cheap job admissions and
active/unreaped retention.
The first-byte regression reproduced 3.0 seconds of startup buffering before the
probe fix and measured 0.141 seconds afterward; the same continuous MP3 stream
retained the measured 1.995 gain ratio. Five-rate impulse tests retained priming
within one preview frame. A synthetic 317,739,756-byte ZIP passed actual upload,
paired review, explicit WAV/MP3 selection, atomic unequal-duration import,
selected-member checksums, Undo/Redo, cancellation, unsafe-path/changed-CRC
rejection and contained cleanup. An owner-authorized real 303,565,857-byte Suno
ZIP also passed server review of 11 parts/22 versions and one selected WAV
import, actual frame playback and Undo/Redo; both supplied originals retained
their hashes. That probe does not qualify decoding all 11 parts. Browser remote
acceptance is still pending. Actual Windows and Linux HTTP/native probes also
completed ZIP admission, scoped suggestion/edit, WAV/MP3 delivery, canonical
Save/Open, another tweak/suggestion/export and Main v2 mastering using one
ordinary guest token and no owner workflow requests. They used seven heavy-job
starts and five admission/command starts, including project Open and the three
Main import/master/export commands. The actual large Suno ZIP was separately
admitted as a guest with both original hashes retained. These checks do not
qualify physical-device latency
or listening.

Native probes copy both CLI and worker to private fixture directories and record
their hashes before launching. They never hold Windows' mutable build images
open during other agents' builds. Existing server/mastering and aligned-bank
APIs remain unchanged. At the original November 16 checkpoint, no production rollout had occurred; the November 17 deployment below supersedes that status.


## Native server-preview output and short suffixes

Server arrangements use `audio/native-arrangement-player.js` for native HTML
media output. Their mix, gains and scoped EQ are already rendered by the server;
no MediaElementAudioSourceNode or second client DSP graph is added. The local
AudioWorklet editor and legacy streaming-mastering player remain separate.

Descriptors distinguish project-rate source frames from the 48 kHz compressed
preview clock. `preview_priming_frames` describes encoder priming;
`preview_padding_frames` describes a silent transport tail in preview-rate
frames. An absent padding field is read as zero for older descriptors; malformed
or more than 120 ms of padding is rejected. Seek still starts at the requested
source frame, and the cursor clamps to the real source end after subtracting
priming. No padding extends clips, projects, analysis or exported audio.

Only positive suffixes shorter than 3/25 second receive enough zero input frames
to reach that decoder minimum. The feeder appends them after every real frame
has been consumed; it does not request extra frames from Rust. Cancellation,
failed/partial input, expired projects and empty sources do not synthesize a
tail. Normal previews receive no extra silence. The existing MP3 encoder and
MP3/MP4 containers are retained.

The browser fills the compressed reader's bounded startup buffer before native
output, preserves natural EOF until the media ended event, and retires readers/streams on rejected
starts or interruption. Generation and stream-ID guards keep an obsolete
continuation from pausing newer playback. Seeking to the exact end stays paused;
the next Play starts at zero. Listening volume uses the Studio 0–1 range and does
not enter an export. Device-specific media/volume behavior and physical-phone
acceptance remain separate from synthetic native-output qualification.


## November 17 production deployment

The qualified backend was installed from source commit `ab948624` after a fresh
root metadata check found no retained projects, audio sessions, active jobs or
native children. The guarded update appended only `--enable-arrangement` to the
service command. Authentication, models, codecs, storage, resource settings and
the existing Funnel ingress were preserved. Rollback files remain available.
The deployed Python fingerprint is
`3bbce76b0ba301b166e5b37af3ebc8f6533428ca32e196e6acef0ef2f73da79a`;
the compiled native inputs were unchanged and their qualified binaries reused.

A fresh ordinary public guest passed health/capability discovery, a 20 ms suffix
with padding4800 and decoded marker frame1809, and the existing WAV import,
analysis, mastering and export flow. The original hash and stereo48kHz/60000-frame
export geometry were preserved; both owned test projects were deleted. This API
acceptance used four processing submissions. See the browser-specific record in
[Web arrangement UI](WEB_ARRANGEMENT_UI.md) for native-output and HTTPS UI scope.
