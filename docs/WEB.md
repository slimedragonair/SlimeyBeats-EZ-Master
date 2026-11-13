# Private browser preview

[Open SlimeyBeats in your browser](https://slimedragonair.github.io/SlimeyBeats-EZ-Master/)

The browser preview uses the project's Rust mastering engine, compiled to WebAssembly. Audio processing happens on your device. The website's host serves static files; there is no upload endpoint, account, analytics, server rendering, or payment system.

## Use it

1. Open the page in a current browser over HTTPS. Choose **Add audio file**, drop a mix, or try the included CC0 example.
2. **Analyze**, at the top right, measures the input. Choose a tonal profile, blend, loudness mode, peak ceiling, and correction intensity.
3. **Master track** creates a preview. This does not download anything. **Play**, then compare **A · Original** and **B · Master** with **Level match** enabled.
4. Open **Manual EQ**. The purple EQ response and teal live spectrum share one plot, with separate dB and dBFS axes. Seven bands affect playback immediately while B is playing. **Suggest EQ** applies modest, deterministic moves; the tonal profile has a light influence. Suggestions are editable and do not imply that every mix needs correction.
5. On desktop, each band's frequency button expands its frequency, Q and filter type. On narrow screens, choose a numbered band to edit it. **?** buttons explain controls, including on touchscreens. Playback remains accessible while you edit.
6. **Export WAV** resolves the latest EQ, measures the encoded file after decoding it, and downloads it. Use **Report → Save JSON** for the corresponding report. Your browser controls the download folder and any save prompt.

Changing an automatic mastering setting marks the preview as needing **Re-master** and disables export until it is applied. Choosing a new reference also invalidates the master, even if the file has the same name. EQ edits do not require re-mastering. Listening volume and Level match never change the exported samples. Cancel stops the processing worker and restores the original; create a new master after cancellation.

## Formats and practical limits

| Item | Browser preview |
| --- | --- |
| Input | One mono/stereo track; formats supported by the current browser's decoder |
| Typical inputs | WAV, MP3, AAC/M4A, FLAC and Ogg support varies by browser |
| Preview limit | Five minutes, 96 MiB of decoded PCM, and 100 MiB source-file size |
| Sample rate | A 48 kHz audio context; browser decoding resamples the source to that rate |
| Export | WAV 16-bit PCM, 24-bit PCM, or 32-bit float; optional deterministic dither for integer formats |
| Live EQ | Rust streaming processor in AudioWorklet, with smoothed parameter changes and peak protection |
| Requirements | WebAssembly, module workers, Web Audio and AudioWorklet; HTTPS or localhost |
| Session storage | Memory in the current tab; no saved session or automatic recovery |

Actual memory capacity can be lower than these limits, especially on phones. Browsers decode the whole file before mastering; very long compressed files may exhaust memory during decoding. Use the desktop app for larger projects, lossless source-rate handling, batch queues, MP3/FLAC exports, and explicit sample-rate conversion. Keep the browser tab active while auditioning; mobile browsers can suspend audio when the screen locks or the app moves to the background.

Responsive phone layouts and desktop browser engines are testable automatically. This is **not physical iPhone or Android qualification**. Safari on an actual iPhone, speaker/headphone routing, background interruptions, thermal behavior, and full-song memory limits need device testing before promising mobile support. A browser preview avoids an App Store installation; it is not a native iOS or Android app.

## Privacy and ownership

Audio files, reference audio, decoded samples, measurements and results stay in the tab. The app uses same-origin GET requests to load its code, artwork and optional example recording. Loading the page still exposes ordinary web requests to its static host. No user audio or measurements are put in a URL, request body, browser storage, or report service.

Closing or reloading the tab discards the session. Download your WAV and any report first. The app does not cache user audio, install a service worker, or persist private tracks. Network-free processing works after the required application files are loaded; first loading or reloading the website still needs access to those files.

The app claims no ownership or royalties in your output. No watermark or software credit is required on your music. The MIT notice must accompany software copies; rights in third-party recordings remain their own. See [output ownership](../OUTPUT_OWNERSHIP.md), [third-party notices](../THIRD_PARTY_NOTICES.md), and the [CC0 example's credits](../examples/audio/CREDITS.md).

## Build and serve

Requirements: stable Rust with the `wasm32-unknown-unknown` target, Python 3.11+, and **wasm-bindgen CLI 0.2.130** matching the locked Rust dependency.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
python web/tools/build.py
python -m http.server 8080 --bind 127.0.0.1 --directory web/dist
```

Open `http://127.0.0.1:8080`. Opening `index.html` directly as a local file will not load the worker/audio worklet correctly. `web/dist` is a self-contained static site, including original artwork, demo credits, dependency license files and output-ownership notice. It contains no user media. A static HTTPS host is sufficient; the Linux server does not receive tracks or run the DSP.

```sh
python -m pip install playwright==1.57.0
python -m playwright install --with-deps chromium
python web/tools/verify_browser.py --out /tmp/slimeybeats-browser-checks
```

The verification exercises the actual application, worker and AudioWorklet, compares worklet PCM with direct shared-Rust processing, checks deterministic suggestions and exports, tests mono/silence, verifies all three WAV formats using FFmpeg, and records all request methods/hosts. FFmpeg is a test tool, not a browser runtime dependency. The harness separately records Playwright 1.57’s WebKit screenshot synchronization warning only when it observes the matching empty style insertion; the production security policy stays unchanged, and other application errors still fail verification. `--browser firefox` and `--browser webkit` select other installed Playwright engines; those desktop engines do not stand in for physical mobile devices.

## Architecture and extension points

`web/site` contains editable HTML, CSS and small ES modules. `app.js` coordinates session actions; `dsp-client.js` owns request IDs and worker cancellation; `worker.js` owns the Rust session. `audio/player.js` handles decoding, synchronized A/B routing, seeking, the listening fader and audio lifecycle. `audio/eq-processor.js` runs the shared Rust processor using preallocated sample buffers. `plots.js` draws the waveform, response and live spectrum; `view.js` renders semantic controls and reports; `help.js` owns beginner guidance.

`web/dsp` is a thin binding crate. `session.rs` retains the untouched input and a pre-manual-EQ automatic master. Re-master always starts from the original. `live_eq.rs` adapts `rust/src/dsp/realtime_eq.rs`; `wav_output.rs` resolves EQ and verifies decoded exports. Browser and native exports share `rust/src/wav_encoding.rs`. The only engine portability adjustment is a target-specific monotonic clock in `rust/src/clock.rs`; native builds retain `std::time::Instant`.

Full-track CPU work stays in the worker. Slider edits go directly to the audio worklet and use the existing loudness estimate; they do not trigger full-track renders. Export resolves compensation and peak limits over the full track, so its measured loudness can differ slightly from the live estimate. Track/revision checks reject obsolete results. Cancellation terminates synchronous WASM work rather than waiting for a queued worker message. Late browser fetch/decode results are discarded before committing track state, and an obsolete job cannot unlock controls owned by a newer job.

The static site and WASM are generated by `web/tools/build.py`; do not commit build products or private tracks. Future input codecs, sample-rate controls and mobile persistence belong in focused adapters. Keep signal processing shared with the native engine and preserve the distinction between audition controls and export settings.
