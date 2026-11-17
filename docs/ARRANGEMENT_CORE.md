# Shared arrangement core

Import `mastercraft::studio::arrangement`. This module is separate from the
aligned `StemSet`; source buffers are never moved, padded, normalized or edited
by arrangement commands. `AssetLibrary` is `BTreeMap<String, Arc<Audio>>`.
Sources must match project rate. Mono sources can feed a stereo project without
copying. Stereo-to-mono conversion must be explicit at the import boundary.

## Exact coordinates and metadata

`Arrangement { version: 1, sample_rate, channels, lanes, eq }`.
`Lane { id, name, gain_db, pan, mute, solo, clips }`.
`Clip { id, name: Option<String>, source_id, start_frame, source_start_frame,
end_frame, gain_db, fade_in_frames, fade_out_frames }`.
Clip start/end are PROJECT frames; end is exclusive. Source duration is
`end_frame - start_frame`, with source end
`source_start_frame + end_frame - start_frame`.

`ScopedEq { id, scope, range: Option<FrameRange>, bands, output_gain_db, bypass }`
has exactly seven existing `EqBand` values. Scope JSON is tagged by `kind`:
`{"kind":"mix"}`, `{"kind":"lane","lane_id":"..."}`, or
`{"kind":"clip","clip_id":"..."}`.
Mix/lane ranges are PROJECT frames. Clip ranges are immutable SOURCE frames:

```
source_frame = clip.source_start_frame + selected_project_frame - clip.start_frame
```

Every range end is exclusive. Moves retain clip EQ source ranges. Trims retain
hidden source ranges; split/duplicate copy the original source-anchored recipe
with new effect IDs. The renderer only emits the visible clip segment.
EQ order is document `eq` order within each target; routing is source -> clip EQ
-> clip gain/fades -> lane sum -> lane EQ -> lane controls -> mix EQ -> output
gain -> one final peak guard. Wet filters run outside enabled EQ ranges so range
entry has continuous filter state. Range edges use 5 ms raised-cosine blends,
within ranges. Ranged EQ must cover at least 20 ms; a whole-target effect with
`range: None` is allowed on clips as short as one frame.
Control/engine transitions use 20 ms smoothing. No automatic loudness matching
is applied to scoped EQ.

## Commands and history

`Edit` uses `#[serde(tag="kind", rename_all="snake_case")]`:

* `add_lane {lane}`, `remove_lane {lane_id}`, `reorder_lane {lane_id,index}`
  (final zero-based position), `set_lane {lane_id,name,gain_db,pan,mute,solo}`
  (optional update fields).
* `add_clip {lane_id,clip}`, `move_clip {clip_id,lane_id,start_frame}`,
  `trim_clip {clip_id,start_frame,source_start_frame,end_frame}`,
  `split_clip {clip_id,at_frame,new_clip_id}` (left retains original ID),
  `duplicate_clip {clip_id,new_clip_id,lane_id,start_frame}`, `delete_clip {clip_id}`.
* `set_clip_name {clip_id,name}` (`null` restores asset-name fallback),
  `set_clip_gain {clip_id,gain_db}`,
  `set_clip_fades {clip_id,fade_in_frames,fade_out_frames}`.
* `set_eq {effect}` upserts by effect ID; `remove_eq {eq_id}`.

`apply_edit(&Arrangement,&Edit,&AssetLibrary)` and
`apply_edits(&Arrangement,&[Edit],&AssetLibrary)` return a validated new
`Arrangement` atomically. They do not mutate input metadata or source PCM.
Metadata and transaction admission use streamed serialized-size accounting with
a 16 MiB budget, not a fixed lane/clip/import-count limit. IDs are stable bounded
strings. Revision tokens and human-readable history labels belong in adapter
command envelopes.

`ArrangementHistory::new(initial,&assets,capacity)` accepts 1..=64 metadata
snapshots. Methods: `current`, `apply`, `apply_batch`, `undo`, `redo`, `can_undo`,
`can_redo`, `referenced_sources`. Adapters must retain assets referenced by all
retained history, including deleted clips; history itself holds no PCM.
The default combined serialized history budget is 32 MiB. Per-state byte counts
are cached; budget pressure evicts the oldest retained undo/redo states while
keeping the current document. `new_with_budget`, `metadata_bytes`, and
`metadata_budget_bytes` expose admission and diagnostics. Each document still
has its independent 16 MiB cap. Snapshot/restore and checked transport admission
are shared with the session layer.

## Actual player/render API

```rust
ArrangementPlayer::new(&Arrangement, &AssetLibrary, &RenderOptions) -> Result<Self>
ArrangementPlayer::new_cancellable(doc, assets, options, cancel) -> Result<Self>
player.sample_rate() -> u32
player.channels() -> usize
player.duration_frames() -> u64
player.position() -> u64
player.next_frame() -> Result<Option<[f32; 2]>>
player.process_block(&mut [f32]) -> Result<usize>
player.seek(frame: u64, cancel) -> Result<()>
player.prepare_seek(frame: u64, cancel) -> Result<ArrangementPlayer>
player.apply_prepared_seek(prepared: ArrangementPlayer) -> Result<ArrangementPlayer>
player.frame_at(frame: u64, cancel) -> Result<Option<[f32; 2]>>
player.replace_prepared(replacement: ArrangementPlayer) -> Result<()>
player.replace_prepared_boxed(Box<ArrangementPlayer>) -> Result<Option<Box<ArrangementPlayer>>>
player.take_retired() -> Result<Option<Box<ArrangementPlayer>>>
render(&Arrangement, &AssetLibrary, &RenderOptions, cancel) -> Result<Audio>
```

`cancel` is `&dyn Fn() -> bool`. Blocks are interleaved; the returned count is
FRAMES, and unused output is zero-filled. Mono frame results have a zero second
channel. Output is delay-compensated and exactly the arrangement duration.
Offline render consumes this same player. Empty arrangements are valid metadata
and yield no live frames; exporting one returns a clear empty-arrangement error.

`RenderOptions { output_gain_db, protect_peaks, ceiling_dbtp, max_output_bytes }`
has serde defaults `0`, `true`, `-1`, and `512 MiB`. Bypass peak protection for
raw neutral PCM equality. Output-size budget limits materialized offline PCM;
live streaming does not allocate full-length output.

Constructors prime the final guard's lookahead off-thread without advancing the
logical position. The first callback frame therefore does not replay startup
lookahead. `AssetValidationCache` admits changed source Arcs once and reuses
already validated immutable PCM. `PreparedEqUpdate::new` computes owned
coefficients on the control side; `apply_prepared_eq` rejects rate/target/range
mismatches and returns the previous metadata for off-thread retirement.

Construction, source validation, `seek`, `frame_at` when nonsequential,
`prepare_seek`, and replacement preparation run OFF the audio callback.
`prepare_seek` reuses the already validated immutable sources without a PCM
rescan. Exact seek currently replays from source-zero state and is cancellable;
it is not a constant-time seek promise. `apply_prepared_seek` swaps ownership and
returns the retired player for OFF-thread disposal.
`prepare_bounded(document,assets,options,&mut cache,frame,warm_frames,cancel)` is
the native worker's alternative: it indexes the active clips at a bounded warm
start and reconstructs at most 1.5 seconds of preceding project/filter history.
This approximates older IIR state; it is not a bit-exact replacement for exact
source-zero seek. `apply_prepared_seek_boxed` swaps a worker-allocated box without
allocating on adoption. `arrangement`, `assets`, `render_options`, and
`validation_cache` provide immutable integration snapshots.

Replacements must already be primed at the current frame and match rate/channels.
Callback users allocate the replacement box off-thread, use
`replace_prepared_boxed`, and transfer its optional retired result to a disposal
queue. `replace_prepared(Self)` is a control-thread convenience that boxes/drops.
Crossfades occur before a single continuous final guard. Call `take_retired`
regularly and transfer its returned box off-thread; queued replacements begin
after that slot is drained, bounding retirement storage. Rapid replacements
coalesce to the latest pending prepared engine. Clip boundary events and active voices are cached; playback
does not scan all document clips on every sample.

Fast live methods are `set_lane_controls(id,gain_db,pan,mute,solo)`,
`set_clip_gain(id,gain_db)`, `set_output_gain_db(gain_db)`, and `set_eq(&ScopedEq)`.
Fast EQ updates retain the existing target/range; structural edits or a changed
target/range use a prepared replacement. Initial engine settings apply directly;
live controls smooth changes. `set_eq` prepares owned coefficients/settings on
the control side; sample processing consumes those precomputed updates without
building coefficients. No per-voice limiter or audio-file rewrite occurs.

Desktop, browser/WASM, server and MCP adapters own decode/conform, persistence,
project revisions, stale-result/cancellation admission, UI selection and export
encoding. They should publish an arrangement edit only with its corresponding
prepared playable revision, retain immutable source IDs, and never infer an EQ
target from a filename.
