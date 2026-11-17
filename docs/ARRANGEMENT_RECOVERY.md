# Native arrangement crash recovery

The private recovery record now includes an optional versioned arrangement.
Legacy records without that field still load. The `tracks` workspace is accepted
by restore and returns to the multitrack page.

`gui/recovery_document.rs` stores the canonical committed `HistorySnapshot`,
its total byte budget, source IDs and names, `RenderOptions`, immutable initial
import origin/reference, tonal profile and paused playhead. Source and comparison
audio use the existing global attachment indices. Multiple source IDs and other
Studio views sharing one `Arc<Audio>` retain one attachment and regain the same
identity when restored. No original audio filename is opened to restore clips.

`gui/studio/recovery_arrangement.rs` captures `history.current()` and retained
undo/redo rather than a live drag, incomplete label or EQ proposal. Its autosave
fingerprint ignores gesture epochs and playback time; committed history revisions,
workspace page, source identities and canonical rendering controls invalidate it.
Arrangement-only work participates in the ordinary autosave/close lifecycle.

The recovery worker validates all source indices, layouts, PCM, history states,
origin, options and comparison geometry before adoption. It prepares waveform
caches and a frame-zero sample engine off the UI thread, with cancellation.
Adoption releases previous players and plugin hosts, replaces the timeline, and
loads the existing native player paused. Seeking to the saved position uses the
native background preparation path. Opening another Studio page preserves the
inactive timeline without substituting it for that page's audio. A record without
a prepared comparison restores metadata without inventing comparison audio.
Recovery never exports audio, runs a plugin or resumes playback.

Storage retains its existing byte/count limits: 8 MiB checkpoint metadata,
512 MiB per audio attachment, 4,096 attachments and the configured store budget.
These recovery admission limits can be lower than a valid editing project's
16 MiB document/32 MiB retained-history budget. Oversized saves report failure
and preserve prior recovery points; history and PCM are never silently truncated.

Focused tests in `gui/studio/recovery_arrangement_tests.rs` cover an actual disk
round trip, shared attachment identity, exact float samples, retained undo/redo,
committed-only capture, invalid relationships and cancellation, empty/inactive
timelines, legacy records, and paused restoration without exports or plugin work.
These tests use synthetic audio with native output disabled; they do not establish
physical-device playback quality or callback deadlines.
