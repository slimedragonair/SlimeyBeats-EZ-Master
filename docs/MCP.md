# AI assistant connection — desktop MCP preview

SlimeyBeats is a desktop mastering workstation first. Its optional **local MCP companion 0.2.0-beta.3** exposes **38 tools** through an MCP-compatible AI client, using the same Rust engine for analysis, mastering, EQ and Studio editing. The matching desktop is **0.5.0-beta.3**. Prepare a preview, then open it in the desktop app to listen and refine before an explicit export.

Beta.3 is available in the current repository source; its binary packages have **not been published as a GitHub release**. [Build the current source](BUILDING.md). Earlier beta.2 packages retain their original nine-part import behavior. **The stable v0.4.2 Windows installer does not include MCP or session handoff.** Use the desktop binary built alongside the companion. Studio includes two/four-stem local separation, **aligned imported stems/mix tracks without a fixed part-count ceiling**, voice cleanup, spectral repair, self-contained projects and optional VST3 effects. There is no native phone MCP server or browser-to-desktop bridge in this milestone.

## In-app setup preview

The **in-development UX/learning preview** on `feature/ux-learning` adds this desktop route. It is pending review and not merged or released; existing public packages do not gain it automatically.

Get [the learning preview source](BUILDING.md#get-the-learning-preview-source) to build the interface described here.

1. Open bottom-right **Help** (**F1**), then **AI setup**.
2. Choose **1. Choose audio folder…** and **2. Choose export folder…**. Authorize only the library you want the assistant to use.
3. Select **3. Generate connection example**. The desktop calls the real companion with `--print-config` and displays its connection JSON.
4. Use **Copy connection JSON**, add it through your AI client's supported MCP configuration, then reconnect that client. Clients with another schema need the same command and arguments.

Generating the example does not edit client settings or start an MCP server connection. The AI client launches the configured local stdio server. If the companion is missing, Help explains the requirement and links to this guide; use a complete Studio Preview package or build the companion. Changing a folder invalidates the old example, so generate it again before copying.

Browser **AI / MCP setup** is desktop-only guidance. The browser has no upload or localhost bridge to the companion; follow [desktop connection setup](#set-up-a-connection). The AI client's [privacy rules remain separate](#limits-and-privacy). For tour replay and remembered interface choices, see [Learning and Help](LEARNING_AND_HELP.md).

## Set up a connection

Use an AI client that supports launching a **local stdio MCP server**. It launches the companion; you do not need to start a network service. No account or model subscription is required by SlimeyBeats itself; the AI client has its own requirements.

For a Windows companion ZIP, extract the complete folder to a writable location. Open **Connect AI Assistant.bat**, select an audio library folder and an export folder, and save the generated JSON example. The helper does not edit any client settings. Add its `command` and `args` using your client's MCP configuration UI or file, then reconnect. This example uses the common `mcpServers` format; clients with another format need the same executable and arguments in their own schema.

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

WAV needs no external codec. Other supported inputs, FLAC/MP3 output and resampling require `ffmpeg` and `ffprobe` on the client-launched process's PATH. Restart the AI client after changing PATH. These programs are not bundled. A Linux desktop launch also needs the correct graphical-session environment; an SSH-only service may not have one.

## Start with a conversation

The server ships its own instructions, tool schemas, defaults and a `get_help` tool. An agent does not need this chat, a custom skill, repository access or a copied system prompt. `get_help` includes complete call examples, job polling, current source/preview IDs, parameter units and error recovery. Its quick-start guide is available as both structured data and JSON text for clients that consume either representation. All clients still need a working local MCP connection and a model capable of using tools.

> Find the tracks in my authorized library. Analyze one and explain the measurements in plain language. Suggest a gentle mastering direction and prepare a preview. Open it in SlimeyBeats so I can listen before exporting.

> Make a warmer revision, but keep the peak ceiling at −1 dBTP. Show me the changes. Do not export until I approve the sound.

> Export the current preview as 24-bit WAV named “My Song master”. Keep existing files.

The intended flow is **Analyze → Master → audition/EQ/re-master → Export**. Suggestions are editable proposals. The tool returns measurements and paths, not audio content; an assistant cannot claim to have heard the track from these results. Genre profiles and artifact indicators are heuristics, not universal quality ratings.

The Studio Preview installer also includes the companion and VST3 worker. Open **Connect AI assistant** from its Start menu group to prepare a connection example; the same library/output arguments apply.

## Tools

The current development companion exposes **38 tools**, including read-only ZIP inspection and job-based ZIP import. Published v0.4.2 installers do not contain this companion or Studio. See [Studio workflows, setup and qualification](STUDIO.md). `get_help` supplies the examples and units inside the connection, so an unfamiliar agent need not read repository documentation.

| Tool | Purpose |
|---|---|
| `get_capabilities` | Bundled profile names/IDs, defaults, formats, library indexes and configured limits |
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

Use the current `source_id` for analysis/mastering and current `preview_id` for EQ/export. Stale IDs are rejected. EQ patches retain omitted settings; a provided band list must contain exactly seven bands. Re-mastering retains manual EQ. Changing a reference invalidates the preview and requires re-mastering. Built-in profile IDs and weighted blends are accepted; arbitrary profile files are deliberately outside this API.

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

These results predate the in-development in-app setup/learning pass. They are not a final qualification matrix for that UI.

Confirmed MCP results include **9 Windows / 10 Linux regressions**, live **38-tool** discovery and 24/64/100-part loose/paired-ZIP banks. Raw exports were sample-exact; a 64-part project retained IDs/faders, and a targeted edit retained the other 63 raw parts exactly. Approximately 9.6 MB combined input passed an 8 MiB per-input setting, confirming that the setting is not the whole-bank allowance. Release checks also passed full eleven-part and 96-part data/project workflows with unchanged original inputs. A fresh agent imported all 96 ZIP parts and adjusted only `part_96` without exporting; after a separate explicit request, it saved 24-bit WAV, all 96 raw stems and a self-contained project, retained the other 95 controls and closed the bridge cleanly with exit code 0. Windows MCP formatting/strict Clippy and current host/plugin null-factory, exit and cancellation checks also passed. See the [beta.3 validation record](STUDIO_BETA3_VALIDATION.md) for scope and remaining checks.

## Earlier beta.2 qualification

The following results cover the earlier nine-part candidate, before count/selection changes; they remain historical.

Earlier beta.2 focused stdio checks passed with **38 tools**, nine WAV/MP3 pairs with sample-exact WAV import, real MP3/fallback, selection of nine from twelve parts, unchanged source hashes and error-state retention. Final suites passed **107 Windows / 109 Linux native tests and nine Windows / ten Linux MCP regressions**, with format checks, strict Clippy and locked release builds. The follow-up also passed **21 focused tests**. The refreshed browser distribution passed **18 Studio groups each in Chrome 156, Firefox 144 and Linux WebKit 26**; source-site **Chromium 143 also passed 18 groups**. The release VST3 host passed strict Clippy and host tests; null-factory, worker-exit and stalled-plugin cancellation checks retained the MCP source. All four actual native/browser views independently scored **9/9/9**. A **20-check actual-data retest** passed default lead/backing role assignment without kind overrides, named edits/exports/projects and unchanged original inputs. Earlier and newly named projects reopened without errors with preview/playback ready; the named project retained all nine original part names. Linux also passed its named eleven-part/core-nine workflow and twelve GUI state captures. Installer lifecycle passed under an **isolated QA app ID/menu group**, using the production NSIS template and byte-identical runtime payload. The test preserved existing Preview/stable installations and did not install the production Preview registration.

A fresh agent imported nine paired parts and changed only the vocal level without exporting, using live help/schemas alone. A separate explicit save request then produced **24-bit WAV plus report, nine raw stems and a self-contained project**, retaining settings, without invoking mastering; the bridge exited with code 0. This qualifies the tested protocol harness, not every AI client/model. See the [dated beta.2 status](STUDIO.md#validation-status--november-13-2026); the separate [MCP validation record](MCP_VALIDATION.md) also contains earlier preview evidence.

MIT software attribution stays with software copies; [your music has no app ownership, royalties or required app credit](../OUTPUT_OWNERSHIP.md).
