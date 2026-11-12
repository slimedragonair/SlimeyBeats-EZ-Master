<img src="rust/assets/slimeybeats-icon.png" alt="SlimeDragon Audio dragon icon" width="96" align="right">

# SlimeyBeats-EZ-Master

[![Windows checks](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/slimedragonair/SlimeyBeats-EZ-Master?color=8b5cf6)](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-14b8a6)](LICENSE)

**Local song mastering. Real-time EQ. Your music stays yours.**

SlimeyBeats-EZ-Master is a native Rust desktop mastering app for Windows. Analyze a mix, prepare a master, compare it at matched loudness, adjust the EQ while listening, and export when you are ready.

**Windows x64 · MIT licensed · No account or subscription · Local processing**

[Download Windows](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.4.2/SlimeyBeats-EZ-Master-0.4.2-Setup.exe) · [Get started](#get-started) · [Example audio](#try-the-example-track) · [User guide](docs/USER_GUIDE.md) · [Build from source](docs/BUILDING.md) · [Your music rights](OUTPUT_OWNERSHIP.md) · [Questions and ideas](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/discussions)

**Installer note:** v0.4.2 is unsigned, so Windows may show an unknown-publisher prompt. See the [installation guide](docs/INSTALLATION.md).

![SlimeyBeats welcome screen, with Analyze at the top right and separate Master and Export actions](docs/images/welcome.png)

**Analyze → Master → Export.** Analyze is at the top right. Master queue prepares a preview; the separate Export selected master button saves it when you are ready. You can tweak Manual EQ or re-master in between. [Open the welcome screen at full size](docs/images/welcome.png).

## What it does

- **Analyze first.** Inspect loudness, true peak, tonal balance and other measurements before deciding what to change.
- **Master, then refine.** Mastering creates a preview. Re-master or tweak it as often as you need; Export is a separate action.
- **Edit EQ in real time.** Seven editable bands share one graph with the live spectrum. A pinned waveform and playback controls keep the music accessible.
- **Start with an editable suggestion.** Suggested EQ is deterministic and gently influenced by the selected tonal profile. It is rule-based assistance, not a promise that every mix needs correction.
- **Compare fairly.** Original/master and pre-EQ/live-EQ comparisons support listening-only loudness matching.
- **Export carefully.** WAV, FLAC and MP3 options, decoded-file peak checks, optional integer dither and filenames that preserve existing outputs.
- **Learn as you go.** Hover control names for plain-language explanations of LUFS, peak ceilings, frequency, gain, Q and other terms.

![Closer view of the actual Manual EQ graph, numeric controls, waveform and playback controls](docs/images/manual-eq-detail.png)

**EQ while listening:** seven bands share a graph with the spectrum; the waveform and playback controls stay below. [Open the EQ detail at full size](docs/images/manual-eq-detail.png) or [the complete Manual EQ screen](docs/images/manual-eq.png). The detail above is a crop of the native app capture; no controls or measurements were redrawn.

## Get started

1. Download the [Windows Setup.exe](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.4.2/SlimeyBeats-EZ-Master-0.4.2-Setup.exe) for Windows x64. A [portable ZIP](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.4.2/SlimeyBeats-EZ-Master-0.4.2.zip) is available too.
2. Run Setup. It installs for your current Windows account without requiring administrator access.
3. Open the app and add or drop a mono/stereo audio file.
4. Click **Analyze** at the top right. Choose a profile, loudness mode and peak ceiling, then **Master queue**.
5. Compare, adjust Manual EQ or re-master. Expand **Export settings** to choose formats and a folder, then **Export selected master**.

WAV input and output work immediately, without Python or FFmpeg. Compressed formats and sample-rate conversion require FFmpeg/ffprobe installed separately. See [installation and codec setup](docs/INSTALLATION.md).

## Try the example track

The repository includes a 45-second electronic-music example based on **Technological Messup** by **Bärghest**, released under **CC0**. The input is a faded, headroom-adjusted excerpt of an existing recording, not an untouched studio premaster.

- [Example input — WAV](examples/audio/demo-input.wav?raw=1)
- [Mastered example — WAV](examples/audio/demo-master.wav?raw=1)
- [Level-matched input — MP3](examples/audio/listen-input.mp3?raw=1)
- [Level-matched master — MP3](examples/audio/listen-master.mp3?raw=1)
- [Credits, source and exact processing](examples/audio/CREDITS.md)

For local playback, open **Examples/Listen.html** in the installed/portable app, or [the source copy](examples/audio/Listen.html) after downloading this repository. The MP3 previews are matched in loudness so a louder file does not automatically seem better. GitHub may download audio links instead of showing a player.

## Your music, your rights

**The app claims 0% ownership and 0% royalties in your music.** You can release, sell, stream or license your own audio without paying the project, adding a watermark or crediting the app.

The **MIT license** lets anyone use, study, change, redistribute and sell the project's original software. Keep its copyright/license notice in software copies. That attribution condition does **not** apply to your music outputs. Third-party material keeps its own rights. Read the [output ownership policy](OUTPUT_OWNERSHIP.md) and [third-party notices](THIRD_PARTY_NOTICES.md).

## Formats and requirements

| Item | Support |
|---|---|
| Desktop | Windows x64; tested on Windows 11 |
| Input | Mono/stereo, 8–384 kHz; WAV is native, other supported codecs use FFmpeg |
| Export | WAV 16/24-bit PCM or 32-bit float; FLAC 16/24-bit; MP3 320 kbps |
| Sample-rate conversion | Optional; requires FFmpeg |
| Processing | Local; full tracks are held in memory |
| Installer data | Settings and exports are kept on your computer and preserved by uninstall |

The installer does not bundle FFmpeg, Python, a DAW, advertising or a cloud mastering service. Web, Android and iPhone versions are future work.

## Evidence and limits

The current DSP has **73 automated regression tests**. The 0.4.1 real-music retest covered 12 songs and 24 full-song renders, with independent peak/integrity checks, six export formats and additional EQ/mono/sample-rate cases. Version 0.4.2 packages the same DSP with documentation, licensing, demo audio and an installer.

On three songs on the development PC, median processing ran about **18–22× real time** with four threads. Those timings exclude decoding, startup and export. They are not competitor benchmarks or a subjective sound-quality score. Genre profiles and artifact indicators remain heuristics; listening is essential. See [validation](rust/docs/CORPUS_VALIDATION.md) and [DSP safety design](rust/docs/DSP_SAFETY.md).

## Developers

```powershell
cd rust
cargo build --release --bins
cargo test --release --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

The native app is in `rust/`; shared built-in profile data is in `mastercraft/data/`. The older Python implementation remains available as a separate reference implementation, with its [historical guide](docs/legacy/python-guide.md). The Rust and Python processors are not sample-identical and do not have identical feature sets.

[Contributing](CONTRIBUTING.md) · [Build and packaging](docs/BUILDING.md) · [Release checklist](docs/RELEASING.md) · [Security reports](SECURITY.md) · [Changelog](CHANGELOG.md)

## License and credits

Original project code, documentation and original bundled SlimeDragon artwork: [MIT](LICENSE), copyright © 2026 SlimeDragon Audio contributors. Separately credited assets and dependencies retain their own licenses. The example recording is CC0; the small Cipher regression fixture is CC BY 4.0. Neither is relicensed by the MIT notice.
