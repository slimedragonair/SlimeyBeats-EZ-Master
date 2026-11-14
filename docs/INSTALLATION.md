# Install SlimeyBeats-EZ-Master

## Windows Studio Preview installer — v0.5.0-beta.3

Download [SlimeyBeats-EZ-Master-0.5.0-beta.3-Setup.exe](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.5.0-beta.3/SlimeyBeats-EZ-Master-0.5.0-beta.3-Setup.exe) from the [beta.3 Preview release](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/tag/v0.5.0-beta.3). Run it and choose an installation folder. The default is inside your account's Local AppData Programs folder; administrator access is not required. A Start menu shortcut is created. A desktop shortcut is optional.

The installer is unsigned. Check that it came from the project's own release before choosing to run it; Windows can show an unknown-publisher notice. No signing certificate is bundled or purchased by the project.

Start **SlimeyBeats-EZ-Master Studio Preview** from the Start menu. The example WAV works without installing anything else. The Start menu also contains an example-audio link and the uninstaller.

Beta.3 includes desktop **0.5.0-beta.3** and optional MCP companion **0.2.0-beta.3**; the matching browser Studio is **0.3.0-beta.3**. Basic/Advanced controls, a persistent optional eight-step tour and bottom-right Help are included. Use Help (**F1** on desktop) to replay the tour or open AI setup. [Learning and Help](LEARNING_AND_HELP.md).

The Studio Preview installer includes the desktop app, MCP companion and VST3 worker. It uses **SlimeyBeats-EZ-Master Studio Preview** in Apps and the Start menu, a separate installation folder and separate registration from stable v0.4.2. Both can coexist. Upgrade a preview in its preview folder; the installer rejects the other channel's marked folder. Preview uninstall preserves settings, exports, profiles, example edits and saved Studio projects.

Open **Connect AI assistant** from its Start menu group to prepare a local connection example. Neural extraction uses an optional one-time engine/model download from Studio; other editing tools work without that runtime. Downloaded model/runtime cache data is outside the program folder and is retained on uninstall. [Studio workflow and limits](STUDIO.md).

Imported stems need no model and have no fixed part-count cap in beta.3. At least two parts must share sample rate, channel count and frame count. Desktop defaults to 512 MiB decoded PCM per input and a separate 4 GiB decoded-bank budget; browser Studio shares 128 MiB across all decoded parts and allows up to 15 minutes per track. Processing needs additional RAM. Automatic model separation remains vocal/instrumental or four stems. The bundled MCP companion is optional to use and exposes 38 tools; see [MCP setup](MCP.md).

## Stable Windows fallback — v0.4.2

If you prefer the stable mastering release, download `SlimeyBeats-EZ-Master-0.4.2-Setup.exe` from the [v0.4.2 release](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/tag/v0.4.2). Start **SlimeyBeats-EZ-Master** from its own Start menu group. Stable does not include Studio, MCP or the beta.3 learning interface. Keep its installation folder separate from Studio Preview.

## Portable ZIP

Download the portable `SlimeyBeats-EZ-Master-0.5.0-beta.3.zip` from the [beta.3 release](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/tag/v0.5.0-beta.3) and extract the complete ZIP to a writable folder. Open `Start SlimeyBeats.bat` or `SlimeyBeats-EZ-Master.exe`. Keep its documentation, license and notice files together if redistributing it.

## Linux

Linux currently uses a [source build](BUILDING.md#get-the-source), with dependencies and desktop setup in [LINUX.md](LINUX.md). The beta.3 release downloads above are Windows packages; no Linux binary release is supplied.

## Optional codecs

WAV input/export is native. Other input codecs, FLAC/MP3 exports and sample-rate conversion need **ffmpeg** and **ffprobe** on PATH. They are separate programs and are not bundled in the installer.

On Windows with Windows Package Manager, one available package is:

```powershell
winget install --exact --id Gyan.FFmpeg
```

Close and reopen the app after installation. Check the tools in a new terminal:

```powershell
ffmpeg -version
ffprobe -version
```

Use a distribution linked from the FFmpeg project if installing manually. That distribution has its own license and build configuration; the app's MIT license does not replace them.

## Settings, exports and updates

Ordinary installed/portable copies keep `user_data/rust-settings.json`, custom profiles and the default `output/` directory beside the executable. You can choose a different export directory. Installing inside a source checkout uses the development checkout's detected application root instead.

Install a new release over the same folder to keep existing settings. The installer replaces its own program/documentation files, but existing Example files of the same name are kept. It does not delete `user_data/`, `output/`, custom profiles or audio you created. Uninstall removes the installed program files and shortcuts while retaining these data folders and the bundled Examples folder. A folder remaining after uninstall can therefore be intentional.

Do not use the app's installation directory as a source-code working tree. Source builds and clean publishing exports are described in [BUILDING.md](BUILDING.md) and [RELEASING.md](RELEASING.md).
