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
| Preview limit | 15 minutes per mix/reference, 384 MiB of decoded PCM, and 384 MiB source-file size; all three limits apply |
| Sample rate | A 48 kHz audio context; browser decoding resamples the source to that rate |
| Export | WAV 16-bit PCM, 24-bit PCM, or 32-bit float; optional deterministic dither for integer formats |
| Live EQ | Rust streaming processor in AudioWorklet, with smoothed parameter changes and peak protection |
| Requirements | WebAssembly, module workers, Web Audio and AudioWorklet; HTTPS or localhost |
| Session storage | Memory in the current tab; no saved session or automatic recovery |

The duration cap is **15:00 inclusive**, measured after decoding; even one frame over is rejected. A 15-minute stereo track decoded at 48 kHz contains **43,200,000 frames / 329.59 MiB of float PCM**. Source-file size and decoded size are different: a small MP3 can expand to hundreds of MiB. Mastering, A/B buffers, temporary processing and export require additional memory beyond that PCM size. The duration/file limits are shown beside Add audio file; import errors appear there too, including the measured and allowed duration.

For MP3s with recognized Xing/Info and LAME-compatible gapless metadata, declared encoder padding is removed when the browser leaves it in the decoded buffer. Already gapless decoding is kept unchanged. This prevents padding from rejecting a valid 15-minute song in WebKit; the duration cap itself stays strict. Missing/inconsistent metadata or unknown decoder layouts are not guessed, and an overlong source is never shortened to make it fit. Other formats continue through the browser decoder unchanged.

Actual memory capacity can be lower than these limits, especially on phones. Browsers decode the whole file before mastering; very long compressed files may exhaust memory during decoding. The web app keeps **one active mix plus an optional reference**, not a queue. A replacement mix releases the old session; there is no combined-duration or song-count cap. That does not promise an unlimited number of operations or a fixed capacity on every device. See the [measured long-track and repeated-use results](WEB_VALIDATION.md#long-tracks-and-repeated-use).

Use the desktop app for tracks over 15 minutes, larger projects, lossless source-rate handling, batch queues, MP3/FLAC exports, and explicit sample-rate conversion. Keep the browser tab active while auditioning; mobile browsers can suspend audio when the screen locks or the app moves to the background. Longer browser support would need incremental decoding and bounded, chunked processing/export; simply removing the guard would not solve whole-file memory pressure.

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
python web/tools/verify_imports.py --out /tmp/slimeybeats-long-imports
python web/tools/verify_gapless.py --browser webkit --out /tmp/slimeybeats-mp3-timing
python web/tools/verify_memory.py --tracks 40 --seconds 30 --out /tmp/slimeybeats-memory-check
```

The verification exercises the actual application, worker and AudioWorklet, compares worklet PCM with direct shared-Rust processing, checks deterministic suggestions and exports, tests mono/silence, verifies all three WAV formats using FFmpeg, and records all request methods/hosts. `verify_imports.py` uses actual 6-/15-minute WAV/MP3 files and an over-limit file. `verify_memory.py` replaces the mix repeatedly without Clear, masters, auditions with live EQ and exports on each cycle; `--tracks 1 --seconds 900` exercises the entire 15-minute UI workflow. On Windows it samples private committed memory for its isolated Chromium process group; WASM heap capacity is recorded separately. It never forces garbage collection. Its safety budget is a test stop condition, not a measured browser limit.

FFmpeg is a test tool, not a browser runtime dependency. The harness separately records Playwright 1.57’s WebKit screenshot synchronization warning only when it observes the matching empty style insertion; the production security policy stays unchanged, and other application errors still fail verification. `--browser firefox` and `--browser webkit` select other installed Playwright engines for the main and import checks; those desktop engines do not stand in for physical mobile devices.

## Architecture and extension points

`web/site` contains editable HTML, CSS and small ES modules. `app.js` coordinates session actions; `dsp-client.js` owns request IDs and worker cancellation; `worker.js` owns the Rust session. `audio/player.js` handles decoding, synchronized A/B routing, seeking, the listening fader and audio lifecycle. `audio/eq-processor.js` runs the shared Rust processor using preallocated sample buffers. `plots.js` draws the waveform, response and live spectrum; `view.js` renders semantic controls and reports; `help.js` owns beginner guidance.

`web/dsp` is a thin binding crate. `import_limits.rs` owns the bounds returned to the frontend; `audio/import-validation.js` checks files/decoded audio and builds the visible hint. `audio/mp3-gapless.js` reads declared MP3 timing before decoding detaches the encoded buffer, then corrects only recognized padding layouts. Its timing-field interpretation follows the [FFmpeg MP3 demuxer](https://github.com/FFmpeg/FFmpeg/blob/master/libavformat/mp3dec.c); `verify_gapless.py` independently checks actual decoded duration/alignment. No decoder dependency is added to the app. `session.rs` takes ownership of transferred samples and retains the untouched input and a pre-manual-EQ automatic master. Re-master always starts from the original. Only reference measurements and its original File are retained; a decoded reference is temporary. Preview transfer copies directly from a read-only WASM view before any mutation. `live_eq.rs` adapts `rust/src/dsp/realtime_eq.rs`; `wav_output.rs` reserves known export/decode sizes and verifies decoded exports. Browser and native exports share `rust/src/wav_encoding.rs`. `rust/src/dsp/convolution.rs` writes browser convolution output directly to interleaved samples while retaining native channel parallelism; a regression requires bit-identical results. `rust/src/clock.rs` uses a browser monotonic clock on WASM and `std::time::Instant` natively.

Full-track CPU work stays in the worker. Slider edits go directly to the audio worklet and use the existing loudness estimate; they do not trigger full-track renders. Export resolves compensation and peak limits over the full track, so its measured loudness can differ slightly from the live estimate. Track/revision checks reject obsolete results. Cancellation terminates synchronous WASM work rather than waiting for a queued worker message. Late browser fetch/decode results are discarded before committing track state, and an obsolete job cannot unlock controls owned by a newer job.

The static site and WASM are generated by `web/tools/build.py`; `runtime_cache.py` gives the connected JS/WASM/CSS asset graph one content-derived version so browser caches do not mix incompatible bindings. It preserves WASM import-table keys. Do not commit build products or private tracks. Future input codecs, sample-rate controls and mobile persistence belong in focused adapters. Keep signal processing shared with the native engine and preserve the distinction between audition controls and export settings.
