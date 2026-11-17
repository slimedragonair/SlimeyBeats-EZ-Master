# Native arrangement audition

The arranger uses the existing `gui::player::Player`, `ABSource`, A/B controls,
listening volume and analyzer tap. It does not create a second player or render
temporary whole-song files for fader/EQ gestures. Existing static, mastering-EQ
and aligned-stem branches remain intact.

## Internal Player API

```rust
load_arrangement(before: Arc<Audio>, prepared: ArrangementPlayer) -> Result<()>
set_arrangement(&Arrangement, &AssetLibrary, &RenderOptions) -> Result<bool>
replace_arrangement(&Arrangement, &AssetLibrary, &RenderOptions) -> Result<bool>
has_live_arrangement() -> bool
arrangement_preparing() -> bool
arrangement_error() -> Option<String>
set_arrangement_lane_controls(id, gain_db, pan, mute, solo) -> Result<bool>
set_arrangement_clip_gain(id, gain_db) -> Result<bool>
set_arrangement_eq(&ScopedEq) -> Result<bool>
```

Initial import jobs construct the prepared engine off-thread and supply an
immutable comparison with matching rate/channels. Its frame count may differ.
The arrangement owns transport duration; Before becomes silence beyond its own
frames. Both branches stop at the arrangement end. The existing `seek`, `stop`,
`set_loop`, `toggle`, matching and workspace-audibility controls remain in use.

`set_arrangement` returns false when no arrangement is loaded; true means the
request was admitted to its live path. The UI should show pending/error status
from the two status methods. Metadata labels need no audio preparation. Changed
sources, geometry, scope/range or peak settings use background preparation.
Existing gains, pan, mute/solo, output gain and fixed-target EQ use coalesced live
updates with 20 ms control transitions. No source samples are modified.

## Files and data flow

* `player_arrangement.rs`: small public methods on the existing Player.
* `arrangement_mailbox.rs`: serialized control writers, latest metadata, typed
  prepared EQ controls, epoch, requested seek and bounded ownership mailboxes.
* `arrangement_worker.rs`: source-validation caching, bounded state preparation,
  epoch cancellation, preparation errors and retired-engine destruction.
* `live_arrangement_path.rs`: nonwaiting callback polling, incremental catch-up,
  prepared adoption and live control delivery.
* `audio_source.rs`: optional arrangement branch, shared transport and exclusive
  A/B selection. The tap remains before listening-only volume.

One worker belongs to a loaded arrangement. Geometry preparation retains the
previous playing engine. Obsolete jobs check their epoch during validation and
warmup and cannot commit. Prepared engines catch up by at most four logical
frames per callback tick until they match the current playing frame, avoiding a
perpetually stale-position rejection. Adoption uses a 5 ms crossfade before one
continuous final peak guard.

Seeks post a target/epoch and temporarily silence/freeze the requested playhead
while its prepared state is pending. They do not call exact `seek`, scan PCM or
compile a plan on the callback. A paused seek stays paused; Stop stays silent.
Loop wraps use the same asynchronous seek path, so seamless physical loop timing
is not established by the functional loop/control tests.

The native worker uses at most 1.5 seconds of project/filter warmup, with indexed
clip activation. Older IIR state is approximated; exact offline source-zero seek
remains available in the core. Newly admitted source buffers are checked once;
unchanged Arc identities do not incur a complete PCM validation scan per edit.

Retired engines travel through a bounded disposal channel to the worker. If
retirement is backpressured, further adoption waits while current audio keeps
playing. The callback consumes already compiled EQ updates; it does not build
their filter coefficients. No hard real-time deadline is asserted.

Control coalescing maps are accessed only by producers and the preparation
worker. They publish bundles of at most 16 controls; the callback borrows one
control per polling tick and retains its keys, IDs, precompiled EQ and replaced
document metadata until the complete bundle is retired to the worker. The EQ
processor's pending transitions contain numeric parameters only. Native updates
use `apply_prepared_eq_in_place` so the control's remaining heap ownership stays
in the retired bundle. Inline compiled coefficients have a documented Clippy
large-variant allowance rather than a separately boxed callback-owned variant.
Full retirement queues defer further adoption while current audio continues.

Native test instrumentation counts allocations, reallocations and releases on
the sampling thread. It exercises coalescing, rapid pending EQ fades, ignored
or stale controls, bundle replenishment and retirement backpressure through the
actual Source iterator with output disabled. This qualifies admitted control
processing; setup, failure formatting and Source teardown are outside the probe.

## Verification scope

Core regression files cover metadata atomicity, 1,000 lanes, PCM invariants,
source-anchored EQ, live/render agreement, cancellation, bounded preparation and
allocation-free prepared handoffs/sample processing.
`gui::live_arrangement_path::tests` consumes the actual native Source iterator
with the device disabled: waveform agreement, unequal A/B duration, exclusive
comparison, Stop, gain/EQ/mute/solo, obsolete epochs, bounded catch-up, paused and
late-project seeks, loop control polling and failed-input recovery.

Existing Player, live mastering EQ, live stem and loop regressions must continue
to pass. These checks qualify synthetic native output only. Physical output
devices, deadline distributions, UI pointer/keyboard acceptance and deployed
browser/mobile qualification belong to their separate integration passes.

Native timeline gestures classify and measure motion from the original pointer
press, including the first drag on an unselected clip. Escape restores the prior
lane, clip, time range and exact effect ID, including selection-only gestures that
never created a metadata draft. Lane EQ and applied lane effects share coherent
lane/clip selection; reopening the current lane keeps its selected time section.
`arrangement_gesture_tests.rs` drives real egui press/move/release/Escape events
through production widgets, covering both trim edges, cross-lane release outside
the viewport and applied-effect targeting. The curve regression compares the
drawn proposal with the shared EQ processor at positive/negative output gain.
These headless checks qualify widget event routing, not OS/device interaction.
