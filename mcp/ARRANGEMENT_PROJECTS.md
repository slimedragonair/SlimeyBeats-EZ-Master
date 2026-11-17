# Arrangement project persistence

**Current source discovery:** the main catalog has 33 organized tools. Use `project`
actions `save_arrangement` / `open_arrangement`, and `open_desktop` action
`arrangement`. The original names documented below remain compatibility aliases
and are advertised by `--tool-profile legacy`. See
[the current catalog](../docs/MCP_TOOL_CATALOG.md) for full calls.

The arrangement project tools use the same canonical v2 snapshot and native
directory I/O as the GUI. No rendered mix is needed to save or reopen clips,
track controls, scoped EQ, immutable sources and retained Undo/Redo.

`save_arrangement_project` takes the current arrangement session ID/revision and
returns a job. Preserve its completed `project_id`. A fresh companion using the
same output root can `load_arrangement_project` with that ID. Replacing an open
arrangement also requires its current `replace_session_id` and
`expected_revision`. Reopening returns a new session ID/revision and no preview;
rendering, exporting and opening the desktop remain explicit separate actions.

`open_arrangement_in_desktop` saves a new v2 project and invokes only the
operator-configured executable with `--studio-session`. It accepts no path or
executable from the caller. Desktop and MCP edits then evolve independently.
Before creating any project directory, that tool alone lazily probes
`--studio-project-capabilities` through the bounded process supervisor (8 KiB
stdout, three seconds, cancellation and process-tree cleanup). The response must
explicitly support v2, live arrangement editing and paused project opening.
Matching version/build labels and `desktop_configured:true` do not prove this.
Old/malformed/incomplete/timed-out probes fail with build/install-preview guidance
and leave no project folders. Discovery, save-only, reopen and existing legacy
desktop tools keep their previous behavior.

Implementation files are separated by responsibility:

- `server/src/arrangement_project_arguments.rs`: strict ID-only load schema.
- `server/src/arrangement_project_protocol.rs`: three explicit tool routes.
- `server/src/arrangement_project_operations.rs`: snapshot jobs and adoption guards.
- `server/src/arrangement_project_files.rs`: private ownership receipts, path/link
  containment and bounded manifest hash verification.
- `server/src/arrangement_project_help.rs`: complete cold-agent examples returned
  by `get_help(topic=arrangement)`.
- `server/src/arrangement_desktop.rs`: operation-specific desktop capability probe.

Projects live in a dedicated store inside the configured output root. Generated
IDs and per-root ownership receipts survive restart, while foreign roots,
symlinks/reparse points, caller paths, changed manifests/assets and resource
budget violations are rejected. Shared native I/O verifies source hashes,
geometry, source totals and metadata/history budgets. Unique reservations avoid
overwriting existing projects and clean up failed pre-publication writes.

Load adoption is rechecked in the job commit section, including cancellation
and workspace identity. Mastering and legacy Studio state are independent.
Before durable publication, cancellation/staleness preserves the prior state.
A save that already committed remains a completed durable result; later
cancellation can prevent a desktop launch and is reported honestly.
