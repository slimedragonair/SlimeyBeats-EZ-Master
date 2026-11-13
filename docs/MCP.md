# AI assistant connection — desktop MCP preview

SlimeyBeats is a desktop mastering workstation first. Its optional **local MCP companion 0.1.0 preview** lets an MCP-compatible AI client use the same Rust engine to analyze audio, propose changes, prepare a master, adjust EQ and export. You can open the prepared settings in the desktop app to listen and continue editing.

This is available in current source and companion preview builds. **The published v0.4.2 Windows installer does not include MCP or session handoff.** Use the desktop binary built alongside the companion. There is no native phone MCP server or browser-to-desktop bridge in this milestone. Stem separation, voice isolation, spectral repair and plugin hosting are planned ideas, not included tools.

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

## Tools

The current development companion exposes **36 tools**, including the Studio additions below. Published v0.4.2 installers do not contain this companion or Studio. See [Studio workflows, setup and qualification](STUDIO.md). `get_help` supplies the examples and units inside the connection, so an unfamiliar agent need not read repository documentation.

| Tool | Purpose |
|---|---|
| `get_capabilities` | Bundled profile names/IDs, defaults, formats, library indexes and configured limits |
| `get_help` | Self-contained quick start, examples, mastering/EQ units, export, troubleshooting and privacy |
| `list_audio_files` | A bounded, non-recursive page of supported files in one library folder |
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
| `clear_session` | Cancel pending processing and release source/preview memory |
| `get_stem_engine_status`, `install_stem_engine` | Inspect/setup the optional local CPU separation engine; setup verifies an existing model, and `repair: true` reinstalls a damaged private engine/model |
| `get_studio_session` | Current editing audio IDs, stem bank/controls, preview and undo/redo |
| `analyze_studio_audio` | Measure an available original, stem or Studio preview without changing it |
| `separate_stems`, `import_stems` | Real model separation in two/four modes, or aligned user stems without a model |
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

## Desktop handoff and saved files

`open_in_desktop` saves a versioned `*.slimeybeats.json` document in the output folder, with original/reference paths, SHA-256 fingerprints and settings. The matching desktop build verifies those files before opening, then rebuilds the preview if one was prepared. Changed audio is rejected: import it again. No final audio is automatically exported.

This is a **new desktop session**, not bidirectional synchronization. Later edits in the desktop window do not alter the companion, and later AI edits do not alter that window. Close an unwanted window before opening another revision. Listen and use desktop Export for revisions made there. Session documents contain local absolute paths; do not share them publicly without reviewing those paths.

Mastering/EQ creates a temporary 32-bit float WAV for local audition, not a final delivery file. It is removed on replacement, Clear or clean shutdown. Final exports and reports persist. A crash/forced termination can leave a `.slimeybeats-mcp-*` cache directory; remove it only after its server process has stopped. Export filenames gain a collision suffix instead of replacing an existing file. Reports use paths relative to the configured export folder and omit private input/output root settings.

## Limits and privacy

When its stdio connection closes, the companion requests cancellation and allows up to 30 seconds for a running job to finish. If that grace period expires, it logs the incomplete shutdown to stderr; temporary cache cleanup may then require removing that stopped process's cache directory.

- Processing is local. This companion contains no upload endpoint, HTTP listener or telemetry. **Your AI client may send filenames, measurements, requests and tool results to its model provider.** Its privacy policy is separate. No audio bytes are returned in MCP tool results.
- One active source and one master are kept per connection. Decoding another source only commits after success; previous state survives failed imports. Multiple AI-client connections are separate server processes and can multiply memory use.
- The default decoded input limit is **512 MiB**, configurable with `--max-decoded-mib 8..4096`. This is an input PCM bound, **not a total RAM cap**. Source, pre-EQ master, current master and DSP/export scratch buffers can use several times that amount. A stereo 48 kHz float input uses about 22 MiB per minute; the default input-only bound is about 23 minutes. Higher rates use more memory. No universal duration guarantee is claimed.
- Encoded inputs are also bounded to four times the decoded-byte allowance. External import decoder/probe calls default to 300 seconds each (`--decode-timeout-seconds 5..3600`). Native export codec/resampling calls currently use the existing desktop exporter and do not have that import timeout; cancellation waits until they return.
- File paths are canonicalized and checked against configured roots; symlink escapes are rejected. Compressed imports force a matching media demuxer and permit local file/pipe protocols, blocking a renamed network playlist. These are application access rules, **not an OS sandbox** against another process modifying the same user's filesystem. Keep codecs current and use trusted source files.
- The assistant cannot choose executable paths, run shell commands, expand its root folders or overwrite arbitrary files through these tools. Only the executable trusted at startup can be launched by desktop handoff. Do not authorize an entire drive when a smaller audio folder will do.

See [validation and remaining checks](MCP_VALIDATION.md). MIT software attribution stays with software copies; [your music has no app ownership, royalties or required app credit](../OUTPUT_OWNERSHIP.md).
