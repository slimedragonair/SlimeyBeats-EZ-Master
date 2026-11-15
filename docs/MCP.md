# AI assistant connection — desktop MCP preview

SlimeyBeats is a desktop mastering workstation first. Its optional **local MCP companion 0.2.0-beta.3** exposes **42 tools in the current Hotfix3 source** through an MCP-compatible AI client, using the same Rust engine for analysis, mastering, EQ and Studio editing. The matching desktop is **0.5.0-beta.3**. Prepare a preview, then open it in the desktop app to listen and refine before an explicit export.

**Hotfix3 adds two private support-report tools, for 42 tools; the previously published Hotfix 2 companion has 40.** This source change does not by itself update earlier binaries. Reports use a preview/review/save flow; [privacy and review requirements](#private-support-reports) apply even though saving is local.

The [beta.3 Studio Preview release](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/tag/v0.5.0-beta.3) includes the matching desktop, companion and VST3 worker. Follow [Windows installation](INSTALLATION.md) or [build from main or the release tag](BUILDING.md#get-the-source); Linux currently requires a source build. **The stable v0.4.2 fallback does not include MCP or session handoff.** Studio includes two/four-stem local separation, **aligned imported stems/mix tracks without a fixed part-count ceiling**, voice cleanup, spectral repair, self-contained projects and optional VST3 effects. There is no native phone MCP server or browser-to-desktop bridge in this milestone.

## In-app setup preview

Beta.3 includes this route in bottom-right Help, alongside Basic/Advanced controls and the optional eight-step tour. Completed, declined and dismissed tour choices persist when storage succeeds; manual replay stays available in Help.

1. Open bottom-right **Help** (**F1**), then **AI setup**.
2. Choose **1. Choose audio folder…** and **2. Choose export folder…**. Authorize only the library you want the assistant to use.
3. Select **3. Generate connection example**. The desktop calls the real companion with `--print-config` and displays its connection JSON.
4. Use **Copy connection JSON**, add it through your AI client's supported MCP configuration, then reconnect that client. Clients with another schema need the same command and arguments.

Generating the example does not edit client settings or start an MCP server connection. The AI client launches the configured local stdio server. In Hotfix 2, the setup action can first prepare a missing application-owned companion from the official recovery catalog, then generate its configuration. If no compatible recovery bundle is available, use a complete Studio Preview package or build the companion. Changing a folder invalidates the old example, so generate it again before copying.

Browser **AI / MCP setup** is desktop-only guidance. The browser has no upload or localhost bridge to the companion; follow [desktop connection setup](#set-up-a-connection). The AI client's [privacy rules remain separate](#limits-and-privacy). For tour replay and remembered interface choices, see [Learning and Help](LEARNING_AND_HELP.md).

## Set up a connection

Use an AI client that supports launching a **local stdio MCP server**. It launches the companion; you do not need to start a network service. No account or model subscription is required by SlimeyBeats itself; the AI client has its own requirements.

For the standalone Windows bundle, [download the Hotfix3 companion ZIP](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.5.0-beta.3/SlimeyBeats-MCP-0.2.0-beta.3-Hotfix3-preview-Windows-x64.zip) and extract the complete folder to a writable location. Open **Connect AI Assistant.bat**, select an audio library folder and an export folder, and save the generated JSON example. The helper does not edit any client settings. Add its `command` and `args` using your client's MCP configuration UI or file, then reconnect. This example uses the common `mcpServers` format; clients with another format need the same executable and arguments in their own schema.

Choose a dedicated input folder containing tracks you want the assistant to access. The assistant can list/read supported audio within that folder, including a subfolder when its path is known. More than one library can be authorized with repeated `--input-root` arguments. The output folder holds new exports, reports, session documents and temporary previews.

From source:

```sh
cargo build --locked --release --bins --manifest-path rust/Cargo.toml
cargo build --locked --release --manifest-path mcp/server/Cargo.toml
```

Generate a configuration example on Windows PowerShell:

```powershell
.\mcp\server\target\release\slimeybeats-mcp.exe --input-root "D:\Audio for AI" --output-root "D:\Mastered exports" --desktop-exe ".\rust\target\release\slimeybeats-ez-master-live.exe" --print-config
```

On Linux, after installing the [native build dependencies](LINUX.md):

```sh
./mcp/server/target/release/slimeybeats-mcp --input-root "$HOME/Music/ForAI" --output-root "$HOME/Music/Masters" --desktop-exe ./rust/target/release/slimeybeats-ez-master-live --print-config
```

Save this JSON through the client's supported configuration workflow. Omit `--print-config` when running the actual server. Paths are separate arguments, so spaces are supported. Do not launch a stdio server through a wrapper that prints banners to stdout. The server reserves stdout for JSON-RPC and writes diagnostics to stderr. A desktop path is optional; without one, the companion checks for a sibling desktop binary. If neither is present, analysis/mastering/export still work, while desktop handoff is unavailable.

WAV needs no external codec. Other supported inputs, FLAC/MP3 output and resampling need a capable `ffmpeg`/`ffprobe` pair. With Hotfix 2, inspect `get_optional_tool_status`; only after owner agreement, call `prepare_optional_tool` with `tool: "audio_codecs"` and `allow_download: true`, poll, then explicitly retry the original operation. Preparation reuses capable system tools or installs the pinned private bundle shared with the GUI. Unrelated MCP tools never download codecs automatically. No application restart or client configuration edit is performed. A Linux desktop launch also needs the correct graphical-session environment; an SSH-only service may not have one.

## Start with a conversation

The server ships its own instructions, tool schemas, defaults and a `get_help` tool. An agent does not need this chat, a custom skill, repository access or a copied system prompt. `get_help` includes complete call examples, job polling, current source/preview IDs, parameter units and error recovery. Its quick-start guide is available as both structured data and JSON text for clients that consume either representation. All clients still need a working local MCP connection and a model capable of using tools.

> Find the tracks in my authorized library. Analyze one and explain the measurements in plain language. Suggest a gentle mastering direction and prepare a preview. Open it in SlimeyBeats so I can listen before exporting.

> Make a warmer revision, but keep the peak ceiling at −1 dBTP. Show me the changes. Do not export until I approve the sound.

> Export the current preview as 24-bit WAV named “My Song master”. Keep existing files.

The intended flow is **Analyze → Master → audition/EQ/re-master → Export**. Suggestions are editable proposals. The tool returns measurements and paths, not audio content; an assistant cannot claim to have heard the track from these results. Genre profiles and artifact indicators are heuristics, not universal quality ratings.

The Studio Preview installer also includes the companion and VST3 worker. Open **Connect AI assistant** from its Start menu group to prepare a connection example; the same library/output arguments apply.

## Tools

The current source and Hotfix3 companion expose **42 tools**; published Hotfix 2 has **40**. The table includes the new report tools alongside read-only ZIP inspection and job-based ZIP import. Stable v0.4.2 installers do not contain this companion or Studio. See [Studio workflows, setup and qualification](STUDIO.md). `get_help` supplies the examples and units inside the connection, so an unfamiliar agent need not read repository documentation.

| Tool | Purpose |
|---|---|
| `get_capabilities` | Bundled profile names/IDs, defaults, formats, library indexes and configured limits |
| `get_optional_tool_status` | Read-only codec readiness/host detection; distinguishes private executable hash verification from bundled-helper detection and leaves unobserved versions unknown |
| `prepare_optional_tool` | Owner-consented codec or official VST host preparation; requires `allow_download: true`, returns a cancellable job, never downloads arbitrary plugins |
| `prepare_support_report` | Return a complete allowlisted Markdown preview and connection-local review ID; no collection of raw logs/errors, private names, paths, audio or environment |
| `save_support_report` | After explicit owner review, save exactly the latest preview to a fresh `.md` filename under the authorized output root; no overwrite, upload or issue posting |
| `get_help` | Self-contained quick start, examples, mastering/EQ units, export, troubleshooting and privacy |
| `list_audio_files` | A bounded, non-recursive page of supported files in one library folder; ZIPs are marked `stem_archive: true` |
| `load_audio` | Import one source; replace the session only after successful decode |
| `load_reference`, `clear_reference` | Change the tonal reference and invalidate the old preview |
| `analyze_audio` | Measure the original track without applying mastering |
| `suggest_eq` | Propose deterministic seven-band EQ; never auto-apply it |
| `master_preview` | Prepare a temporary master using the desktop engine |
| `apply_eq` | Render an EQ revision from the unchanged pre-EQ master |
| `get_session` | Inspect current source/preview IDs, settings and active job |
| `get_job_status`, `cancel_job` | Poll progress/results or request cooperative cancellation |
| `open_in_desktop` | Verify the sources and launch a new desktop session with these settings |
| `export_audio` | Explicitly save one chosen format plus a report, preserving existing files |
| `clear_session` | Cancel processing and release source/preview memory |
| `get_stem_engine_status`, `install_stem_engine` | Inspect/setup the optional local CPU separation engine; setup verifies an existing model, and `repair: true` reinstalls a damaged private engine/model |
| `get_studio_session` | Current editing audio IDs, stem bank/controls, preview and undo/redo; stem assets add friendly `display_name` while `label` retains the stable kind |
| `analyze_studio_audio` | Measure an available original, stem or Studio preview without changing it |
| `separate_stems`, `import_stems` | Real model separation in two/four modes, or aligned loose-file batches with unique IDs and no fixed imported-part ceiling; imports need no model |
| `inspect_stem_bundle` | Read-only ZIP metadata inspection: zero-based part positions, names, suggested kinds and available WAV/MP3 versions; does not decode or replace Studio |
| `import_stem_bundle` | Import all inspected ZIP parts by default, or an explicit subset, as a job; prefer WAV or MP3 with available-format fallback; decoded-bank and inspection budgets apply |
| `mix_stems` | Patch gain, mute/solo, stereo balance and output/peak settings; prepare a mix preview |
| `clean_voice`, `repair_audio` | Voice cleanup or a precise time/frequency repair on a current source/stem/Studio preview |
| `undo_studio_edit`, `redo_studio_edit` | Restore a retained editing revision and its bank/controls |
| `apply_preview_to_stem` | Apply a direct current-stem edit back to that stem and rebuild the mix |
| `export_studio_audio`, `export_stems` | Explicit processed-audio export/report or aligned raw float stem files |
| `save_studio_project`, `open_studio_in_desktop` | Save self-contained working audio/settings, optionally launching the matching desktop |
| `use_studio_for_mastering` | Promote the prepared edit to a new mastering source ID while retaining Studio |
| `list_plugins`, `probe_plugin`, `render_plugin` | Discover/probe/process owner-authorized VST3 effects with generic controls in the isolated helper |

Long operations return a job ID promptly. Poll `get_job_status` with a short delay between polls until `completed`, `failed` or `cancelled`. Only one audio job runs at once. Cancellation waits for DSP checkpoints; it is not an immediate interruption of every calculation. Decoder subprocesses have a timeout and are killed when cancellation is requested. If audio was already durably saved when cancellation arrives, the job reports its saved output. History retains the latest 64 jobs, not their PCM buffers.

Use the current `source_id` for analysis/mastering and current `preview_id` for EQ/export. Stale IDs are rejected. EQ patches retain omitted settings; a provided band list must contain exactly seven bands. Re-mastering retains manual EQ. Changing a reference invalidates the preview and requires re-mastering. Built-in profile IDs and weighted blends are accepted. With an owner-configured `--profiles-root`, the catalog also exposes exact `custom:<filename stem>` IDs. Tool-supplied file paths and custom-profile blends remain rejected.

## Private support reports

**Use non-sensitive descriptions from the start.** Your AI client may send tool arguments and returned report text to its model provider before report review or saving. The companion's filter cannot retract information already sent to that client. Desktop/browser local previews have a different delivery path; the MCP preview explicitly explains this distinction.

This is a development-source addition. Call `get_help` with `topic: "support_reports"` for a self-contained guide inside the connection. Report tools are synchronous and do not reserve or cancel an audio job. They keep one immutable preview of at most **64 KiB** in memory per connection; preparing another valid preview invalidates the previous review ID. Invalid preparation and failed saving preserve the last usable report and all source, reference, master, Studio/project and settings state.

**The entire report returned through MCP reaches the AI client and may reach its model provider.** Local generation/saving does not guarantee private AI-client handling. Do not automatically paste raw logs, errors, file paths, project/track/stem/plugin/profile names or credentials into the title or description. Obtain the owner's agreement before including sensitive text. Automatic diagnostics contain only the compiled companion version, its **linked engine build**, OS/architecture enums, a built-in workflow, and connection counts and yes/no/unknown states. The **installed desktop version remains unknown (`null`)**, even if a desktop executable was configured; the companion does not query or launch it. No tool readiness probe or private desktop recovery-cache read occurs. All four tool-readiness fields remain **unknown (`null`)**. They use optional booleans: `true` means an already-known ready observation and `false` means an already-known not-ready observation; neither value is inferred from an absent check. No automatic probe is performed to fill them.

First prepare a preview with deliberately user-entered text, or omit that text to use empty fields:

```json
{
  "category": "audio_playback",
  "title": "Playback stops",
  "details": "1. Import a track\n2. Press Play\n3. Playback stops"
}
```

Pass those arguments to `prepare_support_report`. Generic categories are `crash`, `freeze_or_slow`, `audio_playback`, `import_or_export`, `mastering_or_eq`, `stems`, `optional_tool_setup`, `updates`, `recovery`, `accessibility` and `other` (default). The title is limited to **512 UTF-8 bytes**, and description/reproduction steps to **16,384 UTF-8 bytes**. Both request schemas reject unknown fields. Obvious absolute paths, email addresses and credential-like text are filtered best-effort. Names in ordinary prose, relative filenames and unknown secret formats may survive. Show the **entire `preview_markdown`** and privacy notices to the owner; allow revision through a new prepare call, then obtain explicit approval to save that exact preview.

Only after that review and save approval, call `save_support_report`:

```json
{
  "review_id": "<latest review_id returned by this connection>",
  "filename": "support-report.md",
  "owner_review_confirmed": true
}
```

`owner_review_confirmed` defaults to false. It records the caller's assertion and cannot prove a human actually reviewed the report; an agent must never invent consent. `filename` is a fresh `.md` filename, at most **180 UTF-8 bytes**, directly under the owner-configured output root. Directories, traversal, absolute paths, reserved device names, stale/foreign review IDs and existing targets are refused. A collision does not suffix or overwrite: use another filename with the still-current reviewed ID. Saved bytes exactly match the immutable preview even if session settings changed after preparation. There is no upload endpoint, issue-posting action or browser launch. For security-sensitive reports, the separate private advisory route is [GitHub private security reporting](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/security/advisories/new); account/login and repository availability are outside this local tool's control.

`get_help(topic: "recovery")` offers desktop recovery guidance only. It grants no new filesystem access. The MCP does not enumerate, restore or delete the GUI's private recovery cache; review and restore those entries in the desktop interface. A desktop restoration and this MCP audio session are independent.

Development validation on **November 14, 2026** passed **34 Windows MCP tests**, **17 linked support-report backend tests**, strict Clippy/formatting and **four companion packaging tests**. The real **debug** stdio companion passed 42-tool discovery, full report review/save/consent/collision/stale-ID/privacy checks, the optional-tool consent/readiness regression, and the existing WAV mastering/EQ/export regression. These checks used synthetic audio in private test folders and preserved source/preview hashes and session settings. They do not qualify an installer, physical playback, a third-party AI client's privacy, or a new published release. Rerun the report smoke against the actual release binary before packaging:

```powershell
python mcp/tools/verify_support_reports.py --server "path/to/release/slimeybeats-mcp.exe"
```

## Optional codecs and the VST host

Hotfix 2 includes these two additional tools, for 40 tools total. The historical beta.3 validation records below remain the frozen results for the original 38-tool release. Start with `get_help` topic `optional_tools`, or the quick start and `get_capabilities.optional_tools`.

```json
{"tool":"audio_codecs","allow_download":true}
```

Pass this to `prepare_optional_tool` only after the owner requests/agrees to setup. For explicit private VST3 host recovery/preparation use `tool: "vst3_host"`. This reuses only a verified private cache and may download even when a bundled candidate is detected. It does not validate or replace the selected bundled host; normal discovery may continue to use that candidate. Omitted or false `allow_download` rejects before job/cache setup. Poll `get_job_status` every 250–1000 ms until terminal; `cancel_job` requests cooperative cancellation. Wait for completion and explicitly retry the original operation. Preparing a helper never grants plugin-root access, downloads third-party plugins, edits an AI-client configuration, restarts an app, or updates the installed application. A completed cache may remain after cancellation; cancelled work never resumes audio processing.

Codec archive limits are **192 MiB Windows x64 / 160 MiB Linux x64 GNU**, with **640 MiB** expanded bytes; Linux private decompression also needs system `xz` and uses a **256 MiB** decompression-memory limit. The official Windows x64 host recovery ZIP is capped at **192 MiB**, **384 MiB** expanded and **128 MiB** per entry. Missing/incompatible official recovery assets fail explicitly; use the full installer manually or build the helper from source. These are download/extraction limits, not total RAM/disk requirements. Staging and audio working buffers need more; the configured default per-input decoded PCM allowance remains **512 MiB**, and imported banks retain their separate **4 GiB** allowance.

Read-only status reports shared codec readiness, a private helper checked against its preparation receipt, or a bundled helper detected without a hash certification. Component and installed-application versions remain `null`; a server/library build is not evidence that an installed app was updated. Setup downloads software, never audio. The AI client still controls filenames/measurements sent to its provider. Native WAV work, voice cleanup and spectral repair do not need codec/model setup. Stem model installation remains the separate, explicitly authorized `install_stem_engine` workflow; `separate_stems` never installs it automatically.

For a companion stored in the private cache, pass `--desktop-exe <original GUI executable>` and `--profiles-root <GUI app root>/profiles` when generating configuration. `--print-config` retains both arguments as separate absolute paths. The MCP parser name is exactly `--profiles-root`; it does not derive a custom profile folder from the cached executable.

Without that flag, the original bundled-only profile API remains. With it, `profile_catalog.rs` snapshots valid JSON from the authorized folder at connection startup; `Engine` retains the catalog, and discovery, mastering, analysis, reference loading and EQ use the same snapshot. Use exact `custom:<filename stem>` IDs from `get_capabilities.profiles`. No tool can choose arbitrary profile paths, and symlinks escaping the root are rejected. Files are capped at 64 KiB each, with an 8 MiB aggregate custom-data budget and a 4,096-entry directory scan bound; invalid profiles appear in `profile_catalog.warnings`. A missing folder is empty and is not created. Reconnect explicitly after changing custom profiles. A future custom-blend extension must preserve this same catalog boundary; bundled weighted blends already work.

## Import loose stems or a Suno/stem ZIP

No neural model is needed for imported stems. Decoded parts must have identical sample rate, channel count and frame count. Removing the nine-part ceiling does not add a free timeline, beat matching, automatic alignment or new resampling controls. Automatic `separate_stems` remains limited to vocal/instrumental or four stems; imported instrument identities do not expand the model's output modes.

Use `get_capabilities` to discover configured resource limits: `stem_import.maximum` is `null`, and its kind list contains examples rather than an exhaustive ID pool. In `stem_import.resource_limits`, `max_decoded_input_mib` and `max_aggregate_decoded_bank_mib` report the separate per-file and bank allowances. At least two aligned parts are required. For a ZIP, begin with `get_help` topic `stems` and `list_audio_files`. Archives are marked `stem_archive: true`. Call **`inspect_stem_bundle`** using the listed library index and relative path:

```json
{"library": 0, "path": "My Song stems.zip"}
```

Inspection returns `parts[]` with names, suggested `kind` values and `versions`, plus `skipped_files`. **`part_index` is the zero-based position in `parts[]`**, not an encoding's file index. Case-insensitive WAV/MP3 basenames are paired within one logical source folder, including matching `wav/` and `mp3/` encoding folders under the same parent. Distinct source folders keep separate parts even when basenames repeat. Duplicate encodings within the same logical group remain ambiguous. Loose-file `import_stems` calls also support repeated friendly names from distinct paths when IDs are unique.

Call **`import_stem_bundle`** with the same `library`/`path`. `preferred_format` accepts `"wav"` (default) or `"mp3"`; a missing preferred version falls back to the available encoding. Omit `parts` to import the entire inspected bank. All parts are selected by default; supply explicit zero-based positions to import a subset. There is no fixed nine-part ceiling, but per-input, separate bank-memory and ZIP inspection budgets still apply. For example, after confirming positions 0 and 1 are wanted:

```json
{
  "library": 0,
  "path": "My Song stems.zip",
  "preferred_format": "wav",
  "parts": [{"part_index": 0}, {"part_index": 1, "kind": "part_1"}]
}
```

Filename-based role suggestions use a trailing Suno role where present, keep backing/harmony on generic parts and reserve `vocals` for the lead. Suggested review order prioritizes common instruments and lead/backing ahead of Brass/FX; all parts remain available. This is a filename heuristic, not audio inference: review names, types and the explicit selection before importing.

Each `kind` override is optional. Kinds must be unique: vocals, instrumental, drums, bass, other, guitar, piano, synth, strings, percussion, or extensible `part_N` identities such as `part_1`, `part_10` and `part_25`. Use canonical positive decimal IDs within the unsigned 32-bit range: zero, leading zeros, signs and overflow are rejected. Generic parts cover repeated/custom instruments. **Exclude the original/full mix when importing its constituent stems.**

Poll `get_job_status`, inspect the resulting IDs with `get_studio_session`, then mix/audition. Import prepares a temporary preview; saving uses `export_studio_audio`, `export_stems` or `save_studio_project` only on the user's export/save request. ZIPs accept WAV/MP3 only and ignore cover art/metadata. Only selected audio is extracted privately; no uploads occur. FFmpeg/ffprobe is still needed for native MP3 decoding. Prefer a consistent encoding, ideally WAV when available: WAV/MP3 counterparts can decode to different lengths, so available-format fallback can fail the alignment check.

For loose files, **`import_stems` already accepts a batch** and requires an explicit kind for each file. Choose one encoding per logical part:

```json
{
  "files": [
    {"library": 0, "path": "guitar-left.wav", "kind": "guitar"},
    {"library": 0, "path": "guitar-right.wav", "kind": "part_1"}
  ]
}
```

Mixing, direct stem edits, raw exports and self-contained desktop projects support the selected bank without a fixed nine-part ceiling, subject to memory and safety budgets. Original friendly names stay with the selected bank through edits/undo and project reopening. `get_studio_session` exposes them through stem `assets[].display_name` and the bank's `stem_names` mapping. Stem `assets[].label` retains the stable kind for compatibility; use the returned IDs/kinds to address audio and controls. Raw WAV filenames remain based on stable kinds, and native `stems.json` adds `display_name` beside `file` and `kind`. Browser raw ZIPs similarly contain one WAV per part plus that friendly-name manifest. [Desktop/browser import workflow and limits](STUDIO.md#import-a-whole-stem-set).

## Desktop handoff and saved files

`open_in_desktop` saves a versioned `*.slimeybeats.json` document in the output folder, with original/reference paths, SHA-256 fingerprints and settings. The matching desktop build verifies those files before opening, then rebuilds the preview if one was prepared. Changed audio is rejected: import it again. No final audio is automatically exported.

This is a **new desktop session**, not bidirectional synchronization. Later edits in the desktop window do not alter the companion, and later AI edits do not alter that window. Close an unwanted window before opening another revision. Listen and use desktop Export for revisions made there. Session documents contain local absolute paths; do not share them publicly without reviewing those paths.

Mastering/EQ creates a temporary 32-bit float WAV for local audition, not a final delivery file. It is removed on replacement, Clear or clean shutdown. Final exports and reports persist. A crash/forced termination can leave a `.slimeybeats-mcp-*` cache directory; remove it only after its server process has stopped. Export filenames gain a collision suffix instead of replacing an existing file. Reports use paths relative to the configured export folder and omit private input/output root settings.

## Limits and privacy

When its stdio connection closes, the companion requests cancellation and allows up to 30 seconds for a running job to finish. If that grace period expires, it logs the incomplete shutdown to stderr; temporary cache cleanup may then require removing that stopped process's cache directory.

- Processing is local. This companion contains no upload endpoint, HTTP listener or telemetry. **Your AI client may send filenames, measurements, requests and tool results to its model provider.** Its privacy policy is separate. No audio bytes are returned in MCP tool results.
- One active source and one master are kept per connection. Decoding another source only commits after success; previous state survives failed imports. Multiple AI-client connections are separate server processes and can multiply memory use.
- The default decoded limit is **512 MiB per input**, configurable with `--max-decoded-mib 8..4096`. Imported banks retain that per-file allowance; it is not a 512 MiB cap on the sum of all parts. A separate **4 GiB aggregate decoded-bank guard** also applies; `--max-decoded-mib` changes the per-input allowance, not that bank guard. Input PCM is **not a total RAM cap**; all parts and working buffers need adequate RAM. Source, pre-EQ master, current master and DSP/export scratch buffers can use several times that amount. A stereo 48 kHz float input uses about 22 MiB per minute; the default input-only bound is about 23 minutes. Higher rates use more memory. No universal duration guarantee is claimed.
- Stem ZIPs are limited to **1 GiB** and **512 MiB per WAV/MP3 entry** after ZIP decompression. Inspection has an **8 MiB aggregate metadata budget**, counting UTF-8 filenames plus 256 bytes per file/entry, including ignored files, before retaining metadata records. These are resource/security bounds, not a nine-part mixer limit. Unencrypted stored/deflate ZIPs are supported. Inspection, decoding and selection errors or cancellation retain the previous Studio preview.
- Encoded audio inputs are also bounded to four times the decoded-byte allowance. External import decoder/probe calls default to 300 seconds each (`--decode-timeout-seconds 5..3600`). Native export codec/resampling calls currently use the existing desktop exporter and do not have that import timeout; cancellation waits until they return.
- File paths are canonicalized and checked against configured roots; symlink escapes are rejected. Compressed imports force a matching media demuxer and permit local file/pipe protocols, blocking a renamed network playlist. These are application access rules, **not an OS sandbox** against another process modifying the same user's filesystem. Keep codecs current and use trusted source files.
- The assistant cannot choose executable paths, run shell commands, expand its root folders or overwrite arbitrary files through these tools. Only the executable trusted at startup can be launched by desktop handoff. Do not authorize an entire drive when a smaller audio folder will do.

## Beta.3 qualification

The results below qualify the earlier beta.3 import/MCP candidate. The released in-app setup and learning interface have a separate [UI validation record](UX_VALIDATION.md), with frozen runtime hashes and hosted CI results. Rebuilt release packaging requires its own artifact and installer checks.

Confirmed MCP results include **9 Windows / 10 Linux regressions**, live **38-tool** discovery and 24/64/100-part loose/paired-ZIP banks. Raw exports were sample-exact; a 64-part project retained IDs/faders, and a targeted edit retained the other 63 raw parts exactly. Approximately 9.6 MB combined input passed an 8 MiB per-input setting, confirming that the setting is not the whole-bank allowance. Release checks also passed full eleven-part and 96-part data/project workflows with unchanged original inputs. A fresh agent imported all 96 ZIP parts and adjusted only `part_96` without exporting; after a separate explicit request, it saved 24-bit WAV, all 96 raw stems and a self-contained project, retained the other 95 controls and closed the bridge cleanly with exit code 0. Windows MCP formatting/strict Clippy and current host/plugin null-factory, exit and cancellation checks also passed. See the [beta.3 validation record](STUDIO_BETA3_VALIDATION.md) for scope and remaining checks.

## Earlier beta.2 qualification

The following results cover the earlier nine-part candidate, before count/selection changes; they remain historical.

Earlier beta.2 focused stdio checks passed with **38 tools**, nine WAV/MP3 pairs with sample-exact WAV import, real MP3/fallback, selection of nine from twelve parts, unchanged source hashes and error-state retention. Final suites passed **107 Windows / 109 Linux native tests and nine Windows / ten Linux MCP regressions**, with format checks, strict Clippy and locked release builds. The follow-up also passed **21 focused tests**. The refreshed browser distribution passed **18 Studio groups each in Chrome 156, Firefox 144 and Linux WebKit 26**; source-site **Chromium 143 also passed 18 groups**. The release VST3 host passed strict Clippy and host tests; null-factory, worker-exit and stalled-plugin cancellation checks retained the MCP source. All four actual native/browser views independently scored **9/9/9**. A **20-check actual-data retest** passed default lead/backing role assignment without kind overrides, named edits/exports/projects and unchanged original inputs. Earlier and newly named projects reopened without errors with preview/playback ready; the named project retained all nine original part names. Linux also passed its named eleven-part/core-nine workflow and twelve GUI state captures. Installer lifecycle passed under an **isolated QA app ID/menu group**, using the production NSIS template and byte-identical runtime payload. The test preserved existing Preview/stable installations and did not install the production Preview registration.

A fresh agent imported nine paired parts and changed only the vocal level without exporting, using live help/schemas alone. A separate explicit save request then produced **24-bit WAV plus report, nine raw stems and a self-contained project**, retaining settings, without invoking mastering; the bridge exited with code 0. This qualifies the tested protocol harness, not every AI client/model. See the [dated beta.2 status](STUDIO.md#validation-status--november-13-2026); the separate [MCP validation record](MCP_VALIDATION.md) also contains earlier preview evidence.

MIT software attribution stays with software copies; [your music has no app ownership, royalties or required app credit](../OUTPUT_OWNERSHIP.md).
