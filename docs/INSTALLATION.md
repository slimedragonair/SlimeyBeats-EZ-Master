# Install SlimeyBeats-EZ-Master

## Windows Studio Preview installer — v0.5.0-beta.3 Hotfix5

Download [SlimeyBeats-EZ-Master-0.5.0-beta.3-Hotfix5-Setup.exe](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.5.0-beta.3/SlimeyBeats-EZ-Master-0.5.0-beta.3-Hotfix5-Setup.exe) from the [beta.3 Preview release](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/tag/v0.5.0-beta.3). This refresh uses build revision **2026111701** with the same app version/tag. See [HOTFIX5-VALIDATION.json](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.5.0-beta.3/HOTFIX5-VALIDATION.json) for exact source/artifact identities, test results and qualification limits. Run Setup and choose an installation folder. The default is inside your account's Local AppData Programs folder; administrator access is not required. A Start menu shortcut is created. A desktop shortcut is optional.

The installer is unsigned. Check that it came from the project's own release before choosing to run it; Windows can show an unknown-publisher notice. No signing certificate is bundled or purchased by the project.

Start **SlimeyBeats-EZ-Master Studio Preview** from the Start menu. The example WAV works without installing anything else. The Start menu also contains an example-audio link and the uninstaller.

Hotfix5 includes desktop **0.5.0-beta.3** and optional MCP companion **0.2.0-beta.3** with **33 compact tools** (58 in the explicit legacy profile); the matching browser Studio is **0.3.0-beta.3**. The desktop refresh includes independent multitrack editing, scoped EQ, high-frequency EQ suggestions, shared EQ preparation improvements, the [stem timeline/time edits and loop auditioning](STEM_EDITOR.md), and [Stem Doctor](STEM_DOCTOR.md). Basic/Advanced controls, a persistent optional eight-step tour and bottom-right Help remain included. Use Help (**F1** on desktop) to replay the tour or open AI setup. [Learning and Help](LEARNING_AND_HELP.md).

The Studio Preview installer includes the desktop app, MCP companion and VST3 worker. It uses **SlimeyBeats-EZ-Master Studio Preview** in Apps and the Start menu, a separate installation folder and separate registration from stable v0.4.2. Both can coexist. Upgrade a preview in its preview folder; the installer rejects the other channel's marked folder. Preview uninstall preserves settings, exports, profiles, example edits and saved Studio projects.

Open **Connect AI assistant** from its Start menu group to prepare a local connection example. Neural extraction uses an optional one-time engine/model download from Studio; other editing tools work without that runtime. Downloaded model/runtime cache data is outside the program folder and is retained on uninstall. [Studio workflow and limits](STUDIO.md).

The legacy aligned stem workspace needs no model and have no fixed part-count cap in beta.3. At least two parts must share sample rate, channel count and frame count. Desktop defaults to 512 MiB decoded PCM per input and a separate 4 GiB decoded-bank budget; browser Studio shares 128 MiB across all decoded parts and allows up to 15 minutes per track. Processing needs additional RAM. Automatic model separation remains vocal/instrumental or four stems. The Hotfix5 companion is optional to use. Independent Tracks support different durations; aligned-bank requirements above apply to the legacy workspace. See [MCP setup](MCP.md).

## Available downloads and source alternatives

Release asset cleanup retains the Hotfix5 installer and the two updater recovery ZIPs (`mcp_bridge` and `vst3_host`). Older installers and portable/source/standalone MCP ZIP attachments were removed; historical tags, checksums and validation records remain. The recovery ZIPs are components, not complete portable or standalone MCP distributions.

Stable v0.4.2 remains in source history but its Windows installer is no longer attached. Existing stable and Preview installations can coexist. For local builds, including the recorded Hotfix5 source checkpoint, follow [the build guide](BUILDING.md#get-the-source). The original beta.3 tag archive is not the Hotfix5 snapshot.

## Linux

Linux currently uses a [source build](BUILDING.md#get-the-source), with dependencies and desktop setup in [LINUX.md](LINUX.md). The beta.3 release downloads above are Windows packages; no Linux binary release is supplied.

## Optional codecs

WAV input/export is native. When compressed audio, MP3/FLAC exports or resampling need codecs, the desktop GUI reuses capable system FFmpeg/ffprobe or prepares a verified private copy and continues. Use **Help → Updates & tools → Set up audio codecs now** to prepare them in advance. There is no global PATH or system Python change, and no codec restart is needed. Progress and Cancel remain available.

Pinned private codec bundles support Windows x64 and glibc Linux x64; Linux extraction needs system `xz`. Other platforms may use their own system FFmpeg. CLI/MCP processing reuses prepared tools without hidden network setup; the MCP companion exposes explicit owner-authorized preparation. Manual system installation remains supported. See [updates and optional tools](UPDATES_AND_OPTIONAL_TOOLS.md) for locations, provenance, limits and first-use workflows.

## Settings, exports and updates

After installing Hotfix2 once, use **Help → Updates & tools** to check/download later builds and explicitly open Setup when your session is saved. Automatic metadata checks can be disabled. Portable/source and Linux copies retain their documented manual routes. Same-version hotfixes use a separate build revision; historical tags and validation records remain, but older binary attachments have been removed.

Ordinary installed/portable copies keep `user_data/rust-settings.json`, custom profiles and the default `output/` directory beside the executable. You can choose a different export directory. Installing inside a source checkout uses the development checkout's detected application root instead.

Install a new release over the same folder to keep existing settings. The installer replaces its own program/documentation files, but existing Example files of the same name are kept. It does not delete `user_data/`, `output/`, custom profiles or audio you created. Uninstall removes the installed program files and shortcuts while retaining these data folders and the bundled Examples folder. A folder remaining after uninstall can therefore be intentional.

Do not use the app's installation directory as a source-code working tree. Source builds and clean publishing exports are described in [BUILDING.md](BUILDING.md) and [RELEASING.md](RELEASING.md).
