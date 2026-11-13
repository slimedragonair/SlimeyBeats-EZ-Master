# Desktop Studio expansion

Active work started November 13, 2026. The desktop remains the flagship. Existing mastering, live EQ and separate export behavior are acceptance requirements, not replacement targets. MCP discovery/help must describe every new operation without requiring project history.

## Delivery gates

1. **Stem Studio:** local vocal/instrumental and vocals/drums/bass/other separation using a real pretrained engine; isolated optional model/runtime setup; progress/cancel; aligned finite stems; preview before export; model/license provenance retained.
2. **Rebalancing and voice cleanup:** gain, mute/solo and pan per stem; repeatable mix render; conservative noise reduction, rumble removal and de-essing; A/B and explicit save. Imported stems should remain useful without a model runtime.
3. **Spectral repair:** a real time/frequency display and editable selection; bounded attenuation/healing; nondestructive revisions, undo and preview; tests for localized changes, channel/frame preservation and cancellation.
4. **Optional VST3:** isolated plugin probing/processing, parameter discovery/editing, bypass and explicit render; actual compatible plugin validation; clear unsupported layout/editor/platform behavior. A bad plugin must not crash the GUI/MCP process.
5. **Web/mobile parity:** shared Rust mixer/cleanup/repair where practical, responsive controls and bounded imports. Qualify model inference separately before exposing it as working. Native plugin binaries cannot be promised as browser features.

## Validation

Run focused deterministic audio regressions, real-model separation and actual plugin processing before claiming those workflows work. Check original hashes, alignment, finite samples, reconstructed mixes, peak protection, stale results, cancellation, file collisions and restart behavior. Test the desktop at normal/compact sizes and exercise browser changes in real engines. Independently review actual new artwork/captures where required by the project instructions; do not turn a compiler pass into a usability or listening claim.

Maintain Windows/Linux builds, the existing web suite, clean source packaging, dependency/model notices, README evidence and self-contained MCP examples. Basic acceptance means implemented workflows pass the documented matrix with no remaining reproduced high-impact defect; it does not mean all possible third-party plugins, devices or recordings are certified.

## Architecture boundaries

Use focused shared modules for stem mixing, voice processing and spectral operations; native-only adapters for model setup/inference and plugins; GUI state/jobs/views separated by feature; MCP request schemas and thin operation adapters. Keep downloads and model inference outside audio callbacks. Preserve immutable original/pre-EQ audio and parameter revisions. Export final files separately and never overwrite the user's source.
