# Browser exports and live preview

These exports are available in the [main web app](https://slimedragonair.github.io/SlimeyBeats-EZ-Master/), alongside the stem timeline and Stem Doctor. The isolated [`current-work` development preview](https://slimedragonair.github.io/SlimeyBeats-EZ-Master/current-work/) remains available for testing. The Windows installer remains Hotfix3; this web publication does not change the app version or release a new installer. See [preview publication, separate storage and validation](MOBILE_PREVIEW.md).

## Save what you are hearing

Choose **WAV 16/24-bit**, **WAV 32-bit float** or **MP3 · 320 kbps** under Export quality. **Export** saves the current mastering direction and manual EQ after resolving whole-track loudness compensation and the peak ceiling. Changes to the automatic mastering direction still require **Re-master**. Listening volume and A/B level matching do not enter the saved audio.

MP3 is encoded locally by a dedicated worker. The app decodes the actual MP3 and runs the shared Rust analysis/true-peak meter on it. If encoding raises its peak above the chosen ceiling, it attenuates the original resolved float WAV and re-encodes, with at most five attempts. It does not repeatedly transcode MP3. Failure to decode/verify produces an error rather than downloading an unverified result. The report records the final decoded measurements, bitrate, source/decoded frame counts, safety gain and attempt count.

Studio's **Export current preview** uses the same WAV/MP3 choices; MP3 verification uses Studio's existing −1 dBTP export ceiling. Commit live stem fader changes with **Update mix preview** before exporting the mix or session. **Export raw stem ZIP** remains available for the immutable bank, including while a different balance is pending.

## Portable downloads

**Report → Save EQ preset** downloads a desktop-compatible version-2 `.eq.json` containing seven bands, loudness hold and output gain. It includes no recording. **Report → Download session ZIP** uses the selected WAV/MP3 format:

| Mastering ZIP | Contents |
| --- | --- |
| `master.wav` or `master.mp3` | Current resolved, verified audio export |
| `analysis.json` | Measured input/output data and separately labeled heuristic suggestions |
| `eq-profile.json` | Selected manual EQ preset |
| `settings.json` | Portable mastering settings and chosen format |
| `session.json` | Source display name/layout, settings and export scope |
| `manifest.json` | Entry names, sizes, roles and schema version |

Studio's **Download session ZIP** includes `mix.wav`/`mix.mp3`, every available raw edited stem as aligned float WAV, `stems.json` identity mapping, `analysis.json`, `settings.json`, `session.json` and `manifest.json`. Without a stem bank it contains the mix and metadata only. Studio settings describe committed bank controls and explicitly say manual EQ was not applied in Studio. A separately edited preview can differ from rendering the bank with those faders again.

These are rendered-audio/settings handoff packages, separate from native editing projects. Original recordings, reference recordings and undo history are not automatically included. Use **Help → Recovery** for workspace restoration. The packager strips local paths and bounds metadata, but the downloaded audio and display names are still your private content to share deliberately.

ZIP work runs in a separate worker and uses Blob parts instead of concatenating a second full-size archive in JavaScript. CRC32 verifies accidental corruption, not authenticity. Bounds are **512 MiB per archive**, **400 MiB per entry**, **4,096 entries** and **1 MiB per JSON document**; existing input/editing/recovery limits remain unchanged. Smaller devices can run out of memory below those budgets. Oversized downloads fail explicitly; use separate files or desktop for larger sessions.

## Timing, devices and remaining parity

The mastering input guard remains **15 minutes**, with **384 MiB source/decoded limits**. Studio retains its separate **128 MiB combined decoded-input budget**. Browser decoding remains at 48 kHz. Encoding and verification need additional temporary memory.

The pinned MP3 encoder currently does not write Xing/LAME gapless metadata. Files have short codec priming/padding, so **use WAV for exact loops and aligned stems**. An actual 900-second stereo export decoded to 900.024 seconds in Chromium, Firefox and FFmpeg and was rejected on reimport by the unchanged 900-second input guard. The app does not guess trimming or silently widen admission. The output-only verifier allows up to 250 ms of codec tail while keeping the decoded memory budget.

Live EQ now prepares new filter history over bounded work units instead of replaying up to 1.5 seconds inside one processing call. It retains current audio while preparing, then uses the existing crossfade; preparation can add up to 100 ms. Multiple settings arriving between worklet blocks coalesce to the newest complete revision. This addresses a measured source of work bursts; it does not establish the cause of the tester's unprovided recording/device problem or qualify physical mobile playback.

Desktop remains the flagship: browser neural separation, native VST3 hosting, free clip arrangement and the new desktop timeline/time-edit UI are not added here. See the [easiest-to-hardest roadmap](BROWSER_PARITY_ROADMAP.md) for issue markers, activity views and practical next parity work.

## Files and data flow

`audio/mp3-export*.js` own bounded input parsing, the pinned encoder adapter and worker lifecycle. `audio/verified-mp3-export.js` owns the decode/measure/retry policy; `audio/saved-audio-analysis.js` transfers decoded audio to the existing DSP owner. `web/dsp/src/encoded_audio.rs` validates and analyzes saved samples without replacing editable session buffers. `worker.js` and `studio-worker.js` expose that worker operation.

`session-export.js` coordinates focused audio/header, settings and metadata modules; `zip-builder.js` validates ZIP32 names/sizes and assembles Blob parts. `session-export-client.js`/`session-export-worker.js` move packaging off the UI thread. Only freshly exported disposable bytes are transferred. `app.js` connects mastering actions; `studio-export-actions.js` connects Studio actions and cancellation. Existing raw-stem ZIP behavior remains separate.

`bootstrap.js` owns the outer module-load boundary and a 30-second startup deadline, so a missing required module produces an actionable error with independent support reporting instead of a permanent Loading label. It never copies an exception's private paths or URLs into the startup message.

`rust/src/dsp/realtime_eq.rs` remains the shared native/browser processor. `audio/eq-processor.js` coalesces revisions at block boundaries. Signal processing stays in Rust; browser export packaging does not add a desktop MCP tool. The current source companion has 45 tools, including time edits and Stem Doctor; the published Hotfix3 companion has 42.

The encoder's original LGPL notices, wrapper MIT notice, corresponding source archive, build inputs, exact commits and 17 provenance hashes are retained in [`web/site/audio/encoders`](../web/site/audio/encoders/README.md). They are copied into the static build alongside the replaceable WASM asset. App-owned code remains MIT; third-party terms and output ownership remain separate.

## Verification

Use a fresh static build before checking connected JS/worklet/WASM assets. New focused commands complement the existing mastering, Studio, live-monitoring, recovery and learning suites:

```sh
python web/tools/verify_eq_preview.py --out output/eq-preview.json
python web/tools/verify_mp3_export.py --out output/mp3-export
python web/tools/verify_session_export.py
python web/tools/verify_export_workflows.py --out output/export-workflows
python web/tools/verify_session_export_lifecycle.py
python web/tools/verify_bootstrap_browser.py --out output/bootstrap
```

The MP3 roundtrip verifier also uses NumPy and FFmpeg/ffprobe as test tools; the lifecycle verifier uses Node.js with structured cloning and Blob support. The application does not require these tools. Add `--integration` to the MP3 verifier to exercise saved-file analysis and actual peak-overshoot retries. Use `--browser firefox`/`webkit` for other installed engines. The full-duration MP3 encoder probe is opt-in with `--boundary` and is not a mobile-memory qualification. Windows Playwright WebKit lacks audio APIs needed for integrated decoding; Linux WebKit is checked separately. See the [dated validation note](WEB_EXPORT_VALIDATION.md) for exact results and unresolved physical-device/listening scope.
