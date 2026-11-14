# First master, then refine

1. **Add your audio.** Drop a mono/stereo file onto the window or use Add audio files. WAV works without extra codecs.
2. **Analyze.** The top-right button measures the source. A measurement or tonal deviation is information, not proof that a musical choice is wrong.
3. **Choose a direction.** Neutral is a processing preset, not bypass. Genre profiles are starting points; custom blends and an optional finished reference can guide the balance.
4. **Master.** Use the track or queue button to prepare an in-memory preview. It does not export audio. Gentler and Louder choose different loudness intents; Louder can reduce dynamics and may stop short of its target under the safety limits.
5. **Listen and adjust.** Compare Original/Master at matched loudness. In Manual EQ, A is the pre-EQ automatic master and B is the live EQ result. Playback continues while you edit.
6. **Export when ready.** Expand Export settings, choose formats/folder, then Export selected master. You can re-master before exporting. Existing output files receive unique names rather than being replaced.

## Basic, Advanced and Help

In beta.3, **Basic** is the starting view and **Advanced** reveals detailed controls. Switching modes or pages preserves processing settings; notices flag active custom values whose editors are hidden. Bottom-right **Help** (**F1** on desktop) provides the optional eight-step tour, plain-language terms, troubleshooting and desktop AI setup. Completion, decline and dismissal are remembered immediately when storage succeeds; replay stays in Help. See [Learning and Help](LEARNING_AND_HELP.md) for the tour and browser storage fallback.

In a compact desktop Overview, open **Frequency balance · spectrum** to compare the frequency graph. The waveform and playback remain below the scrolling content. Studio shows essential faders together; **Show part waveforms** and **Comparison waveform** reveal larger plots when wanted. Voice/repair editors occupy the workspace, and their pinned **Preview** action prepares the audio that **Export current preview** saves.

## Reading the controls

Studio Preview adds a separate editing workspace for stems, voice cleanup, spectral repair and compatible VST3 effects. It preserves the main mastering workflow above. Follow the [Studio guide](STUDIO.md) for model setup, imported stems, A/B, undo, projects and explicit exports.

Use desktop **0.5.0-beta.3** from the [beta.3 Preview release](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/tag/v0.5.0-beta.3); [installation](INSTALLATION.md) includes the stable v0.4.2 fallback, which does not include Studio. In beta.3, **Choose stem files…** or **Choose Suno / stem ZIP…** imports aligned parts without a model or a fixed part-count cap. Review the names, selected parts and encoding, then **Import selected stems**. All distinct parts start selected; matching WAV/MP3 versions count as one part. Supply at least two parts with identical sample rate, channel count and frame count. Imports do not automatically align or resample your stems. Model separation still offers only vocal/instrumental or four stems.

Desktop defaults to **512 MiB decoded PCM per input** and a separate **4 GiB aggregate decoded-bank budget**. Browser Studio retains **128 MiB combined decoded input** and **15 minutes per track**. More parts share those budgets; mixes, processing, undo and export need additional RAM. Archive/metadata limits also apply. The optional [MCP companion](MCP.md) exposes 38 tools for the same local workflows; an AI client connection is not needed to use Studio directly.

| Control | Meaning |
|---|---|
| Integrated LUFS | Average programme loudness with weighting/gating; not a quality score or a universal release target |
| True-peak ceiling | A reconstructed-peak limit; independent saved-file checks help catch peaks introduced by encoding/resampling |
| Correction intensity | Strength of bounded automatic tonal/dynamic corrections; reduce it to preserve more of the source character |
| Frequency | Where an EQ band works, in hertz |
| Gain | How much that band boosts/cuts, in decibels |
| Q / width | How narrow or broad the band is; higher Q is narrower |
| Compensate EQ level | Adjusts the actual processed level to offset EQ's loudness change, subject to peak/gain limits |
| Loudness match | Listening-only attenuation for A/B comparison; does not change exports |

Hover names and controls for explanations in the app.

## Remembering desktop settings

The beta.3 desktop remembers these choices across normal restarts:

- **Mastering:** export folder, formats, sample rate, filename suffix, profile, processing settings and Manual EQ.
- **Desktop preferences:** Studio's independent export folder and format, separate master/Studio listening volume and level matching, and the two/four-stem separation choice. Listening controls still do not change exported audio.

Valid changes save automatically after **750 ms without another preference change**, using atomic file replacement. Closing the app normally flushes pending changes. **Save settings** saves both files immediately.

Mastering settings remain in `rust-settings.json`; the other choices use `desktop-preferences.json`. Windows keeps both in the app's portable `user_data/` directory. Linux uses the existing [XDG configuration directory](LINUX.md#user-files-and-desktop-integration).

If either of these two preference files fails to load, an error is shown and that file is preserved. Automatic saving, including the close flush, is blocked for the affected file until you explicitly use **Save settings** to replace it with the current valid choices. Write failures also show an error.

The interface separately stores Basic/Advanced and tutorial choices in `interface-preferences.json`, beside a `tutorial-dismissed.marker` that protects completed/dismissed choices from older open windows. Interface-file recovery uses **Save current interface preferences** in Help. These files share the existing settings directory and contain no audio. [Interface storage and browser fallback](LEARNING_AND_HELP.md#remembered-choices).

This saves preferences, not loaded audio, prepared previews, the queue or Studio projects. Use **Save project** and reopen its `session.studio.json` to continue Studio work; exporting audio remains explicit. Browser sessions still disappear on reload/close, and MCP keeps its explicit session/project workflow.

## Suggested EQ and the graph

**Suggest EQ** creates a separate editable proposal. When it is ready, **Apply curve** copies it into the seven-band editor. Identical decoded samples, profile and build/platform yield repeatable proposals. The suggestion is a limited rule-based starting point, not a learned mastering model.

The purple curve shows EQ gain. The gray/teal spectrum uses a separate power scale. During playback, the output trace reads the actual live stream before listening volume/A-B matching. At the paused start it shows a full-track average; a paused seek shows the local playback window. These displays answer different questions and can differ.

The pinned waveform shows the latest full-track measured revision and updates after edits settle. It remains seekable while compact-window controls scroll. Export resolves the selected revision if background preparation is not current.

## Example and troubleshooting

Try [the bundled example](../examples/audio/CREDITS.md) first. Open `Examples/Listen.html` in an installed copy for level-matched before/after playback. A louder example is not automatically a better one.

- **Compressed file fails to open:** install FFmpeg/ffprobe; see [installation](INSTALLATION.md).
- **Target not reached:** the gain/limiter limits may be protecting the result. Read the report and audition before increasing limits.
- **Master changes tone too much:** reduce intensity, use a more suitable profile, or adjust Manual EQ. A reference from an unrelated style can suggest an unsuitable balance.
- **Controls need more room:** enlarge the window or scroll the editing/settings areas. Playback stays pinned.
- **Silent, extremely short or multichannel input:** reliable loudness/tonal assistance may be unavailable. Multichannel audio needs an explicit downmix outside the app.
