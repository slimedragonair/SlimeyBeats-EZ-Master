# Server processing in the normal editor

`studio.js` wires a single top-bar switch and contextual information. It checks
availability and acquires a scoped guest credential, but never intercepts the
main file picker, drag/drop or Master button. `main-controls.js` exposes only
engine selection, explicit mastering and cancellation. It never overrides editor
settings, transport, export labels or the editor's busy locks.

`app.js` always imports/decodes the original into the same browser Session and
player. Analyze stays local. Its ordinary Master action snapshots settings and
chooses either local DSP or `runServerMaster`. The latter uploads the original
as a segmented float WAV, using `browser-track.wav` instead of a source filename.
The normal profile/blend, mode, target, ceiling, intensity and safety settings
are sent. An optional reference is sent as validated measurements only.

`pcm-transfer.js` streams a float WAV result directly into one bounded PCM array,
checking MIME, RIFF/chunk lengths, float representation, finite samples and exact
sample rate/channel/frame identity. The editor's 15-minute/384-MiB decoded budget
still applies; it does not use the legacy generic artifact-download budget.
`Session.adopt_master` installs this pre-manual-EQ audio atomically after local
analysis, retaining only bounded numeric server metadata. A/B playback, live EQ,
suggestions, WAV/MP3 and ZIP export then use the same paths as a local master.
Re-mastering always starts from the unchanged original.

After receiving the result, the client requests deletion. Failure keeps the
completed master plus its job/credential for an explicit cleanup retry in the
info panel; it never claims deletion without acknowledgement. On processing
failure, the original, settings and prior preview are preserved. Cancel aborts
requests; a retained job must be deleted before a subsequent upload. The switch
is locked while the editor is busy. Turning it off after completion keeps the
received master editable. No failure silently chooses another engine.

`processing-controller.js` obtains access only on explicit opt-in or Retry.
`server-availability.js` uses bounded unauthenticated status checks every 30 seconds
while enabled and visible. Health recovery never creates, uploads, runs or
replays an audio job. The ordinary Master action may reconnect before starting.
A stale 401/404 job can be forgotten locally by Retry before new guest access;
forgetting does not confirm remote deletion. Interrupted files otherwise expire
under the service's advertised retention.

Only the explicit switch plus normal Master action authorize transfer. The
protocol's `consent:true` records that action; there is no additional checkbox.
Credentials and job references remain in tab memory. Nothing is saved in URLs,
preferences or local recovery. Recovery stores exact received PCM and bounded
numeric processing metadata, restoring locally without network calls or a fresh
master. The received master does not require the original encoded File to be
reselected; a later explicit server Master can serialize the recovered original.

`PUBLIC_API_ORIGIN` is the exact deployment pin shared by health/access and the
visible info panel. CSP also permits only exact configured origins, not a
provider wildcard. Public requests have a five-second deadline and 16-KiB JSON
budget. Authenticated metadata uses 64 KiB; control/chunk requests have 120-second
deadlines and the bounded float result has 15 minutes for slower connections.
All artifact requests carry Authorization headers, never bearer URLs.

Node tests cover transfer bounds, settings snapshots, cancellation, cleanup
acknowledgement, availability and session isolation. DSP tests cover atomic PCM
adoption, reference preservation, EQ export and recovery. The browser verifier
runs the ordinary editor against an actual owned CLI/service; its exact-origin
API adapter is a transport fixture, not evidence of public deployment or phone
listening. The shared editor still needs its local WASM/AudioWorklet engine even
when heavy mastering is remote. Studio and MCP remain local.
