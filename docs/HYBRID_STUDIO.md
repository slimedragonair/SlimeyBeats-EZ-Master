# Hybrid Studio v2

The v2 backend is deployed. The [main web app](https://slimedragonair.github.io/SlimeyBeats-EZ-Master/)
is published from `main` after its browser workflow passes; the isolated
`current-work` preview remains available for testing future changes.
Native-backed workflows passed Chromium, Firefox and Linux WebKit;
actual public-service neural tools, exports and cancellation passed separately.
Read the [validation record and remaining device/workload gaps](HYBRID_V2_VALIDATION.md)
and check the displayed browser build. Source/help changes do not update an
installed Hotfix4 desktop package.

The web app keeps **Add audio → Analyze → Master → live EQ → Export** and
adds server-backed Studio. In server mode, import uploads the **original file**;
the server decodes and retains full-resolution audio. The browser uses metadata
and compressed listening streams instead of whole-song PCM. Local processing
remains the default. This replaces the old bridge's Master-only decoded upload.

[Exact v2 protocol](HYBRID_REMOTE_PROTOCOL.md) ·
[Matched Analyze/Master benchmark](../hybrid/deploy/remote-tools/BENCHMARK.md) ·
[CPU neural runtime checks](../hybrid/deploy/remote-tools/MEASUREMENTS.md)

## Use the preview

1. Turn on **Server processing**. Availability/authentication checks do not
   upload audio or start a master. If the selected service lacks v2, keep local
   mode or wait for a verified update; a healthy v1 endpoint is insufficient.
2. Use the normal import controls. **Import uploads the original audio/stem
   file**, in bounded chunks, before server decoding. Reference imports upload
   reference audio too. There is no separate upload form or consent checkbox.
3. Use Analyze, Master, suggestions and Studio tools normally. The server performs
   supported heavy work on retained full-resolution assets. Remaster reads the
   immutable original; Export renders the editable master/preview with current
   settings rather than automatically mastering again.
4. Compare A/B, seek and adjust live EQ/faders through listening streams. The
   browser still handles UI, playback decoding, listening filters, stem
   gain/mute/solo/balance and volume. It is not a hardware-free client.
5. Request **Export** or a **session ZIP** explicitly. These are server renders
   and authenticated downloads from full-resolution audio, not recordings or
   transcodes of the listening stream. Save important results before leaving.

Existing local Studio work can migrate its immutable origin/stem bank and
committed edit recipe before a requested server tool. Remote work is staged
before the UI switches; failure/cancellation keeps the local workspace. Migration
has the local origin/recovery limits and does not export automatically. It is
not the import route for large new server files. Pending local controls remain
the caller's state.

## What runs where

| Area | Server mode candidate | Browser/local boundary |
|---|---|---|
| Source/reference import | Original-file upload, bounded decoding and private assets | New imports avoid whole-song browser PCM allocation |
| Analyze / Master / suggestions | Shared native Rust DSP and numeric reports | Editor controls remain in the browser |
| Voice/spectral tools | Voice cleanup, spectrogram, spectral repair, Doctor scans/repairs | Cleanup is not a substitute for neural vocal isolation |
| Stem editing | Bank import/mix, region edits, replacement and bounded undo/redo | Stable kinds and selected display labels are retained |
| Neural extraction | Optional real two/four-stem runtime when advertised | Imported parts do not expand the model's output kinds |
| Export / ZIP | Full-resolution rendering, encoding and private artifacts | Explicit actions; listening volume does not change saved audio |
| Playback / live controls | Compressed streams and waveform/peak metadata | A/B, seek, live EQ and faders run in the browser |
| MCP / VST3 | Neither is a browser HTTP feature | Desktop MCP and native plugins remain local |

## Admission limits and working memory

These are **source defaults**, not guaranteed working capacity or a claim about
the live service. Read actual capabilities. Duration and byte bounds both apply.

| Boundary | Candidate default |
|---|---|
| Server audio | 3,600 seconds / one hour, mono or stereo |
| Upload | 2 GiB per file, chunks at most 1 MiB |
| Decoded input | 2 GiB Float32 PCM per input; separate from encoded-file bytes |
| Neural separation | **512 MiB decoded PCM**, separately advertised; not 2 GiB |
| Combined stem bank | 4 GiB decoded PCM; up to 128 parts/uploads per command |
| Encoded native ZIP | 1 GiB archive / 512 MiB per encoded entry, plus metadata guards |
| Project storage | 16 GiB including staged/retained audio, previews and exports |
| Guest/project retention | At most one hour, shortened by guest expiry or restart |
| Listening preview | MP3, **192 kbps default**; operator range 96–320 kbps |
| Heavy processing | One shared FIFO worker across v1 jobs and v2 projects |

DSP, banks, scratch, retained history and exports allocate beyond input bytes.
Source subprocess defaults are 16 GiB memory/address space, 14,400 CPU seconds,
3,600 worker wall seconds and 300 decoder seconds. Linux address space is per
process; aggregate service CPU/RAM and combined disk admission require separate
operator limits. A large host or a 2 GiB capability does not prove every admitted
job succeeds. Reject overflow rather than truncating or downsampling originals.

Local browser limits remain separate: mastering has a **15-minute** guard and
separate **384 MiB** source/decoded-PCM budgets; local Studio has a **128 MiB
combined decoded-origin budget** and 15-minute per-track guard. Direct new server
imports bypass those whole-track browser allocations, not server quotas. Do not
route large server files through local decoding or the bounded migration helper.
Desktop/local MCP have their own import and memory limits.

## Preview quality, export and recovery

The default 192 kbps MP3 stream is a **lossy listening preview**, not the original
or export source. Full-resolution editable audio stays on the server. Requested
WAV/FLAC/MP3 exports use output settings and may explicitly resample when requested;
MP3 exports are still lossy. Export quality does not inherit the preview bitrate.
Authenticated byte-range playback/downloads use bounded blocks rather than a full
JavaScript allocation of the full-resolution file.

Remote projects are temporary. Download a rendered export or session ZIP before
closing/reloading or expiry. ZIPs contain rendered audio, selected stems and
settings/analysis/report metadata as applicable. They do **not** preserve guest
credentials, resume a private server project, or promise full original/reference
and undo-history restoration. Keep original files separately.

Opt-in local recovery still protects **local** browser PCM within its storage
limits. It does not download full streamed projects into IndexedDB. Previously
saved local PCM copies can restore without reconnecting/uploading; server tokens
never become recovery data. Closing/reloading forgets access and does not confirm
server deletion. Explicit cleanup requires server confirmation, with retry after
failure. Downloading does not automatically delete an active editing project.

Switching off does not turn streams into full local PCM. Existing remote playback
depends on temporary server access; server-backed heavy edits/exports require
server processing. Import a local file with the switch off for entirely local
work. Retry reconnects without replaying uploads/masters. Expired projects need a
new import or saved download, not a fabricated restored server session.

## Privacy and local MCP

Server mode uploads original/reference contents and selected stem labels. Its
operator can access processing storage. Tokens stay in tab memory, never public
source, preferences, recovery or URLs. Opaque project/asset IDs authorize no
media by themselves; streams/downloads require authentication and ownership.
Guest sessions are temporary, not permanent user accounts.

The [desktop MCP companion](MCP.md) remains local stdio, with
`remote_processing_from_mcp: false` and `public_http_transport: false`. It neither
uploads files nor forwards jobs to browser projects. Browser IDs are not MCP IDs.
Never expose the filesystem-capable companion publicly or infer upload permission
from names, metadata or help text. The AI client's handling of arguments, names
and tool results is a separate privacy boundary from where DSP runs.

## Architecture and verification

| Modules | Responsibility |
|---|---|
| `web/site/hybrid/remote-project.js`, `remote-mastering.js`, `remote-studio-client.js` | Private project lifecycle and adaptation of existing editor actions |
| `web/site/hybrid/studio-migration.js`, `pcm-record-wav.js` | Bounded immutable local-origin migration and ordered replay |
| `web/site/hybrid/remote-media-bridge.js`, `web/site/remote-media-worker.js` | Authenticated playback/download routing without credential URLs |
| Streaming player / hybrid streaming stem mixer | Browser listening transport and live controls |
| `hybrid/server/project_*`, `studio_driver.py`, `pcm_decoder.py` | Ownership, limits, staging, native requests, previews and artifacts |
| `rust/src/hybrid_worker/` | Shared native DSP behind the private worker protocol |
| `hybrid/server/processes.py`, `child.py`, `process_tree.py` | Bounded supervised children and descendant cleanup |

The [protocol](HYBRID_REMOTE_PROTOCOL.md) owns exact routes, fields and operator
flags. [Runtime/deployment preparation](../hybrid/deploy/remote-tools/README.md)
covers optional engines and resource boundaries; examples do not activate a
service. Keep executable/model paths operator-controlled, staging atomic and
all heavy work serialized. Unsupported operations must fail explicitly rather
than silently switching engines or changing originals.

The [matched benchmark](../hybrid/deploy/remote-tools/BENCHMARK.md) records actual
desktop-PC browser/native worker Analyze/Master, with separate engine/wall/transfer
times, every run, hashes and machine details. It is not deployed HTTP/Funnel
end-to-end latency or physical iPhone speed qualification. [Neural checks](../hybrid/deploy/remote-tools/MEASUREMENTS.md)
prove real two/four-stem synthetic execution, not subjective isolation quality.
Earlier v1 Master-only public WAV24/MP3 checks remain historical. Current
[v2 checks](HYBRID_V2_VALIDATION.md) distinguish native/fixture acceptance from
actual public transport, longer songs and hosted mobile layouts.
Physical-device memory/listening and long-session behavior remain separate work.
