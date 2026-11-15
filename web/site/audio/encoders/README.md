# Local browser MP3 encoder

The dedicated module worker `../mp3-export-worker.js` encodes mono/stereo audio
without sending audio or fetching executable code from an external origin.
`../mp3-export.js` owns cancellation, lifetime, copying/transfer and progress;
`../mp3-export-input.js` owns RIFF/PCM validation;
`../mp3-export-encoder.js` adapts the narrow pinned WASM ABI.

## Integration

```js
import { encodeMp3Wav } from "../mp3-export.js";
const encoded = await encodeMp3Wav(wav32fBytes, {
    bitrateKbps: 192,
    gain_db: 0,
    transferInput: false,
    signal: controller.signal,
    onProgress: (fraction, text) => {},
});
// encoded.bytes: Uint8Array; encoded.blob: audio/mpeg Blob.
// Also frames (input frames), channels, sampleRate, bitrateKbps, gain_db,
// peakVerified:false, gapless:false.
```

The WAV is the immutable rendered float32 snapshot after the user's current
processing settings. Normal RIFF and WAVE_FORMAT_EXTENSIBLE IEEE float32 are
accepted, including ancillary chunks and unaligned data. Integer/compressed WAV,
malformed/truncated chunks and partial channel frames fail explicitly.
`transferInput:true` detaches the supplied ArrayBuffer; only use it when the caller
owns disposable bytes and will not need them for a safety retry. The default
copies the input before transfer. `encodeMp3` also accepts AudioBuffer, planar
`{samples:Float32Array[], sampleRate, channels}` and interleaved
`{samples:Float32Array, sampleRate, channels}`; those inputs are copied.

After encoding, decode the actual MP3 and measure its true peak with the shared
DSP before offering it as a verified download. For an overshoot, calculate a
cumulative negative `gain_db` and encode the **original WAV** again. Set a finite
retry count and fail explicitly if the measured output still exceeds the ceiling.
This module performs neither that measurement nor the retry policy. It never
changes the source preview or claims `peakVerified:true`.

Each module instance admits one export at a time. Each job has a dedicated worker,
a ten-minute timeout, a 15-minute/384-MiB decoded PCM bound, a 385-MiB WAV bound,
and a 40-MiB MP3 output bound. Work is chunked at 16,384 frames. AbortSignal
terminates the worker; an error or abort releases admission for the next export.
Accepted rates: 32,000, 44,100, 48,000 Hz, preserved explicitly. Accepted constant
bitrates: 128, 192, 256, 320 kbps. The safety gain is limited to -60 through 0 dB.
Nonfinite/out-of-range samples fail rather than silently clip. These bounds do
not guarantee sufficient memory or encoding speed on every mobile device.

The upstream wrapper disables Xing/LAME gapless metadata. Exported MP3s therefore
contain codec priming/padding and are unsuitable for sample-exact loops/stem
alignment. Three-second fixtures decoded through FFmpeg contained 1,152–1,920
additional frames; the one-frame case contained 2,303 additional frames. Retain
WAV for exact timing. Do not label `frames` as the decoded MP3 frame count.

An actual exact 900-second, 48 kHz stereo fixture at 320 kbps was encoded and
independently decoded on November 15, 2026. Chromium 143, Firefox 144 and FFmpeg returned
43,201,152 frames (900.024 seconds), with 1,152 extra frames; the MP3 had
36,000,960 bytes. Encode-only elapsed times were approximately 41 seconds in
Chromium and 83 seconds in Firefox on this desktop synthetic fixture, excluding
decoding/verification and without a peak-memory measurement. The existing
900-second browser import validator correctly
rejected it, and the gapless normalizer left its unknown layout unchanged.
Consequently an MP3 exported at the exact duration boundary may not reimport
into this preview. No import limit was widened and no guessed trimming added.

## Dependency and source provenance

`lame-3.100-wme-0.7.0.wasm` is unmodified `package/wasm/mp3.wasm` from
`wasm-media-encoders` **0.7.0**. The package tarball's published SHA-512 integrity
was checked before extraction. `provenance.json` records the exact tarball,
source commits, source archive and SHA-256 hashes for all vendored inputs.

- Wrapper source commit: `4a45333baadbab312d1cc0911151dfad23157c51`
  in <https://github.com/arseneyr/wasm-media-encoders>.
- Its LAME submodule: `98db548e8e851defbba3184125ce10725355c332`
  in <https://github.com/arseneyr/lame>, reporting version **3.100**.
- Wrapper MIT notice: `MIT-wasm-media-encoders.txt`.
- LAME original license/notice files: `LGPL-2.0-LAME.txt`, `LICENSE-LAME.txt`,
  `README-LAME.txt`; preserve these notices separately from the project's MIT license.
- Exact LAME source archive: `lame-98db548-source.tar.gz`.
- Unmodified WASM build inputs from the wrapper commit: `source/`.

The package repository was unarchived and last pushed August 7, 2025 when checked
November 15, 2026; the pinned package release itself dates to May 24, 2024.
No claim of a new release or independent whole-code security audit is made.
Boundary review covered the wrapper's configuration/PCM/output ownership,
explicit sample rate/CBR, disabled decoder/frontend, version/MIME guards, and
failure handling. The published WASM was not rebuilt locally. No npm install,
bundler, application dependency rewrite, Ogg assets or CDN runtime is required.

To rebuild/modify the binary, use the upstream Makefile at the pinned wrapper
commit and the exact LAME source. The recorded recipe uses Emscripten 2.0.8,
`-Oz -flto`, disables decoder/frontend/shared/stdio, and exports only the graph
in `source/src/wasm/minify_graph.json`. The vendored source inputs are preserved
to make those settings inspectable; rebuilding byte-identical output is unverified.
Update the binary filename, adapter size/version checks and provenance together
when intentionally changing the encoder. The site builder copies this directory
and versions connected worker/module/WASM URLs as part of its existing graph.

The LAME license and its relinking/source conditions continue to apply to its
binary. This directory includes the exact source and wrapper build inputs;
the separately loaded `.wasm` file is replaceable, rather than hidden in a packed
application script. To rebuild/relink it, expand the included source archive at
`source/src/wasm/lame/lame-src`, activate the Emscripten SDK, then use the pinned
upstream Makefile's MP3-only target (`make wasm/mp3.wasm`) from `source/`.
The Makefile also expects its Node/Yarn/tsx build utilities; its exact
`source/package.json`, `source/yarn.lock` and TypeScript configuration are retained.
Alternatively
use a checkout of the complete pinned wrapper commit with its lockfile and the
included exact LAME revision. Distribute original notices and corresponding source
with any replacement. These are the upstream recorded build/license facts;
this work does not certify the project's complete distribution compliance.

## Focused checks

Run `python web/tools/verify_mp3_export.py --browser chromium --out output/mp3-export-chromium`
from the repository root; repeat with `firefox`/`webkit`. The fixture suite uses
only synthetic audio. It verifies actual worker encoding, all input forms,
immutable/consumed input contracts, four bitrates, attenuation, progress,
cancel/error recovery, malformed WAV, rate/channel/sample/duration validation,
short/silent audio, local GET-only requests, provenance hashes and independent
FFmpeg decode/ffprobe layout. Aligned channel SNR must exceed 25 dB on the tone
fixtures; this is a limited numerical fidelity check, not a listening score.
Add `--boundary` to encode and stream-decode an actual exact 15-minute fixture
and verify its unchanged import admission behavior. This intentionally uses a
345.6-MB float WAV and substantial browser memory; it is a desktop probe, not
mobile memory qualification.

Windows WebKit 26 lacks AudioContext in the installed Playwright build, so its
worker output can be FFmpeg-verified but its browser decode/AudioBuffer adapter
are explicitly `not_run`. This does not qualify Safari/macOS, mobile audio,
long-duration memory behavior or the parent's UI/decoded-peak integration.
