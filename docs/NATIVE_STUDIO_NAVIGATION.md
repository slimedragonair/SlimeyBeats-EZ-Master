# Native Studio playback ownership

`gui/studio/arrangement_navigation.rs` owns `Studio::activate_page(page, ctx)`.
The workspace tab buttons and Doctor route through this entry point. Crossing
between Tracks and any legacy tool pauses playback, saves that engine's position
and comparison side, and calls `Player::unload` before preparing its replacement.
Switching between legacy tools retains the existing player and running state.

Tracks preparation uses the existing background job and page/epoch admission
guard. `arrangement_events.rs` captures legacy playback before either an imported
session or a prepared player replaces it, including when Tracks is already the
selected page. A zero-duration unloaded player cannot overwrite the saved state.
`arrangement_runtime.rs` stores the two audition positions/sides in memory.

Returning to legacy tools restores only playback from the retained revision and
stem bank. Pending live mix controls are republished when monitoring was active;
they are not materialized as a preview or added to history. Analysis, spectrogram,
tool settings, source PCM and revision history are not reset by navigation.
Neither direction autoplays. Failure leaves the old engine unloaded and reports
the playback error. An empty workspace stays silent.

`arrangement_navigation_tests.rs` exercises actual native tab widgets using the
device-free player fixture, different timeline/legacy durations, pending stem
control delivery, both desktop layouts, Doctor navigation and async admission.
These checks do not qualify physical-device playback. Shell Space/drop routing
and explicit import/render actions remain separate integration responsibilities.
