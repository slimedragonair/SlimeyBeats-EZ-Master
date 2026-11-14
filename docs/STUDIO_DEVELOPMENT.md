# Desktop Studio expansion

Active work started November 13, 2026. The desktop remains the flagship. Existing mastering, live EQ and separate export behavior are acceptance requirements, not replacement targets. MCP discovery/help must describe every new operation without requiring project history.

## Delivery gates

1. **Stem Studio:** local vocal/instrumental and vocals/drums/bass/other separation using a real pretrained engine; isolated optional model/runtime setup; progress/cancel; aligned finite stems; preview before export; model/license provenance retained.
2. **Rebalancing and voice cleanup:** gain, mute/solo and pan per stem; repeatable mix render; conservative noise reduction, rumble removal and de-essing; A/B and explicit save. Imported stems should remain useful without a model runtime.
3. **Spectral repair:** a real time/frequency display and editable selection; bounded attenuation/healing; nondestructive revisions, undo and preview; tests for localized changes, channel/frame preservation and cancellation.
4. **Optional VST3:** isolated plugin probing/processing, parameter discovery/editing, bypass and explicit render; actual compatible plugin validation; clear unsupported layout/editor/platform behavior. A bad plugin must not crash the GUI/MCP process.
5. **Web/mobile parity:** shared Rust mixer/cleanup/repair where practical, responsive controls and bounded imports. Qualify model inference separately before exposing it as working. Native plugin binaries cannot be promised as browser features.

## Imported banks and resource boundaries

Imported stems/mix tracks have no fixed nine-part ceiling. Select every discovered part by default, retaining explicit review and Import. Named roles and extensible canonical `part_N` IDs are stable identities; friendly display names may repeat. Preserve IDs/names through direct edits, undo, raw manifests and self-contained projects. Automatic neural extraction remains two/four only.

Native imports retain the default 512 MiB decoded allowance per input and use a separate 4 GiB aggregate decoded-bank guard. Do not reinterpret the per-input setting as the sum of the bank or regress previously supported projects. Browser Studio retains 128 MiB combined decoded PCM and a 15-minute per-track guard. These are input limits, not total RAM caps. Preserve native/browser ZIP byte limits and the 8 MiB inspection metadata budget, counting filename bytes plus 256 bytes per file/entry including ignored entries before retaining records. Existing project JSON/per-asset safeguards remain. Do not replace the removed mixer cap with another arbitrary track-count maximum.

All decoded parts must have identical sample rate, channels and frames. This change does not introduce free timeline placement, beat matching, automatic alignment or new resampling controls. Keep these boundaries visible in GUI/MCP/browser help.

## Validation

The [beta.3 validation record](STUDIO_BETA3_VALIDATION.md) tracks measured larger-bank evidence; earlier nine-part beta.2 passes remain historical. Cover larger/partial banks, all-selected defaults, canonical IDs, repeated names from distinct paths, aggregate decoded limits and metadata-budget rejection, in addition to the existing workflows.

Run focused deterministic audio regressions, real-model separation and actual plugin processing before claiming those workflows work. Check original hashes, alignment, finite samples, reconstructed mixes, peak protection, stale results, cancellation, file collisions and restart behavior. Test the desktop at normal/compact sizes and exercise browser changes in real engines. Independently review actual new artwork/captures where required by the project instructions; do not turn a compiler pass into a usability or listening claim.

Maintain Windows/Linux builds, the existing web suite, clean source packaging, dependency/model notices, README evidence and self-contained MCP examples. Basic acceptance means implemented workflows pass the documented matrix with no remaining reproduced high-impact defect; it does not mean all possible third-party plugins, devices or recordings are certified.

## Architecture boundaries

Use focused shared modules for stem mixing, voice processing and spectral operations; native-only adapters for model setup/inference and plugins; GUI state/jobs/views separated by feature; MCP request schemas and thin operation adapters. Keep downloads and model inference outside audio callbacks. Preserve immutable original/pre-EQ audio and parameter revisions. Export final files separately and never overwrite the user's source.
