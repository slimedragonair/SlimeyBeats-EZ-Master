# Updates and optional tools

The desktop **0.5.0-beta.3 Hotfix2** build adds in-app updates and first-use setup. Install this build once; older installers do not acquire an updater without being updated. The application version stays **0.5.0-beta.3**. Its separate build revision distinguishes newer hotfixes.

## Update from the app

Open **Help → Updates & tools → Check for updates**. A small **Update available** indicator also appears near Help when the background check finds a newer build. Automatic checks run at most once a day and can be disabled; they request version information from GitHub and send no audio or project data.

Choose **Download update** and continue working. Progress and Cancel are available; the installer is checked before it becomes ready and checked again before opening. **Install update…** opens the existing Windows Setup. Save/export your audio and save Studio projects first, then confirm opening Setup. The app closes after Setup starts successfully. Setup remembers the registered installation folder and preserves settings, profiles, projects and exports; its Finish page can launch the updated app. Close other app windows and active MCP connections before Setup copies files.

Portable/source copies receive an explicit notice: Setup installs or updates the registered Windows copy, rather than replacing the portable folder. Download the portable ZIP from the release page to stay portable. Linux currently remains a source-build update route; the app never downloads a Windows installer on Linux. Browser users receive the hosted update after its deployment and a reload; download audio before reloading because browser sessions remain in tab memory.

## Use a tool without manual dependency hunting

| Action | Preparation when needed |
| --- | --- |
| Open compressed audio/reference or import selected MP3 stems | Prepare FFmpeg and ffprobe, then continue decoding. WAV-only work needs no codec download. |
| Export MP3/FLAC or change sample rate | Prepare codecs, then continue the explicit export. Selecting a format alone does not download anything. |
| **Setup and separate** a full mix | Prepare the isolated local stem engine/model, then separate the captured source in the same cancellable job. |
| Import existing stems, voice cleanup, spectral repair | No separation-model download. |
| Choose a VST3 effect when the app helper is missing | Recover the verified application-owned helper, then inspect the selected local plugin. Third-party plugins are not downloaded. |
| Generate an AI connection example when its companion is missing | Recover the verified companion, then generate its real configuration. No AI-client settings are edited and no server is started by generating the example. |

Working system codecs are reused. Otherwise Windows x64 and glibc Linux x64 use pinned private bundles, without administrator access, a global PATH change or a system Python change. Linux extraction requires system `xz`; other platforms can use their own system FFmpeg. Private cache paths follow Local AppData or XDG cache conventions and survive application upgrades. Setup verifies file hashes and runs bounded codec checks; failed/cancelled preparation keeps the existing working installation and audio. Optional setup requires internet, storage and time, with visible progress and cancellation. The downloaded separation engine uses its own managed Python environment.

The normal installer includes the MCP companion and VST3 helper; recovery is for missing helpers. Automatic recovery bundles currently target Windows x64. Linux source users build their native helpers. Read the [MCP guide](MCP.md) for its explicit owner download policy and the self-describing optional-tool routes.

## Implementation and extension points

- `rust/src/app_update/`: fixed official release catalog, semantic version/build ordering, platform/URL/size/hash validation and inert installer staging. `identity.rs` defines the build revision; `--build-info` exposes the actual executable identity for packaging checks.
- `rust/src/gui/update_state.rs`, `update_view.rs`, `update_launch.rs`: background jobs/preferences, readable Help controls and the explicit verified Setup launch. The native updater does not silently patch a running executable.
- `rust/src/downloads.rs`: shared bounded HTTPS streaming and SHA-256 publication. `native_process.rs` supervises worker timeout, cancellation and logs.
- `rust/src/optional_tools/`: fixed FFmpeg provenance, owned cache, safe archive extraction and functional codec readiness. Lookup never starts a download.
- `rust/src/component_tools/`: exact official helper-bundle entries, protocol/file hashes, owned cache and atomic readiness. `gui/tool_setup.rs` decides which requested actions need codecs. Ordinary CLI/MCP processing does not silently start network setup.
- `gui/studio/auto_setup.rs`: first-use stem setup → separation progress/cancellation orchestration. Source buffers and prepared previews remain separate from saved audio.
- `rust/tools/release_packaging/update_manifest.py`, `native_tool_bundles.py`: catalogs and compact helper bundles from the actual packaged bytes and original license notices.

Append **`UPDATE-MANIFEST-<build_revision>.json`** to the existing release after its named installer/tool assets are ready. Clients choose the highest canonical numeric revision, require its content to match that revision and never fall back to older catalogs after a selected-catalog failure. Increment the revision for every shipped hotfix even when the semantic version stays the same. Earlier manifests, installers and checksums remain immutable; the original release tag is not retargeted. Legacy `UPDATE-MANIFEST.json` is read only when no versioned catalog exists.

Build with `python rust/tools/build_release.py --out <fresh-folder> --label Hotfix2`. The producer checks GUI `--build-info`, creates independent helper bundles and emits a versioned catalog and checksums. Use a new label and revision for a subsequent build. `cargo run --release --example update_probe` explicitly checks the public catalog; `--download --cache <absolute-fresh-folder>` downloads a newer installer without launching it.

Fork publishers must review and change the compiled repository/URL policy in `rust/src/app_update/release.rs` and the manual release URL in `mod.rs`, alongside their documentation links. The repository-link configuration helper does not change this trusted download destination.

Codecs and model weights retain their own upstream licenses. The application's software license and audio-output ownership policy are unchanged. Consult candidate-specific release validation; archive tests, synthetic audio and headless UI checks do not establish physical-device listening or universal plugin compatibility.
