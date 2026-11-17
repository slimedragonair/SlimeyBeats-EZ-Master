# Private browser preview

The main browser Studio includes **Stem Doctor**: read-only timestamped issue scans, explained waveform markers, explicit conservative repair previews and metadata-only issue-report JSON. It shares the native/MCP Rust core; [workflow, limits and validation status](STEM_DOCTOR.md) distinguish source/browser checks from installer releases and physical-device acceptance.

**Browser editor:** the [stem timeline and time-edit milestone](WEB_STEM_EDITOR.md)
shares the desktop Rust processor for silence, fades, gain and reverse, while
keeping aligned-stem limits, live faders, recovery and separate exports. Its guide
records current validation and features still reserved for desktop.

**Updated web app:** [open the main browser app](https://slimedragonair.github.io/SlimeyBeats-EZ-Master/) for local 320 kbps MP3 export, portable EQ presets, mastering/Studio session ZIPs, bounded live EQ preparation, stem time edits and Stem Doctor. The separate [`current-work` preview](https://slimedragonair.github.io/SlimeyBeats-EZ-Master/current-work/) remains available for phone testing; it shows its build ID and keeps isolated preferences/recovery. **Hotfix4 desktop** includes the shared EQ preparation, timeline/time edits and Doctor plus desktop loop auditioning and the 45-tool companion; see [release scope and external QA report](HOTFIX4.md). See [preview publication and validation](MOBILE_PREVIEW.md), [export workflows and limits](WEB_EXPORTS.md), and the [browser parity roadmap](BROWSER_PARITY_ROADMAP.md). Historical measurements below retain their recorded scope.

[Open SlimeyBeats in your browser](https://slimedragonair.github.io/SlimeyBeats-EZ-Master/)

The default **local browser mode** uses the project's Rust mastering engine, compiled to WebAssembly. Local processing happens on your device; the website serves static files. This local workflow does not upload audio or require an account. The static site has no analytics or payment system.

**Optional Server processing (v2):** use the same import, Analyze, Master, live EQ, Export and Studio controls in the main web app. Switching it on alone performs availability/authentication checks; importing then uploads the original file, including references or selected stems, for server decoding and native DSP. The browser receives metadata and compressed listening streams instead of allocating whole-song PCM. Local work never falls back to the service automatically. [Workflow, capabilities, privacy and deployment status](HYBRID_STUDIO.md) distinguish local processing from remote processing and the earlier Master-only bridge.

## Use it

1. Open the page in a current browser over HTTPS. Choose **Add audio file**, drop a mix, or try the included CC0 example.
2. **Analyze**, at the top right, measures the input. Choose a tonal profile, blend, loudness mode, peak ceiling, and correction intensity.
3. **Master track** creates a preview. This does not download anything. **Play**, then compare **A · Original** and **B · Master** with **Level match** enabled.
4. Open **Manual EQ**. The purple EQ response and teal live spectrum share one plot, with separate dB and dBFS axes. Seven bands affect playback immediately while B is playing. **Suggest EQ** first shows a compact, unapplied proposal, including any gentle upper-treble cut; expand its note for all reasons and limitations. The same button then becomes **Apply curve**; choose it to replace the current seven EQ bands. Until then, your edits, the displayed EQ curve and playback stay unchanged. Listen with **B · Master**, and compare with **A · Original** at matched loudness; this compares the complete master with the original mix. Suggestions are editable heuristics, do not identify instruments or AI origin, and never export automatically. Changing the source, profile or EQ clears stale proposal metadata.
5. On desktop, each band's frequency button expands its frequency, Q and filter type. On narrow screens, choose a numbered band to edit it. **?** buttons explain controls, including on touchscreens. Playback remains accessible while you edit.
6. Choose WAV or **MP3 · 320 kbps** under **Export quality**, then **Export WAV / Export MP3**. Export resolves the latest EQ, checks the actual encoded file after decoding it, and downloads it. Local verification runs in the browser; server verification uses the native exporter. Use **Report → Save JSON** for the displayed processing report. Your browser controls the download folder and any save prompt.

The hosted site follows the protected `main` branch after its browser workflow passes. A source merge and a completed Pages deployment are separate events. See the [merge validation and availability record](MERGE_VALIDATION.md).

Changing an automatic mastering setting marks the preview as needing **Re-master** and disables export until it is applied. Choosing a new reference also invalidates the master, even if the file has the same name. EQ edits do not require re-mastering. Listening volume and Level match never change the exported samples. Local cancellation stops the processing worker and restores the original; create a new master after cancellation. Server cancellation requests that the staged job stop while keeping committed audio; see the [server workflow](HYBRID_STUDIO.md).

## Formats and practical limits

The **0.3.0-beta.3 browser Studio** includes a separate **Studio** tab for aligned imported stems/mix tracks, rebalancing, voice cleanup and spectral repair. Imported banks have **no fixed nine-part ceiling**: all discovered parts start selected for review, and extensible unique `part_N` IDs cover custom/repeated instruments. Review the names, selected parts and encoding before explicitly importing. ZIPs retain source-folder identity for repeated basenames; browser bulk pickers expose filenames without original folders, so use a ZIP or individual assignments when those names would be ambiguous.

**Local** Studio retains its **128 MiB combined decoded-input budget across the whole bank**, **128 MiB encoded file/ZIP and audio-entry bounds**, and **15-minute per-track guard**. ZIP inspection also retains an **8 MiB aggregate metadata budget**, counting UTF-8 filename bytes plus 256 bytes per entry, including ignored files. More parts share the same memory allowance; compression does not reduce decoded PCM. Processing/history/export use additional memory, and hardware can impose a lower practical limit. Use desktop or the optional server workflow for larger banks, subject to their own limits.

Decoded parts must have identical sample rate, channel count and frame count. This remains an aligned mixer: removing the local count ceiling adds no free timeline, beat matching, automatic alignment or new resampling controls. The local 48 kHz browser decoding context is unchanged. Local browser mode has no neural extraction or native VST3; optional server mode adds advertised two/four-stem extraction, while VST3 remains desktop-only. See the [Studio workflow, resource limits and qualification scope](STUDIO.md). Earlier beta.2 packages retain their original behavior; stable v0.4.2 does not include Studio. The table below describes the separate **local** mastering page, not Studio's smaller budget.

The v2 server defaults are **one hour**, **2 GiB per upload** and **2 GiB decoded Float32 PCM per input**; both duration and byte bounds apply. Neural separation retains a separate **512 MiB decoded-input cap**. Read the connected service's capabilities: these admission limits do not guarantee working-memory capacity. Its default listening stream is **192 kbps MP3**; final exports use full-resolution retained audio and the native encoders, independently of preview quality. [Server limits and formats](HYBRID_STUDIO.md).

| Item | Local browser mode |
| --- | --- |
| Input | One mono/stereo track; formats supported by the current browser's decoder |
| Typical inputs | WAV, MP3, AAC/M4A, FLAC and Ogg support varies by browser |
| Preview limit | 15 minutes per mix/reference, 384 MiB of decoded PCM, and 384 MiB source-file size; all three limits apply |
| Sample rate | A 48 kHz audio context; browser decoding resamples the source to that rate |
| Export | WAV 16-bit PCM, 24-bit PCM, or 32-bit float; MP3 320 kbps; optional deterministic dither for integer WAV |
| Live EQ | Rust streaming processor in AudioWorklet, with smoothed parameter changes and peak protection |
| Requirements | WebAssembly, module workers, Web Audio and AudioWorklet; HTTPS or localhost |
| Session storage | Memory by default; opt-in local recovery in IndexedDB with separate budgets and explicit deletion |

The duration cap is **15:00 inclusive**, measured after decoding; even one frame over is rejected. A 15-minute stereo track decoded at 48 kHz contains **43,200,000 frames / 329.59 MiB of float PCM**. Source-file size and decoded size are different: a small MP3 can expand to hundreds of MiB. Mastering, A/B buffers, temporary processing and export require additional memory beyond that PCM size. The duration/file limits are shown beside Add audio file; import errors appear there too, including the measured and allowed duration.

For MP3s with recognized Xing/Info and LAME-compatible gapless metadata, declared encoder padding is removed when the browser leaves it in the decoded buffer. Already gapless decoding is kept unchanged. This prevents padding from rejecting a valid 15-minute song in WebKit; the duration cap itself stays strict. Missing/inconsistent metadata or unknown decoder layouts are not guessed, and an overlong source is never shortened to make it fit. Other formats continue through the browser decoder unchanged.

Actual local memory capacity can be lower than these limits, especially on phones. **Local mode** decodes the whole file before mastering; very long compressed files may exhaust memory during decoding. Optional v2 server imports avoid that whole-track browser allocation. The local mastering page keeps **one active mix plus an optional reference**, not a queue. A replacement mix releases the old session; there is no combined-duration or song-count cap. That does not promise an unlimited number of operations or a fixed capacity on every device. See the [measured local long-track and repeated-use results](WEB_VALIDATION.md#long-tracks-and-repeated-use).

Use optional v2 server mode or desktop for tracks beyond the local browser limits, subject to their separate admission and memory bounds. Desktop retains batch queues and native plugin workflows; the server uses native export formats and explicit output-rate settings when advertised. Local browser and native exports use separate encoding paths. Keep the browser tab active while auditioning; mobile browsers can suspend audio when the screen locks or the app moves to the background. Longer **local** browser support would need incremental decoding and bounded, chunked processing/export; simply removing the guard would not solve whole-file memory pressure.

Responsive phone layouts and desktop browser engines are testable automatically. This is **not physical iPhone or Android qualification**. Safari on an actual iPhone, speaker/headphone routing, background interruptions, thermal behavior, and full-song memory limits need device testing before promising mobile support. A browser preview avoids an App Store installation; it is not a native iOS or Android app.

## Privacy and ownership

In default **local mode**, audio files, reference audio, decoded samples, measurements and results stay in the tab. Explicitly enabling local recovery stores private audio/settings in this site's IndexedDB beyond tab lifetime. The app uses same-origin GET requests to load its code, artwork and optional example recording. Loading the page still exposes ordinary web requests to its static host. Local processing does not send user audio or measurements to a service. In optional server mode, requested imports upload original/reference audio and selected stem labels; the service operator can access processing storage. Server access tokens stay in tab memory, not URLs, preferences or recovery. [Server privacy and cleanup](HYBRID_STUDIO.md). Browser audio storage remains off by default and separate from small interface preferences.

Without a completed local recovery point, closing/reloading discards local editing state. Export important audio first. **Help → Recovery** offers opt-in local restore points and explicit private recovery-file downloads; its 128 MiB snapshot and 256 MiB / 12-point global budgets do not change editing limits. Stored copies can be deleted explicitly and may be evicted by the browser. Streamed server projects are not copied into local recovery: download a session ZIP or export before leaving, and retain original files separately. A ZIP does not restore server credentials or full undo history. Optional server streaming registers a same-origin Service Worker for authenticated playback/downloads; it does not cache full tracks or persist credentials. **Report a problem** previews reviewed text and never attaches recordings or recovery files automatically. [Recovery and reporting workflow](RECOVERY_AND_REPORTING.md). Local processing can work without a processing service after required application files load; remote streams and tools need the server connection.

The app claims no ownership or royalties in your output. No watermark or software credit is required on your music. The MIT notice must accompany software copies; rights in third-party recordings remain their own. See [output ownership](../OUTPUT_OWNERSHIP.md), [third-party notices](../THIRD_PARTY_NOTICES.md), and the [CC0 example's credits](../examples/audio/CREDITS.md).

## Build and serve

Requirements: stable Rust with the `wasm32-unknown-unknown` target, Python 3.11+, and **wasm-bindgen CLI 0.2.130** matching the locked Rust dependency.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
python web/tools/build.py
python -m http.server 8080 --bind 127.0.0.1 --directory web/dist
```

Open `http://127.0.0.1:8080`. Opening `index.html` directly as a local file will not load the worker/audio worklet correctly. `web/dist` is a self-contained static site, including original artwork, demo credits, dependency license files and output-ownership notice. It contains no user media. A static HTTPS host is sufficient for local mode. Optional Server processing additionally needs the separately configured native service described in [Hybrid Studio](HYBRID_STUDIO.md).

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

## Larger-bank validation scope

Chrome 156, Firefox 144 and Linux WebKit 26 each passed **20 complete beta.3 workflow groups**, including 24 manual rows, 64-part import/export, all-selected ZIPs, names/manifests, error retention and responsive controls. The web-DSP library separately passed **11 native-host binding tests**, including the 128 MiB boundary and rejection of the next sample over it. See the [beta.3 validation record](STUDIO_BETA3_VALIDATION.md) for exact scope and the separate independent GUI review and physical-device limits; earlier browser measurements remain historical.

## Architecture and extension points

`web/site` contains editable HTML, CSS and small ES modules. `app.js` coordinates session actions; `dsp-client.js` owns request IDs and worker cancellation; `worker.js` owns the Rust session. `audio/player.js` handles decoding, synchronized A/B routing, seeking, the listening fader and audio lifecycle. `audio/eq-processor.js` runs the shared Rust processor using preallocated sample buffers. `plots.js` draws the waveform, response and live spectrum; `view.js` renders semantic controls and reports; `help.js` owns beginner guidance.

`web/dsp` is a thin binding crate. `import_limits.rs` owns the bounds returned to the frontend; `audio/import-validation.js` checks files/decoded audio and builds the visible hint. `audio/mp3-gapless.js` reads declared MP3 timing before decoding detaches the encoded buffer, then corrects only recognized padding layouts. Its timing-field interpretation follows the [FFmpeg MP3 demuxer](https://github.com/FFmpeg/FFmpeg/blob/master/libavformat/mp3dec.c); `verify_gapless.py` independently checks actual decoded duration/alignment. No decoder dependency is added to the app. `session.rs` takes ownership of transferred samples and retains the untouched input and a pre-manual-EQ automatic master. Re-master always starts from the original. Only reference measurements and its original File are retained; a decoded reference is temporary. Preview transfer copies directly from a read-only WASM view before any mutation. `live_eq.rs` adapts `rust/src/dsp/realtime_eq.rs`; `wav_output.rs` reserves known export/decode sizes and verifies decoded exports. Browser and native exports share `rust/src/wav_encoding.rs`. `rust/src/dsp/convolution.rs` writes browser convolution output directly to interleaved samples while retaining native channel parallelism; a regression requires bit-identical results. `rust/src/clock.rs` uses a browser monotonic clock on WASM and `std::time::Instant` natively.

In local mode, full-track CPU work stays in the browser worker. Slider edits go directly to the audio worklet and use the existing loudness estimate; they do not trigger full-track renders. Export resolves compensation and peak limits over the full track, so its measured loudness can differ slightly from the live estimate. Track/revision checks reject obsolete results. Local cancellation terminates synchronous WASM work rather than waiting for a queued worker message. Late browser fetch/decode results are discarded before committing track state, and an obsolete job cannot unlock controls owned by a newer job. Optional server mode moves supported full-track DSP/export to the native worker while keeping streamed playback and live listening controls in the browser; [its architecture](HYBRID_STUDIO.md) is separate from this local WASM path.

The static site and WASM are generated by `web/tools/build.py`; `runtime_cache.py` gives the connected JS/WASM/CSS asset graph one content-derived version so browser caches do not mix incompatible bindings. It preserves WASM import-table keys. Do not commit build products or private tracks. Future input codecs, sample-rate controls and mobile persistence belong in focused adapters. Keep signal processing shared with the native engine and preserve the distinction between audition controls and export settings.
