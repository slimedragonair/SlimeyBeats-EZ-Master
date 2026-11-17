# Browser session diagnostic trace

Help → support report includes **Preview session JSON trace**, **Download reviewed
JSONL**, and **Clear session trace**. Download exports exactly the reviewed snapshot,
not events that arrived after preview. No trace is sent automatically.

Every line is one JSON object:

```json
{"session":"random-diagnostic-uuid","timestamp":"2026-11-17T10:00:00.000Z","message":"uploaded file","extra":{"schema_version":1,"sequence":7,"file_hash":"d41d8cd98f00b204e9800998ecf8427e","hash_algorithm":"md5","hash_status":"complete","size":0,"format":"unknown"},"level":"info"}
```

`session` is a random diagnostic ID for this page load, not a server credential or
project ID. Trace lives only in memory, up to 1,000 events. Refresh/close clears it.
The first export line describes retention and the number of evicted events.
There is no telemetry service, analytics vendor, network sink or persisted log.

Coverage: browser clicks/changes use allowlisted control IDs (no typed values or
link URLs); arrangement adapter calls, Studio local/remote processing calls, server
HTTP outcomes, and Studio/server messages. Start/end events share a process
sequence. The recent action sequence is best-effort context within 30 seconds,
not proof of causality for concurrent/background operations. HTTP 200 means the
request succeeded, not that a queued processing job has completed. Operation
completion is separately recorded by the adapter/Studio call.

Selected file events are distinct from **uploaded file** events: the latter is
recorded only after acknowledged complete upload. Hashing uses incremental MD5 in
256 KiB slices, yielding between slices, at most two concurrent hashes. More than
1 GiB is skipped; busy/read-failure/cancel states are explicit with null hash.
Hashes are content identifiers, not security checksums; they can identify matching
content and should be reviewed before sharing. No audio bytes are retained by the
trace. Size is bytes. WAV format/bit-depth come from header parsing, not filename:
for example `wav 32f` versus `wav 32pcm`. Unknown or unsupported encodings stay
unknown. MD5 doesn't validate that a file is valid audio.

Only known, static app-authored message text is retained. Dynamic/unrecognized
text is omitted, with a redacted message event; selected known failure patterns
retain a safe description and HTTP status. No raw exceptions, server response
bodies, tokens, endpoint URLs, filenames, user/project labels or input values are
logged. Maintain `session-trace-catalog.js` when adding static controls/messages.
This deliberately favors privacy over verbatim capture of every dynamic message.

Tests: `node --test web/site/session-trace.test.mjs web/site/hybrid/*.test.mjs`.
This is browser instrumentation, not native desktop or server-side audit logging.
