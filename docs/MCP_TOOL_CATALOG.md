# MCP tool catalog and compatibility profiles

The companion's default **compact** profile advertises **33 tools**: 19 frequent
operations keep their established names, and 14 tools group related operations.
The explicit `--tool-profile legacy` profile advertises the original **58 tools**
and their original help. These are discovery profiles, not different audio
engines or different access permissions.

**No old function was deleted.** All 58 historical routes remain callable as
compatibility aliases in default mode, although they are not all advertised by
`tools/list`. Calling one does not add it to discovery. New agents should use
the advertised compact workflows; an older client can continue using its cached
routes, or an operator can choose the legacy discovery profile explicitly.

An already running installed connector is unchanged by a source edit or a new
local build. Its operator must replace/select the appropriate executable and
restart or reconnect the client to refresh discovery. This change does not edit
client configuration or update installed processes automatically.

## Start with discovery

Call `get_capabilities` and `get_help` with `topic: "quick_start"`. Capabilities
identify the active profile, advertised tool count, libraries, limits and
workflow groups. Inspect the relevant tool's `inputSchema` before acting.

In compact mode, `get_session` preserves mastering fields such as `source`, `master` and
`active_job_id` at the root. Its `studio` and `arrangement` objects describe the
independent legacy Studio and Tracks workspaces in one atomic snapshot:

- Mastering: `get_session.source.source_id` and `get_session.master.preview_id`.
- Legacy Studio: `get_session.studio.assets`, `stem_set` and `preview`.
- Tracks: `get_session.arrangement.session_id`, `revision`, assets and clip IDs.

Those IDs are not interchangeable. A filename is a label, not a target selector.
Use the latest returned ID/revision, particularly after Undo/Redo or reopening.

## Retained tool names

| Workflow | Retained tools |
| --- | --- |
| Discovery and files | `get_capabilities`, `get_help`, `list_audio_files`, `load_audio`, `get_session` |
| Jobs | `get_job_status`, `cancel_job` |
| Mastering | `suggest_eq`, `master_preview`, `apply_eq`, `clear_session` |
| Stems and repair | `separate_stems`, `mix_stems`, `edit_stem_region`, `apply_preview_to_stem`, `scan_audio_issues`, `use_studio_for_mastering` |
| Tracks analysis and preview | `suggest_arrangement_eq`, `render_arrangement` |

## Grouped tools and typed actions

Grouped calls use an object with one `operation` object. The nested `action`
selects a specific typed schema; remaining fields belong to that action:

```json
{
  "operation": {
    "action": "mastering",
    "source_id": "<current source_id>"
  }
}
```

This example is the arguments for `analyze`. It is not an arbitrary
`tool`/`arguments` dispatcher. Do not mix another action's IDs or fields into the
request. The exception is the read-only `optional_tool_status`: call it with
`{}` to inspect components and the stem engine, or supply its optional `tool`
filter; it does not need an operation envelope.

| Tool | `operation.action` | Target or purpose |
| --- | --- | --- |
| `reference` | `load`, `clear` | Load uses an authorized `library` and relative `path`; clear invalidates the mastering reference/preview. |
| `analyze` | `mastering`, `studio` | Respectively `source_id` or `audio_id`; measurements do not apply processing. |
| `export_result` | `mastering`, `studio`, `arrangement`, `raw_stems` | Respectively master `preview_id`, legacy `studio_preview_id`, Tracks session/revision, or `stem_set_id`. |
| `open_desktop` | `mastering`, `studio`, `arrangement` | Explicit handoff of that workspace; Tracks requires current session/revision and a compatible v2 desktop. |
| `stem_import` | `inspect_zip`, `zip`, `files` | Review ZIP parts, import a reviewed ZIP, or import explicitly identified aligned files. |
| `process_audio` | `voice_cleanup`, `spectral_repair`, `stem_doctor` | Current `audio_id`, plus that operation's typed settings/selection/repair. |
| `studio_history` | `undo`, `redo` | Legacy Studio history, not Tracks history. |
| `arrangement_edit` | `create`, `add_assets`, `apply`, `undo`, `redo` | Tracks metadata; except create, use current `session_id` and `expected_revision`. |
| `arrangement_inspect` | `state`, `waveform`, `eq_response` | Metadata/IDs, source waveform, or a complete scoped EQ response. |
| `project` | `save_arrangement`, `open_arrangement`, `save_studio` | Canonical Tracks persistence or the separate legacy Studio save. |
| `optional_tool_status` | No action required | Read-only `{}` or component filter; includes stem-engine status. |
| `setup_optional_tool` | `component`, `stem_engine` | Component `tool` or optional stem-engine `repair`, with explicitly authorized `allow_download`. |
| `plugin` | `list`, `probe`, `render` | Authorized plugin roots and actual probed class/parameter IDs only. |
| `support_report` | `preview`, `save` | Prepare an immutable local preview, then save only after explicit owner review and approval. |

## Mastering example

After listing an authorized file, call `load_audio` and poll its job. Use its
completed `source_id` for the grouped analysis:

```json
{"tool":"analyze","arguments":{"operation":{"action":"mastering","source_id":"<source_id>"}}}
```

Call retained `master_preview`, poll for `preview_id`, inspect the measurements,
and review any EQ proposal before applying it. Only after the user requests an
audio export:

```json
{"tool":"export_result","arguments":{"operation":{"action":"mastering","preview_id":"<latest preview_id>","name":"My master","format":"wav32f","dither":false}}}
```

The Studio export action uses `studio_preview_id` instead. The Tracks action uses
`session_id` and `expected_revision`. Neither chooses a workspace from a file's
name, and a project save is not a final audio export.

## Tracks save and reopen example

Create a timeline with `arrangement_edit` action `create`. For **each** source,
load it, poll, obtain its owned audio ID from `get_session.studio.assets`, and
attach it using `arrangement_edit` action `add_assets` before loading the next
source. Attached immutable sources survive subsequent source loads. Different
durations are allowed; project sample rates must match explicitly.

Apply typed clip/lane/EQ edits through action `apply`; use actions `undo` and
`redo` for Tracks history. Do not pass the legacy `redo` boolean to these fixed
direction actions. Frame ends are exclusive. Clip EQ ranges use immutable source
frames; lane/mix EQ ranges use project frames.

For an explicit project save:

```json
{"tool":"project","arguments":{"operation":{"action":"save_arrangement","session_id":"<current arrangement ID>","expected_revision":3}}}
```

Poll for the owned `project_id`. Retain it across companion restarts under the
**same configured output root**. A fresh connection with no open arrangement can
reopen it without the original input recordings:

```json
{"tool":"project","arguments":{"operation":{"action":"open_arrangement","project_id":"<saved project_id>"}}}
```

Replacing an open arrangement additionally requires both `replace_session_id`
and the current `expected_revision`. Successful reopen returns a new arrangement
session identity/revision; source, lane, clip and effect IDs remain in the saved
recipe. It restores metadata, immutable assets and retained history without
rendering a whole mix, launching the desktop or autoplaying. Mastering and legacy
Studio state remain separate.

Use `open_desktop` action `arrangement` for an explicitly requested desktop
handoff. The existing bounded v2 capability check still runs before project
creation; matching engine/build version labels alone are insufficient.

## Same processing and guard boundaries

The compact wrappers select the established guarded handlers; DSP, export,
project I/O, cancellation and resource policies remain shared. There is no new
codec/model installation or audio-quality claim implied by the smaller catalog.

An annotation applies to the **whole grouped tool**. Mixed read/write groups
cannot promise read-only behavior merely because one action lists metadata.
Conservative annotations do not authorize a destructive action, download,
plugin execution, export, project replacement or support-report save.

Only one heavy job may run at a time. Poll `get_job_status` until terminal before
starting dependent work. Failed/cancelled work does not supply a usable new
preview. Existing ID/revision checks, source preservation, containment,
non-clobber saves and explicit consent requirements still apply. Optional setup
uses `allow_download:true` only after owner agreement; support-report saving
requires the actual latest review ID and explicit owner approval.

## Architecture and real stdio validation

`mcp/server/src/tool_catalog.rs` owns discovery profile names and migration
mapping. Focused `compact_*_tools.rs` modules own typed action enums and dispatch
to the existing protocol handlers. `compact_help.rs` and `compact_guidance.rs`
provide compact instructions and transform static call examples. They must not
rewrite caller filenames or project audio data.

Run the new verifier against the parent's completed binary:

```powershell
python mcp/tools/verify_compact_catalog.py --server mcp/server/target/debug/slimeybeats-mcp.exe --evidence F:/Temp/SlimeyBeats-Compact-Catalog
```

The verifier imports the normal `Client` from `protocol_client.py`, copies the
binary into a private fixture, and records exact stdio requests/results. It
checks 33 unique default tools, nested typed action schemas, active profile/count,
all concrete structured help examples, mixed/stale request rejection without
state mutation, mastering analysis/export, Tracks save/cold reopen, callable
unadvertised aliases without discovery expansion, and explicit 58-tool legacy
compatibility. It does not download components, invoke third-party plugins,
start physical playback, launch the desktop, or modify installed connectors.

If the existing optional Python `jsonschema` package is available, every
concrete static `get_help` call example is validated against its advertised
tool's actual JSON Schema. The verifier otherwise records this check as
`not_run`; it does not install dependencies. Only objects containing **both**
`tool` and `arguments` are treated as call examples. A component argument such
as `operation.tool: "audio_codecs"` is not an MCP tool call. Generic templates
whose tool name is a placeholder are reported separately from concrete examples.

The result reports catalog UTF-8 byte counts using the same compact JSON
serialization for both `tools` arrays. This is a reproducible schema-size
comparison, not an estimate of model tokens or raw transport whitespace. Fewer
advertised tool names do not by themselves guarantee a smaller serialized
schema payload; use the measured results.

Export names are base stems: format/bit-depth and collision suffixes may be
added, including `_32f` for Float32 WAV. Use the completed job's returned paths
for the actual files; do not construct a path or silently rename prior exports.

## Validation scope

The real stdio verifier checked both profiles and 334 requests, including typed
rejection, unchanged state, explicit export, owned project reopen across full
shutdown, and compatibility aliases that do not expand main discovery. JSON
Schema validation covered 64 compact and 71 legacy structured help examples.
These are synthetic local tests; no network/model/plugin setup or listening is
implied.

A fresh agent with no source, documentation or chat history used only tools/list,
initialization guidance and get_help. It used 16 different advertised tools and
completed 17 processing jobs with zero MCP errors: mastering/export, ZIP stem
rebalance/Undo/Redo, and two-source section EQ/project restart/reopen/export.
Source hashes and samples outside the EQ range remained unchanged. Its runner
errors and filename assumptions were retained in the private test record; later
help corrections clarify nested session results, redo actions and returned
export filenames. This is one agent/Windows transport qualification, not a
promise that every AI client behaves identically. The ZIP fixture had no
alternate-encoding duplicates; this cold test does not qualify that edge case.
