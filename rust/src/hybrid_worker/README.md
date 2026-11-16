# Native server DSP worker, private API 2

Build without GUI: `cargo build --locked --no-default-features --bin slimeybeats_hybrid_worker`.
Run `slimeybeats_hybrid_worker --request ABS_JSON --response ABS_JSON --threads 4`.
Both CLI paths must be absolute. Python writes a new private request for each
command, supervises the process tree, and publishes only a successful response.
Cancellation is process-tree termination; no persistent worker or audio device.

```json
{
  "api_version": 2,
  "project_root": "/srv/private/project-123",
  "operation": "master",
  "input": "/srv/private/project-123/original.wav",
  "out_dir": "/srv/private/project-123/jobs/job-456",
  "settings": {"profile":"neutral","mode":"streaming_safe","ceiling_dbtp":-1},
  "limits": {
    "max_pcm_bytes":2147483648,
    "max_bank_pcm_bytes":4294967296,
    "max_duration_seconds":3600,
    "decode_timeout_seconds":3600,
    "waveform_bins":1024
  }
}
```

`api_version`, `project_root`, `operation`, `out_dir`, and `settings` are required.
Unknown top-level and settings fields fail. Settings is a flat, partial native
`config::Settings` object: missing values use its defaults. `reference` paths
are forbidden; supply a validated native `Analysis` as `reference_analysis`.
Settings output paths are replaced by the validated command directory.
Only bundled profiles and blends are supported. No custom profile file reads.
`reference_analysis` uses the same server geometry/PCM limits as input.

Operations and additional fields:

| Operation | Input | Additional fields | Result |
|---|---|---|---|
| `import` | `input` | none | exact float original, analysis, waveform |
| `analyze` | `input` | none | analysis, waveform, input dimensions |
| `suggest` | `input` | optional profile in `settings` | shared `suggestion:{bands:[7 EqBand]}` and analysis; source unchanged |
| `master` | immutable `input` | optional `reference_analysis` | pre-user-EQ float master, shared Report, waveform |
| `render` | pre-EQ master `input` | native `settings.manual_eq`, `eq_output_db`, `eq_hold_loudness`, `formats`, `output_sr` | EQ float preview and shared final exports |
| `issues` | `input` | none | native Doctor IssueReport |
| `repair` | `input` | `repair` below | float original revision, IssueReport, analysis, waveform |
| `voice_cleanup` | `input` | `voice` below | float original revision, analysis, waveform, tool metadata |
| `spectral_repair` | `input` | `spectral` below | float original revision, analysis, waveform |
| `spectrogram` | `input` | optional `spectrogram:{columns:320,rows:128}` | numeric shared Spectrogram under 1 MiB |
| `region_edit` | `input` | `region` below | float original revision, analysis, waveform |
| `separate` | `input` | `runtime`, optional `separation_mode` | two/four stems, raw-sum original, guarded full mix preview |
| `mix` | `stems` | optional `mix` | float original revision, analysis, waveform |
| `import_stems` | `input` ZIP **or** `inputs` **or** explicit `stems` | none | aligned stems, raw-sum original, guarded full mix preview |

`bundle` runs in Python from approved assets, outside this DSP protocol.
Render formats: `wav24`, `wav16`, `wav32f`, `flac16`, `flac24`, `mp3`.
Render never repeats mastering. Master explicitly disables every manual EQ band,
including cut filters, and zeroes EQ output gain. It never replaces the original.
Edits return new files; supplied source files are never overwritten.
Stem import/separation uses shared `stem_mix::render` for both root assets:
`original` is the neutral full-bank sum with `protect_peaks:false`, while
`preview` uses the bank's default guarded mix. `audio` and top-level analysis /
waveform describe this full mix preview, never the first stem. All individual
stems retain role `stem`. The backend retains an existing project original
when separation/import produces an additional raw-sum original.

```json
{
  "voice": {"highpass_hz":75,"noise_reduction":0.3,"noise_region":null,"deess":0.3,"gate_threshold_db":null,"output_gain_db":0,"hold_loudness":true,"protect_peaks":true,"ceiling_dbtp":-1},
  "repair": {"action":"reduce_low_mids","start_seconds":1,"end_seconds":2,"strength":0.5},
  "spectral": {"start_seconds":1,"end_seconds":2,"low_hz":250,"high_hz":900,"reduction_db":-6,"mode":"attenuate"},
  "region": {"start_seconds":1,"end_seconds":2,"operation":{"kind":"gain","db":-3}},
  "stems": [{"path":"/srv/private/project-123/vocals.wav","kind":"vocals","name":"Lead"},{"path":"/srv/private/project-123/drums.wav","kind":"drums"}],
  "mix": {"tracks":[{"kind":"vocals","gain_db":-3,"balance":0,"mute":false,"solo":false},{"kind":"drums","gain_db":0,"balance":0,"mute":false,"solo":false}],"output_gain_db":0,"protect_peaks":true,"ceiling_dbtp":-1},
  "runtime": {"python":"/srv/tools/runtime/env/bin/python","models":"/srv/tools/runtime/models/4stems"},
  "separation_mode":"vocal_instrumental"
}
```

Only send fields applicable to that operation. Repair action also accepts
`declip`; spectral mode also accepts `heal`. `noise_region` is either null or
`{start_seconds,end_seconds}`. Region operation kinds are `gain` (requires `db`),
`silence`, `fade_in`, `fade_out`, `reverse` (no other fields). Spectrogram accepts
16 to 320 columns and 16 to 128 rows; levels_dbfs is a flat column-major array,
with frequencies_hz, columns, rows and duration_seconds. Voice/mix defaults are
their shared native defaults.
Stem kinds are native named kinds or canonical `part_N`. Stem inputs must share
frames, sample rate and channels; no hidden alignment/resampling. Import pairing
uses the shared Suno planner, prefers WAV, and retains all distinct parts.
Loose `inputs` accepts either absolute path strings or objects
`{path:ABS,kind?:StemKind,name?:originalFilenameWithExtension}`. Original names
drive shared pairing/role suggestions when uploads have opaque storage names.
Explicit kinds keep separately selected parts distinct. Names are metadata,
never paths to extract or execute. `stems` is already explicit and does not pair.
Runtime executable/model paths are trusted private server configuration and may
live outside project_root; they are never accepted from browser payloads.

Successful response (fields omitted when inapplicable):

```json
{
  "api_version":2,"operation":"master",
  "audio":{"path":"/srv/private/project-123/jobs/job-456/master.wav","frames":48000,"sample_rate":48000,"channels":2,"duration_seconds":1},
  "assets":[{"role":"master","name":"master","path":"/srv/private/project-123/jobs/job-456/master.wav","format":"wav32f","frames":48000,"sample_rate":48000,"channels":2,"duration_seconds":1,"channel_peaks":[0.3,0.25]}],
  "waveform":{"peaks":[[-0.2,0.3]],"duration_seconds":1,"channel_peaks":[0.3,0.25]},
  "resource_limits":{"max_pcm_bytes":2147483648,"max_duration_seconds":3600,"separation_max_pcm_bytes":536870912}
}
```

`analysis`, `report` and `issues` serialize the shared native types. Optional
`tool_metadata` contains measured tool notes, EQ status or separation provenance.
Roles are fixed to `original`, `master`, `preview`, `stem`. A master operation's
float `master` is immutable pre-EQ audio. A render operation's float `preview`
is editable server PCM; its `master` assets are final encodes. Python makes compact
streaming previews separately and maps every private path to an opaque asset ID.
Waveform bins summarize all channels, include right-only energy, and contain
`[minimum,maximum]`, not per-channel arrays. There are min(frames,bins) bins,
default/max 1024.
Waveform metadata and each asset include `channel_peaks`, one finite absolute
linear PCM peak per channel, preserving values over unity. Stem assets carry
these peaks even when there is no single current audio/waveform in the response.

The response file is atomically published without replacing an existing file.
Errors exit 2, use stderr, and leave no success response. Stdout contains only
JSON progress lines `{ "progress":0.5,"message":"Processing audio" }` with
bounded numeric values and fixed text, never paths or upstream tool messages.
Progress is phase-level and does not promise a time estimate.

Audio, request and response paths must resolve under project_root; response must
be a new file inside out_dir. All paths
are absolute. Output ancestors must not be symlinks/reparse points, and generated
files go only under out_dir, including ZIP extraction and codec temporaries.
Use a server-private project tree: canonical checks are not a sandbox against
another process concurrently replacing directories. The supervisor removes
uncommitted command directories after failures/kills; partial files are private.

Default bounds are 2 GiB decoded float PCM per input and 3600 seconds, independent
of browser admission limits. The combined stem bank defaults to 4 GiB. These are
admission bounds, **not** process RSS limits: mastering, meters, EQ, exports and
stem mixing allocate additional full-length buffers. Set an external memory/CPU
quota and serialize native/model commands. Neural separation currently retains
the existing **512 MiB decoded PCM cap** and CPU model execution; it does not
promise 2 GiB neural jobs. ZIP inspection also retains shared native bounds:
1 GiB archive, 512 MiB per encoded entry, and 8 MiB planner metadata. Loose stems
use the server PCM budget. The request JSON itself is capped at 16 MiB.

Files: request/settings/reference validate serialized state; paths enforces
containment; input bounds decoding; output publishes PCM and builds envelopes;
processing dispatches shared DSP; stems adapts bank import/mix/separation.
`verify_worker.py` exercises a built binary through a small loopback HTTP parent.
See [VALIDATION.md](VALIDATION.md) for executed checks, artifact paths, hashes,
and their qualification limits.
