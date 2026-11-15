# Native recovery store

Integration API (native-only module registration belongs to the parent):

```rust,ignore
let listing = recovery_store::list_candidates(&private_absolute_root, &cancel)?;
// On an explicitly selected, unlocked point from an interrupted prior run:
let selected = &listing.checkpoints[0].checkpoint;
let private_metadata = recovery_store::inspect(&private_absolute_root, selected, &cancel)?;
let snapshot = recovery_store::load(&private_absolute_root, selected, &cancel)?;
let mut store = recovery_store::Store::open(&private_absolute_root, budget_bytes, &cancel)?;
let saved = store.save(&snapshot, &cancel)?;
// Only after an explicit dismissal request, and after retaining the restored work:
recovery_store::discard_run(&private_absolute_root, selected.run_id(), &cancel)?;
store.finish(&cancel)?; // explicit successful shutdown; Drop does not mark clean
```

All cancellation callbacks are `&dyn Fn() -> bool`. Calls perform disk I/O and belong
on a worker. `Store` is Send; keep one instance in the recovery worker for its window.
Its lifetime holds an OS run lock. `list_candidates` returns `Listing { checkpoints,
warnings }`; `list_history` also includes clean exits. Entries are newest first and
contain a `CheckpointRef`, capture time in Unix milliseconds, `interrupted`, attachment
count, unique encoded audio bytes, decoded bytes counting every index, and optional
`maintenance_warning` (a committed checkpoint whose later retention cleanup failed).
A reference serializes for worker messages;
every operation revalidates its components. Active runs are excluded from lists,
load and deletion. `Store::root()` and `Store::run_id()` expose identifiers for UI.
`inspect(root, &CheckpointRef, cancel) -> Result<serde_json::Value>` uses the same
owner checks, checkpoint checksum and active-run locking as `load`, but returns
only the private payload without opening blob metadata or PCM. Inspection does not
establish that the audio is intact; Restore performs that verification separately.
`cleanup_unused(root, cancel) -> Result<()>` retries owned tombstone and unreferenced
blob cleanup even after the selected run has already been removed. A missing root
is a no-op and creates nothing. It takes the global transaction lock, validates the
owned namespace, preserves active/history references, and reports malformed or
unknown material instead of claiming completion. Clear should call it on Retry
even when there is no remaining Store or fallback run ID. Show successful cleanup
only after this call succeeds; cancellation may follow an already-committed run
deletion while some unreferenced private audio still awaits cleanup.
`discard_run` is idempotent for an already-absent safe run ID in a validated owned
root. That does not claim garbage collection is complete: call `cleanup_unused`
afterwards even when the run was removed by an earlier attempt.

Payload schema, version migration, object-to-index mapping and GUI state are caller
responsibilities. Preserve all required lineage references as audio indices.
Deduplicate indices only by `Arc::ptr_eq` in the GUI capture adapter. Each attachment
index loads as a distinct `Arc<Audio>`, even when two indices have identical PCM.
Shared lineage is restored by cloning the same indexed Arc. Blob bytes can still
deduplicate on disk. The 8 GiB decoded limit sums every index, including repeats,
independently of the store's physical/logical file-byte quota.
Payload contains private local source names if needed; it must never be bundled into
a support report. This module has no network, tool installation or audio processing.

The root must be dedicated, absolute and either absent or already owned by this
format. Existing unmarked folders and links/reparse points are rejected. Storage
restricts its owned root to mode 0700 on Unix; Windows uses the supplied user-cache
directory's inherited access controls. Local recovery is not encrypted.
Storage uses immutable SHA-256-addressed float32 WAVs, immutable checksummed checkpoint
JSON, per-run OS locks and a cancellable global transaction lock. The shared logical
file-byte budget covers all runs in this root (8 GiB default); each audio asset is
limited to 512 MiB, metadata to 8 MiB, attachments to 4096 and runs to 256.
Checkpoint envelopes contain `data_json` and its SHA-256. The digest covers the
exact UTF-8 string bytes and is verified before parsing its JSON/numeric payload.
It never hashes a reserialized `Value`: default JSON float parsing can alter the
last decimal digit without any storage corruption. Applications requiring exact
f64 control values should also enable serde_json's `float_roundtrip` feature;
checksum integrity itself does not rely on that feature.
Three committed checkpoints are retained per run, plus the caller's live state.
Unchanged immutable Arc buffers do not get rehashed or rewritten on each edit.
Empty clean runs are reclaimed by `finish`. Empty interrupted startup scaffolds
are reclaimed on the next open, so they do not consume the 256-run history limit.

Budget exhaustion rejects only recovery storage and preserves prior generations.
It never removes another window's history to create room. Dismissal requires an
explicit stale run ID; it deletes only validated owned files. Unknown or damaged
material is preserved and reported. Reads validate checksums, canonical WAV layout,
finite samples, dimensions and resource bounds. Lists do not read PCM.

Crash consistency: complete blobs are synced before checkpoint rename; an
interrupted checkpoint never replaces an earlier commit. A crash can leave a
marked, incomplete transaction; bounded owned cleanup removes only its fixed files.
Dismissed runs and unreferenced blobs are atomically renamed into a reserved deletion
namespace before cleanup. A later open resumes an interrupted deletion, validates
remaining metadata, and preserves unknown or malformed material with an error.
Directory fsync is used on Unix; Windows relies on synced files and atomic same-
directory renames. Sudden hardware failure can still exceed OS durability guarantees.
