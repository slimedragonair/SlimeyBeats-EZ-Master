# Local MCP companion architecture

The companion adds an automation surface to the desktop product without adding MCP/Tokio dependencies to the GUI or browser. It uses stdio only and shares the Rust library with both native entrypoints.

| Files | Responsibility |
|---|---|
| `mcp/server/src/protocol.rs` | Tool schemas, annotations and thin request adapters |
| `help.rs`, `capabilities.rs` | Embedded agent guidance, parameter explanations, examples and current capability discovery |
| `arguments.rs`, `file_policy.rs` | Owner-selected roots, resource settings and scoped paths |
| `state.rs`, `jobs.rs` | Immutable session snapshots, one active job, progress/cancel and bounded metadata history |
| `audio_input.rs`, `codec_process.rs` | Bounded/cancellable local imports and codec process monitoring |
| `processing.rs` | Analysis, preview/EQ, explicit export and desktop handoff operations |
| `rust/src/preview.rs` | Shared desktop/companion master and EQ rendering path |
| `rust/src/session_document.rs`, `source_identity.rs` | Versioned desktop handoff with content fingerprints |
| `mcp/tools/` | Real stdio validation, Windows configuration helper and preview packaging |

Requests validate current IDs/settings before scheduling. A worker owns a snapshot of source/reference/preview `Arc`s. It commits a new source/reference/master only when successful and still in the same session generation. Clear increments the generation and requests cancellation; it keeps the active-job slot until the worker finishes. A durable export or saved desktop handoff is reported even after a late cancellation. Export accepts one format per job, avoiding hidden partial batches.

Each preview owns a temporary file; job history stores JSON metadata rather than audio. Revisions reuse the pre-EQ master and re-render the manual processor, then measure the result. The GUI worker and EQ preparation now call this shared path. The actual running desktop playback source remains the existing live processor; MCP EQ revisions are background renders, not a remote real-time audio stream.

Desktop handoff transfers source identities/settings, not large PCM files. The GUI validates the document before reading those sources and uses its ordinary worker to rebuild the master. There is no shared mutable desktop/companion session after launch. Global settings are not saved merely by opening a handoff document.

Studio jobs reuse the same lifecycle through `studio_protocol.rs`, `studio_operations.rs` and `studio_state.rs`. Stem import/apply/export, plugin operations and self-contained desktop projects have separate route modules. The editing workspace retains its immutable original and stem-bank source label when its preview is promoted to mastering; the mastering source can then change without relabeling or replacing the Studio original. Current audio IDs remain discoverable through `get_studio_session`.

Model setup/inference and plugin process supervision live in `rust/src/studio_native`; shared mixing, cleanup and repair stay in `rust/src/studio`. Downloads and third-party native code run outside the mastering DSP. Studio handoff saves self-contained WAV assets and content fingerprints, rather than requiring the original library files. It persists working audio without making a final mastered export. [Studio files and data flow](STUDIO.md#implementation) cover browser proposals, native jobs and qualification limits; desktop model/plugin parity is not promised for browsers.
