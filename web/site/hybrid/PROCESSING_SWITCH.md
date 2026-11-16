# Server processing in the normal editor

The top switch selects the processing engine. It checks availability and obtains
a scoped, temporary guest credential without transferring audio. The normal
import, Analyze, Master, Studio and Export controls perform the requested work.
There is no second upload form or consent checkbox and no silent engine fallback.

## Responsibilities and data flow

- `studio.js`, `processing-controller.js` and `server-availability.js` own the
  switch, guest access and bounded health checks. `main-controls.js` exposes
  selection, the authenticated connection and availability failures.
- `remote-project.js` owns one v2 project per workspace, encoded-file chunk
  upload, command revision/polling, cancellation, metadata and confirmed cleanup.
  Only the original encoded file is uploaded; it need not fit a browser PCM heap.
- `remote-mastering.js` and `remote-studio-client.js` translate the established
  editor actions to server commands. Shared native DSP performs analysis,
  mastering, final EQ/export, issues, repair, voice/spectral tools, mixing, region
  edits and optional two/four-stem separation.
- `studio-migration.js` can move an existing local Studio origin plus its edit
  recipe. It publishes only after successful replay; pending live faders survive.
  Failure preserves the local workspace. Full local PCM is released after adoption.
- `remote-media-worker.js` and `remote-media-bridge.js` authenticate native media
  Range requests using a MessageChannel to the initiating editing tab. Credentials
  remain in memory and never enter URLs, audio caches, preferences or recovery.
  A short-lived artifact-to-tab route handles native downloads with no clientId.
  An attachment navigation streams downloads without an entire JavaScript Blob;
  Chromium's `a[download]` path can bypass the Service Worker.
- `StreamingPlayer` and `RemoteStudioPlayback` stream 192 kbps MP3 previews.
  AudioWorklet EQ and stem AudioParams process current playback blocks locally.
  Final downloads are rendered from full-resolution server audio. The player
  routers retain the existing fully local path when that engine is selected.

Server projects have immutable audio assets, atomic revisions and two audio
undo/redo checkpoints. Read-only scans, suggestions and exports do not consume
audio history. Authoritative `before_id`, `preview_is_mix` and `edit_origin_kind`
prevent solo-stem previews from being mistaken for a complete bank mix.

## Boundaries

Default source admission is one hour / 2 GiB encoded and decoded, with a 16 GiB
project disk budget. These are admission ceilings, not promises that every
maximum-size workload fits available memory. Neural separation separately
accepts at most 512 MiB decoded PCM. The server has one shared heavy worker;
queue, tool limits and the operator's CPU/memory quotas still apply. Preview
stream count and live rendering also depend on the browser/device.

Switching off prevents new remote edits but retains access to existing streams
and downloads. Import a smaller local file for completely local processing.
Clear/Close requests deletion of that workspace's server project and requires
acknowledgement; expiry is at most one hour, also bounded by guest expiry.

Local recovery intentionally does not store streamed server audio or access
tokens. Download a session ZIP before leaving; it contains full-resolution audio
and numeric/settings metadata, not a persistent remote login or complete undo
history. Browser/OS download behavior and physical phone listening need their
own qualification.

## Compatibility and verification

The original v1 Master-only bridge (`master-runner.js`, `pcm-transfer.js`) remains
available for protocol compatibility and its focused tests. The current editor
uses v2. V1 metadata remains bounded at 64 KiB; v2 manifests use an explicit
1 MiB ceiling. Public health/access JSON remains bounded at 16 KiB.

See [the protocol](../../../docs/HYBRID_REMOTE_PROTOCOL.md),
[deployment and privacy](../../../docs/HYBRID_STUDIO.md), and
[measured performance](../../../hybrid/deploy/remote-tools/BENCHMARK.md).
Native/API tests, isolated three-engine transport/UI checks, actual hosted
service checks and physical-device listening establish different evidence.
The separate MCP companion remains a local stdio service and is never exposed
as this public processing API.
