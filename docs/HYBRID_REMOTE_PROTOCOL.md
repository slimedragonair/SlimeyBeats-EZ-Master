# Remote Studio protocol v2

This protocol is additive. The v1 session, status and job API remain available.
All v2 routes require the existing owner or guest `Authorization: Bearer ...`
header. Mint guests through `POST /v1/session` as before. Guests see only their
own projects; the owner can manage all projects. Tokens stay in tab memory. A
same-origin browser service worker forwards authenticated fetches, including
Range, for media elements. Tokens in URLs and cookie authentication are unsupported.

## Browser contract

`GET /v2/capabilities` returns `api_version:2`, `available`, `operations`,
`formats`, `input_formats`, `profiles`, `features`, `limits`, and `privacy`.
`available` requires the operator-configured Studio binary and FFmpeg. Stem
separation additionally requires fixed operator Python/model paths. No request
can choose executable, model, filesystem, URL, or subprocess arguments.

`POST /v2/projects` accepts `{}` and returns HTTP 201 with this manifest.
`GET /v2/projects/<id>` returns the same shape. IDs are opaque 32-character hex.

```json
{
  "api_version": 2,
  "id": "OPAQUE_PROJECT_ID",
  "revision": 0,
  "state": "ready",
  "expires_at": "2026-11-16T15:00:00Z",
  "original_id": null,
  "master_id": null,
  "preview_id": null,
  "reference_id": null,
  "input_id": null,
  "before_id": null,
  "preview_is_mix": false,
  "edit_origin_kind": null,
  "stems": [],
  "controls": {"tracks":[],"output_gain_db":0,"protect_peaks":true,"ceiling_dbtp":-1},
  "assets": [],
  "artifacts": [],
  "uploads": [],
  "analysis": null,
  "report": null,
  "issues": null,
  "spectrogram": null,
  "command": null,
  "can_undo": false,
  "can_redo": false
}
```

An audio asset is `{id,role,name,frames,sample_rate,channels,duration_seconds,bytes,
content_type,has_preview,has_waveform}` with optional fixed stem `kind` and safe
numeric `analysis:{metrics:{...}}`. Full 240-bin Analysis stays private per
asset for references/native calls; explicit analyze and project.report.output
still expose full safe Analysis. `stems` contains the same projection for the current bank;
`controls` uses native StemMixSettings. Roles
are `original`, `reference`, `master`, `render`, `stem`, or `edited`. Names are
generated generic names; stem display labels are bounded and never paths. Original and pre-EQ master
assets are immutable. Remaster reads the retained original; render reads the
pre-EQ master when one exists. Export artifacts are separate from editable audio.
An artifact is `{id,name,bytes,content_type}`. No native path or raw log is public.

`POST /v2/projects/<id>/uploads` accepts `{filename,bytes,role,label?,kind?}`. Roles are
`original`, `reference`, `stem`, `stem_bundle`. Extensions are WAV, MP3, FLAC,
M4A, AAC, AIFF/AIF, OGG; stem_bundle requires ZIP. A stem label is 1–128
characters without controls, path separators or traversal. Optional stem kind
is a fixed native kind or canonical part_N (1..4294967295). These fields are
stem-only and feed the shared native import planner; labels never select local
filenames. Returns HTTP 201 with
`{id,total_bytes,uploaded_bytes,role}`. Append with
`PUT /v2/projects/<id>/uploads/<uploadid>?offset=N`, raw bytes and Content-Length.
Each request is at most 1 MiB, must start at exactly uploaded_bytes and must not
overrun total_bytes. Success returns HTTP 200 with the upload object. Re-read
the manifest to recover the offset after an interrupted request. Partial writes
are rolled back. Transfer-Encoding, duplicate offsets and extra queries fail.

`POST /v2/projects/<id>/commands` accepts `operation`, integer `base_revision`,
and optional `upload_ids`, `audio_id`, `settings`, `voice`, `repair`, `spectral`,
`stems`, `mix`, `separation_mode`, `format`, `redo`, `source_project_id`, `region`,
`spectrogram`, `kind`, `include_stems`, `as_preview`, `reference_id`. Returns HTTP 202 with the manifest.
Only one command per project can be queued/running. Poll GET until command state
is `completed`, `failed` or `cancelled`. Active command shape is
`{id,operation,state,base_revision,progress,message,error_code}`. Project state is `ready`,
`queued`, `running`, or `deleting`. Failed/cancelled commands leave revision and
the previous committed manifest unchanged. Successful commands atomically
publish a complete new revision, including analysis-only commands. A stale
base_revision returns 409; no implicit retries or last-write-wins.
Audio undo history is separate from command revisions: analyze, issues,
spectrogram, suggest, export and bundle neither add an undo state nor clear redo.
The manifest's before_id, preview_is_mix and edit_origin_kind are authoritative
A/B state, included in every audio checkpoint and restored by undo/redo. Direct
voice/Doctor/spectral previews compare with the selected immutable target;
edit_origin_kind identifies a selected bank stem. Complete bank mixes compare
with original_id, set preview_is_mix:true and clear edit_origin_kind.
Successful exports/bundles append `command.artifact_id`; suggestion adds
`command.result:{bands:[seven EqBand objects]}`. Message text is fixed server
text. Manifest JSON is at most 1 MiB. Retired audio is retained while referenced
by current state or the two undo/redo states; IDs no longer listed can expire.

| Operation | Input and result |
| --- | --- |
| import | One complete original upload; atomically switches original_id/input_id/preview_id to a new immutable asset, clearing the former master and bank. as_preview:true instead switches only preview_id and its analysis, preserving original/master/bank. |
| reference | One complete reference upload; analyzes and retains reference for mastering. |
| analyze / issues | Selected audio_id or active audio; updates numeric analysis / issue report. |
| master | Retained original/current mastering input by default, or explicit audio_id; safe settings and retained reference analysis; immutable pre-EQ master becomes active. |
| render | Pre-EQ master or selected/active audio; manual EQ render becomes active; final encoded artifact is separate. |
| repair / voice_cleanup / spectral_repair | Selected/active audio with the corresponding bounded option object; creates edited audio. |
| separate | Selected/active audio, separation_mode vocal_instrumental or four_stems; retains original input and installs only role=stem assets in the bank plus a guarded full-sum Preview. |
| import_stems | Complete stem uploads or one stem_bundle ZIP; replaces the bank and atomically switches original_id/input_id/before_id to the new full raw sum with protect_peaks:false. Clears the previous master/report/issues/spectrogram and publishes a distinct guarded full mix as Preview. Undo restores the preceding source and bank. Separation instead keeps its existing original. A/B and export never select one arbitrary stem. |
| mix | stems array of `{audio_id,kind}` plus mix controls; creates edited mix. |
| bundle | audio_id/settings/format plus optional kind mastering/studio and include_stems bool; renders final format and packages safe manifest/settings/analysis/report/EQ. No stems required for mastering. include_stems:true includes the chosen/raw bank. A stems-only bundle may omit audio_id. |
| export | Selected/active audio and format; creates encoded artifact without replacing editable audio. |
| clone | source_project_id and audio_id from a same-principal source project; atomically replaces destination original/input/preview pointers without browser reupload. Existing state survives failure. |
| history | redo false/omitted for undo, true for redo; retains two previous committed Studio states. Revision remains monotonic. |
| replace | audio_id selects a bank stem; queues an immutable replacement using current Preview, then renders the complete bank with current controls. Bank and mixed Preview publish together only on success. |
| promote | audio_id selects current mastering input and preview; original_id remains immutable. |
| spectrogram | Selected/active audio with optional spectrogram:{columns,rows}; defaults 320 by 128, at most 65536 cells and within the whole-manifest JSON bound. |
| region_edit | Selected/active audio and region:{start_seconds,end_seconds,operation:{kind}}; kind silence/fade_in/fade_out/reverse, or gain with db -60..12. A selected bank stem is immutably edited, replaced in the bank and remixed using all current controls; the resulting Preview is the full bank, not the isolated edited stem. |
| suggest | Selected/active raw master/original plus safe settings; returns command.result.bands from the shared EQ suggestion engine. Native reliability checks require at least one second of audible audio. |

Master's optional reference_id selects an opaque asset belonging to this
project and requires full validated private Analysis. Explicit null disables
reference matching for that command; omission uses the legacy project.reference_id.
An explicit prior reference overrides a newer project default without changing
that default. Reference selections remain retained within project resource and
expiry bounds, independently of audio undo depth. The browser sends its known
choice with every Master so an older retained reference or a lost reference-upload
response cannot silently change the requested tonal match. No public Analysis or
reference path is accepted in this field.

`POST /v2/projects/<id>/cancel` accepts `{}` and waits for owned work to stop,
then returns the manifest. Previous committed audio stays available. DELETE
`/v2/projects/<id>` needs no body and returns `{"deleted":true}` only after the
child, uploads and downloads have stopped and all owned storage is removed.
503 means cleanup needs retry. Projects and guest sessions are ephemeral and
are never resumed after restart.

## Authenticated media

`GET /v2/projects/<id>/assets/<audioid>/audio` streams full audio.
`.../preview` streams an MP3 preview with source metadata stripped at 192 kbps by default;
operators can select 96–320 kbps. Full audio and exports keep source fidelity.
Generated codec frame-count/delay metadata is retained for gapless timeline alignment.
`.../waveform` returns `{duration_seconds,peaks:[[min,max],...],channel_peaks:[...]}`
with at most 1024 pairs normalized to -1..1, plus absolute linear per-channel
peaks for live faders. `GET /v2/projects/<id>/artifacts/<artifactid>`
streams the export. Downloads require Authorization on every request and never
redirect. Audio, preview and artifacts support a single byte Range including
open ends and suffixes. Responses send Accept-Ranges: bytes, correct 206
Content-Range/Content-Length, or 416 with Content-Range: bytes */SIZE. Multipart
ranges are rejected. CORS allows Range and exposes Accept-Ranges, Content-Range,
Content-Length, Content-Disposition and Retry-After, without credentials.
Streaming uses bounded blocks and stops on deletion or expiry. Cancelling a
command keeps prior committed media available.

## Safe settings and private worker contract

settings is a flat safe projection of native Settings: built-in profile/blends,
mode, target_lufs, ceiling_dbtp, intensity, output_sr, match_strength,
match_loudness, match_width, dither, safety, max_makeup_db, max_limiter_gr_db,
disabled, manual_eq (exactly seven `{type,freq,gain,q,on}` bands), eq_output_db,
eq_hold_loudness, and allowlisted numeric/boolean modules controls. Unknown
settings, module fields and any paths are rejected. Output format is chosen
with command format: wav24, wav16, wav32f, flac24, flac16 or mp3.

voice supports highpass_hz, noise_reduction, noise_region (start_seconds,
end_seconds), deess, gate_threshold_db, output_gain_db, hold_loudness,
protect_peaks, ceiling_dbtp. repair requires action declip/reduce_low_mids,
start_seconds,end_seconds,strength. spectral requires start_seconds,end_seconds,
low_hz,high_hz,reduction_db,mode attenuate/heal. mix supports tracks of
`{kind,gain_db,balance,mute,solo}`, output_gain_db, protect_peaks, ceiling_dbtp.
Numbers must be finite and bounded; native validation also checks actual geometry.

The server invokes only Config.studio_cli with
`--request PRIVATE_JSON --response PRIVATE_JSON --threads N`. N is the operator's
Config.v2_threads, default 4 and bounded to 1–32 (`--v2-threads`). The same value
sets the v2 child's Rayon/OpenMP environment; v1 invocation/environment stay at 4.
Request has
api_version:2, operation, absolute input (when needed), out_dir, project_root,
settings, optional reference_analysis/voice/repair/spectral/stems/mix/inputs,
separation_mode, operator runtime:{python,models}, and generated limits.
Native response has api_version:2, operation, audio:{path,frames,sample_rate,channels,
duration_seconds}, assets:[{role,name,path,format,kind?,...}], analysis, report,
issues, waveform, optional suggestion/spectrogram. Loose stem inputs are objects
`{path:ABS,name:LABEL_WITH_EXTENSION,kind?:NATIVE_KIND}`. All audio/output paths must resolve inside command staging
or already committed private input. Only validated regular, unlinked files
under staging can be newly published. Responses are size bounded and reports
are projected to numeric/boolean data and fixed enums before publication.
Report input/output and top-level analysis are complete sanitized Analysis
objects. Detector issues retain only mud/harshness/sibilance/fizz/clipping/noise/
stereo keys, finite severity 0–100, and fixed server descriptions; native free
text is discarded. Suggested profiles are retained only for built-in IDs,
otherwise neutral. The existing shared Live EQ/native deserializer can consume
these objects unchanged. The report includes a safe profile
label and empty notes/log/exports collections for the report UI; arbitrary native
notes, stage text, paths and process logs remain private.
Backend maps reference to native import, export to native render, and builds
bundle ZIPs itself. Native errors become fixed public error codes.

## Limits, scheduling and modules

v2 defaults: 3600 seconds, 2 GiB upload and decoded f32 audio, 16 GiB project
disk, 1 MiB chunks, 1024 waveform bins, 512 retained uploads, 512 audio assets and
32 artifacts. Command batches support up to 128 stems/uploads; this is bounded
API admission rather than the former 16-stem UI restriction. Combined decoded
bank audio remains bounded to 4 GiB and storage walks to 4096 entries, independent
of bank part count. Native ZIP admission additionally caps
encoded ZIP size at 1 GiB and each encoded entry at 512 MiB. Neural separation
keeps its 512 MiB decoded PCM cap; capability limits advertise it separately.
v2 subprocess budgets default to 16 GiB memory/address space, 14400 CPU seconds,
3600 seconds wall time and 300 seconds decode time. These are separate from v1's
unchanged defaults; Linux memory is per process and needs an aggregate operator
cgroup/service limit. A 2 GiB input admission limit is not a promise every DSP
operation fits those budgets. Limits are separate from v1 and validated operator overrides are
available. Retention is at most one hour and clamped to guest expiry. Projects
and v1 jobs share the configured max_jobs retained slots and disk reservations,
and a single FIFO heavy worker. There is no second native worker. Guests may
retain two v2 projects (main plus Studio); v1 keeps its original one-retained-job
admission policy. v2 allows 120 accepted editing commands per
guest per hour, with bounded IP/global windows; v1 keeps its three-start budget.

The implementation separates project models/store, upload/command validation,
native driver, audio decoding/preview, safe response projection, media leases,
and HTTP routing. Command output lives in a private staging directory until
every output validates. Cancellation/failure removes staging and keeps prior
committed revisions. Resource limits apply to the entire project, including
temporary decoder and encoder files. Original uploads are decoded with forced
demuxers and a protocol whitelist; M4A external data references are disabled.
The bounded float WAV pipe sink preserves source sample rate and channels;
overflow fails instead of truncating or silently resampling/downconverting.
ZIP imports are validated before extraction and never extract client paths.

Operator flags are `--studio-cli`, `--stem-python`, `--stem-models`, and
`--v2-max-upload-bytes`, `--v2-max-decoded-bytes`, `--v2-max-audio-seconds`,
`--v2-max-project-disk-bytes`, `--v2-retention-seconds`, `--v2-preview-kbps`,
`--v2-threads`, `--v2-memory-bytes`, `--v2-cpu-seconds`, `--v2-worker-timeout-seconds`,
`--v2-decode-timeout-seconds`. The venv Python path is invoked as configured,
even when its binary is a symlink; resolving it before exec bypasses the venv.
Omitting studio-cli leaves v1 available and v2 capabilities.available false.
Native command response JSON must be inside its out_dir. Private request files
are exclusive 0600 files, removed after the owned tree is reaped. Linux uses a
subreaper and owned-session/descendant termination to cover nested process groups
and detached codec/model helpers; Windows retains its kill-on-close Job Object.

Files: project_models/store/commands own state and transactions; project_requests
and project_settings own public schemas; project_native_request and studio_driver
own private adaptation; pcm_decoder/project_audio own decoding and previews;
project_outputs/reports own publication; project_media owns leases and ranges;
project_retention keeps current/history assets; project_http owns routing.
project_bank_edits composes region/replace with a final shared native mix under
one queued command and cancellation event. Intermediate files stay private;
failure/cancellation of either pass retains every prior committed pointer.
project_references resolves explicit, null and default per-master choices from
project-owned immutable assets and validates their full private measurements.
worker.py dispatches both API versions on the same thread and FIFO queue.

Run full regression/native checks with
`python -m hybrid.tests --cli ABS_V1_CLI --ffmpeg ABS_FFMPEG --studio-cli ABS_STUDIO_WORKER`.
The native project tests require real executables; unit fixtures never claim DSP
qualification. Unix-only resource/tree tests skip on Windows.

HTTP errors return a fixed `error` code; worker command failures set error_code
(`processing_failed`, `cancelled`, or `cleanup_failed`). Reports intentionally
omit arbitrary native free text, paths, filenames, config, and logs. Physical
listening, live browser streaming, deployed ingress and real stem-model
inference require separate qualification from backend tests.

## Backend qualification, November 16, 2026

The original 72-test native backend baseline passed before implementation (one
Unix-only skip on Windows; the first run's Windows socket abort did not repeat).
The expanded 111-test suite passed on Linux/WSL without skips and on Windows
with four platform-only skips. Real HTTP/native tests cover import/reference,
master/render, all six export formats, loose stem labels/kinds, full-bank raw
sum/guarded Preview, mix/replace/history, clone, as_preview migration, suggestions,
voice/Doctor/spectral/region edits, spectrogram, both bundle variants, compressed
source geometry, overflow rejection, failed-import rollback and Range downloads.
Linux tests also prove nested process-group/detached-session cleanup, venv
interpreter identity, resource limits and stalled-reader cleanup. Generated MP3
previews retain codec delay/frame-count information; a seekable-file decoder
check confirms the exact resampled fixture duration. Browser codec behavior is
still separately qualified by the browser integration owner.
Additional real PCM checks prove region edits and Replace publish complete
bank mixes using current controls, including the 0.1 + 0.2 stem fixture with a
half-gain edit yielding the full 0.25 mix. Voice/Doctor/spectral A/B metadata,
undo after scan/export, all six read-only commands preserving redo, second-pass
cancellation and native mix rejection are covered. A real 32-stem import checks
all identities and summed PCM; schema tests admit 128-item batches and keep a
512-asset metrics projection below 1 MiB. Native reference tests verify explicit
null omits matching, an older reference asset overrides a retained newer default,
and omission keeps the legacy choice. The disk monitor's removed-temporary-directory
race has a deterministic regression; missing roots and unsafe entries still fail.
The HTTP report contract regression passes both exact returned input/output
Analysis objects into the real native shared deserializer without consumer-side
repairs. Redaction preserves required typed fields and approved detector warnings
while replacing native Issue text; safe report profile/collection defaults also
satisfy the report UI. A clipped PCM fixture retains its clipping warning, and
foreign detector text/profile canaries never appear in the projected response.
Thread-control regressions cover the 1–32 bound/default, --v2-threads parsing,
native invocation arguments and real supervised child environments: v2 can use
8 while v1 remains 4. Deployment and its eight-CPU resource allocation remain
operator integration steps.

Deploy the updated bank-parity worker alongside this backend. The tested native
release SHA-256 values are Linux
`37fd55fc09201f0d4397ecd784c03bf1fbf65c2e05921c89f2f674247ec2c8ff`
and Windows
`bbc5362dc623599b758b21bd3e7f8f379cc154fa60a27a05f796b00e08eed4b5`.
Older API 2 binaries that emit stems without complete original/Preview mixes
fail publication instead of exposing one stem as the whole mix. Actual model
runtime, deployed ingress/browser media, listening and matched speed benchmarks
remain independent from these local backend assertions. No speed multiplier is
inferred from synthetic correctness fixtures.
