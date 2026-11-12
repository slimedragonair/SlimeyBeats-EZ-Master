# Historical Python dependency notices

This preserved inventory describes the separate development Python environment on its stated date. It is not the native Rust installer's dependency inventory or a statement that these Python packages are shipped in the native release. Current native releases include their own Cargo-derived notices and license files.

Master-Craft's own code is pure Python. It does **not** import or bundle `pedalboard`, `matchering`, Essentia, spleeter or demucs.
All DSP (EQ, crossovers, dynamics, de-esser, saturation, M/S, true-peak limiter) is written on numpy/scipy.

This table lists every distribution installed in the app's private `.venv`, as read from the package metadata on 2026-11-11 with `tools_licenses.py`. It includes transitive dependencies.
**Flags:** ✅ means permissive. ⚠️ means weak copyleft (LGPL/MPL): OK when dynamically linked and unmodified, but you must meet the conditions below. Nothing in the venv is strong-copyleft-only.

| Package | Version | License | Why it is here | Flag |
|---|---|---|---|---|
| numpy | 2.5.4 | BSD-3-Clause (+0BSD, MIT, Zlib, CC0 parts) | core | ✅ |
| scipy | 1.18.1 | BSD-3-Clause | core DSP | ✅ |
| soundfile | 0.14.0 | BSD-3-Clause | audio I/O wrapper | ✅ |
| └ libsndfile (bundled `_soundfile_data/libsndfile_x64.dll`) | 1.2.2 | **LGPL-2.1-or-later** (bundles LAME, mpg123, FLAC, Ogg/Vorbis/Opus) | WAV/FLAC/MP3 read + write | ⚠️ |
| librosa | 1.0.0 | ISC | analysis helpers | ✅ |
| pyloudnorm | 0.2.0 | MIT | loudness cross-check | ✅ |
| noisereduce | 3.0.3 | MIT | HF hiss reduction | ✅ |
| PySide6 / PySide6_Essentials / PySide6_Addons | 6.12.0 | **LGPL-3.0-only** OR GPL-2.0/3.0 (Qt for Python) | GUI | ⚠️ |
| PySide6_Pdf, PySide6_WebEngine | 6.12.0.140 | LGPL-3.0-only OR GPL | pulled in by the PySide6 meta-package, **unused** | ⚠️ |
| shiboken6 | 6.12.0 | LGPL-3.0-only OR GPL | PySide6 binding runtime | ⚠️ |
| └ FFmpeg DLLs in PySide6 (avcodec-63, avformat-63, avutil-61, swresample-7, swscale-10) | Qt build | **LGPL-2.1-or-later** (Qt ships an LGPL configuration) | QtMultimedia playback backend (A/B player) | ⚠️ |
| soxr (python-soxr, wraps libsoxr) | 1.1.0 | **LGPL-2.1-or-later** | optional high-quality resampling (falls back to scipy `resample_poly`) | ⚠️ |
| certifi | 2026.10.23 | **MPL-2.0** | transitive (requests ← pooch ← librosa) | ⚠️ (file-level copyleft; unmodified use is fine) |
| tqdm | 4.70.1 | **MPL-2.0 AND MIT** | transitive | ⚠️ (as above) |
| numba | 0.68.1 | BSD-2-Clause | transitive (librosa); also used optionally to speed up the compressor | ✅ |
| llvmlite | 0.50.0 | BSD-2-Clause AND Apache-2.0 WITH LLVM-exception | transitive (numba) | ✅ |
| scikit-learn | 1.9.1 | BSD-3-Clause | transitive (librosa) | ✅ |
| joblib | 1.6.0 | BSD-3-Clause | transitive | ✅ |
| threadpoolctl | 3.8.0 | BSD-3-Clause | transitive | ✅ |
| matplotlib | 3.11.2 | PSF-style matplotlib license (permissive) | transitive (noisereduce); **not imported by Master-Craft** | ✅ |
| contourpy | 1.4.0 | BSD-3-Clause | transitive (matplotlib) | ✅ |
| cycler | 0.12.1 | BSD-3-Clause | transitive | ✅ |
| kiwisolver | 1.5.1 | BSD-3-Clause | transitive | ✅ |
| fonttools | 4.68.0 | MIT | transitive | ✅ |
| pillow | 13.0.0 | MIT-CMU (HPND) | transitive | ✅ |
| pyparsing | 3.3.3 | MIT | transitive | ✅ |
| python-dateutil | 4.0.0 | BSD-3-Clause / Apache-2.0 | transitive | ✅ |
| packaging | 26.3 | Apache-2.0 OR BSD-2-Clause | transitive | ✅ |
| pooch | 1.9.0 | BSD-3-Clause | transitive (librosa) | ✅ |
| requests | 2.34.2 | Apache-2.0 | transitive | ✅ |
| urllib3 | 2.8.0 | MIT | transitive | ✅ |
| idna | 3.20 | BSD-3-Clause | transitive | ✅ |
| charset-normalizer | 3.5.2 | MIT | transitive | ✅ |
| platformdirs | 4.14.5 | MIT | transitive | ✅ |
| lazy-loader | 0.6 | BSD-3-Clause | transitive | ✅ |
| decorator | 5.3.1 | BSD-2-Clause | transitive | ✅ |
| msgpack | 1.2.4 | Apache-2.0 | transitive | ✅ |
| cloudpickle | 3.1.2 | BSD-3-Clause | transitive | ✅ |
| narwhals | 2.28.0 | MIT | transitive | ✅ |
| cffi | 2.1.1 | MIT-0 | transitive (soundfile) | ✅ |
| pycparser | 3.12 | BSD-3-Clause | transitive | ✅ |
| colorama | 0.4.6 | BSD-3-Clause | transitive | ✅ |
| typing_extensions | 4.16.0 | PSF-2.0 | transitive | ✅ |
| tzdata | 2026.6 | Apache-2.0 | transitive | ✅ |
| pip | 25.0.1 | MIT | venv tooling, not a runtime dependency | ✅ |

Not in the venv: **pedalboard (GPL-3.0) has been uninstalled.** matchering (GPL-3.0) was never installed.

## Obligations when distributing

### Qt / PySide6 / shiboken6 (LGPL-3.0)
- Use PySide6/Qt **unmodified** and **dynamically linked**: ship the original `PySide6`, `shiboken6` and Qt DLLs as separate files. Do not statically link them or merge them into one-file executables that stop users from swapping them.
- Users must be able to **replace** the Qt/PySide6 libraries with their own compatible build. With a venv- or folder-based distribution, they replace the `PySide6` folder.
- Include the LGPL-3.0 and GPL-3.0 texts, which are already in `PySide6/` and `shiboken6/` as LICENSE files. Say that the app uses Qt under the LGPL, and link to the Qt source (https://code.qt.io, https://download.qt.io/official_releases/QtForPython/).
- Do not forbid reverse engineering for the purpose of debugging those modifications in your EULA (LGPL §4/§6 equivalent).
- You are not using the commercial Qt licence, so do not use the GPL-only Qt modules. None are used: the app needs only QtCore, QtGui, QtWidgets and QtMultimedia.
- Optional: to shrink the distribution, remove the unused `PySide6_Addons` / `PySide6_WebEngine` / `PySide6_Pdf` parts by depending on `PySide6_Essentials` (+ the QtMultimedia part of Addons). Verify the QtMultimedia location before doing this.

### FFmpeg inside PySide6 (LGPL-2.1+)
- It ships as unmodified DLLs from Qt's LGPL build. The same rules apply: keep them as separate replaceable DLLs, include the LGPL-2.1 text, and point to the FFmpeg source (https://ffmpeg.org) and Qt's build configuration.

### libsndfile 1.2.2 (LGPL-2.1-or-later), bundled with `soundfile`
- It is a dynamically loaded `libsndfile_x64.dll`. Keep it a separate, replaceable file, include `_soundfile_data/COPYING` (LGPL-2.1), and point to the source at https://github.com/libsndfile/libsndfile.
- This libsndfile build provides MP3 encode/decode through **LAME** (LGPL-2.0+) and **mpg123** (LGPL-2.1). Both are LGPL, not GPL. MP3 patents have expired, so no patent licence is needed.

### python-soxr / libsoxr (LGPL-2.1-or-later)
- It is a dynamically loaded extension (`soxr_ext.pyd`). Keep it replaceable and include the LGPL-2.1 text. It is optional: if `soxr` is absent, Master-Craft falls back to scipy.

### certifi, tqdm (MPL-2.0)
- These are used unmodified. MPL obligations apply only to modified MPL files: keep their licence files and offer the source of any MPL file you change.

### Permissive packages (BSD/MIT/ISC/Apache/PSF)
- Keep each package's copyright notice and licence text in the distribution. They are inside each `*.dist-info/` folder (`LICENSE*`). Apache-2.0 packages also need their NOTICE files kept if present.

### Not bundled
- **ffmpeg.exe** is used only as a last-resort *decoder* for formats libsndfile cannot read (e.g. m4a/aac/wma). Master-Craft calls it only if the user already has `ffmpeg` on PATH. It is a separate program run as a subprocess, so it is not linked or distributed.

### Algorithms and reference notes (no third-party code)
- The punch detector, transient protection, soft clipper and two-stage limiter release (2026-11 transient fix) were written from scratch in numpy/scipy. No new dependency was added.
- `docs/ref/mc_reference_dsp.py` contains the researcher's from-scratch sketches. It is documentation, not imported by the app.
- GPL/AGPL projects named in `docs/research2_eq_automastering.md` (matchering, pedalboard, AQUA-Tk, Essentia) are cited as ideas only. None of their code is used, copied or translated.

## Project licence
**Not chosen yet.** Master-Craft's own source has no LICENSE file because none was specified. The owner must pick one (e.g. MIT, Apache-2.0, or proprietary) before distribution. All dependencies above are compatible with a proprietary or permissive licence as long as the LGPL/MPL conditions are met.
