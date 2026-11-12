# First master, then refine

1. **Add your audio.** Drop a mono/stereo file onto the window or use Add audio files. WAV works without extra codecs.
2. **Analyze.** The top-right button measures the source. A measurement or tonal deviation is information, not proof that a musical choice is wrong.
3. **Choose a direction.** Neutral is a processing preset, not bypass. Genre profiles are starting points; custom blends and an optional finished reference can guide the balance.
4. **Master queue.** This prepares an in-memory preview. It does not export audio. Gentler and Louder choose different loudness intents; Louder can reduce dynamics and may stop short of its target under the safety limits.
5. **Listen and adjust.** Compare Original/Master at matched loudness. In Manual EQ, A is the pre-EQ automatic master and B is the live EQ result. Playback continues while you edit.
6. **Export when ready.** Expand Export settings, choose formats/folder, then Export selected master. You can re-master before exporting. Existing output files receive unique names rather than being replaced.

## Reading the controls

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

## Suggested EQ and the graph

Suggest EQ creates a separate editable proposal. Apply suggested EQ copies it into the seven-band editor. Identical decoded samples, profile and build/platform yield repeatable proposals. The suggestion is a limited rule-based starting point, not a learned mastering model.

The purple curve shows EQ gain. The gray/teal spectrum uses a separate power scale. During playback, the output trace reads the actual live stream before listening volume/A-B matching. At the paused start it shows a full-track average; a paused seek shows the local playback window. These displays answer different questions and can differ.

The pinned waveform shows the latest full-track measured revision and updates after edits settle. It remains seekable while compact-window controls scroll. Export resolves the selected revision if background preparation is not current.

## Example and troubleshooting

Try [the bundled example](../examples/audio/CREDITS.md) first. Open `Examples/Listen.html` in an installed copy for level-matched before/after playback. A louder example is not automatically a better one.

- **Compressed file fails to open:** install FFmpeg/ffprobe; see [installation](INSTALLATION.md).
- **Target not reached:** the gain/limiter limits may be protecting the result. Read the report and audition before increasing limits.
- **Master changes tone too much:** reduce intensity, use a more suitable profile, or adjust Manual EQ. A reference from an unrelated style can suggest an unsuitable balance.
- **Controls need more room:** enlarge the window or scroll the editing/settings areas. Playback stays pinned.
- **Silent, extremely short or multichannel input:** reliable loudness/tonal assistance may be unavailable. Multichannel audio needs an explicit downmix outside the app.
