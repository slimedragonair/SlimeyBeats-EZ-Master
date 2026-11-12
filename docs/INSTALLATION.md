# Install SlimeyBeats-EZ-Master

## Windows installer

Download `SlimeyBeats-EZ-Master-0.4.2-Setup.exe` from the repository's Releases page. Run it and choose an installation folder. The default is inside your account's Local AppData Programs folder; administrator access is not required. A Start menu shortcut is created. A desktop shortcut is optional.

The installer is unsigned. Check that it came from the project's own release before choosing to run it; Windows can show an unknown-publisher notice. No signing certificate is bundled or purchased by the project.

Start **SlimeyBeats-EZ-Master** from the Start menu. The example WAV works without installing anything else. The Start menu also contains an example-audio link and the uninstaller.

## Portable ZIP

Extract the complete ZIP to a writable folder. Open `Start SlimeyBeats.bat` or `SlimeyBeats-EZ-Master.exe`. Keep its documentation, license and notice files together if redistributing it.

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
