# Third-party notices

The project's original code, documentation and original bundled SlimeDragon artwork are MIT licensed. That notice does not relicense third-party components or assets.

## Native Rust release

Native releases include `THIRD-PARTY.json` and `licenses/`, generated from the locked Cargo dependency graph. Original license/notice files supplied by dependencies are retained. The inventory can include platform-specific dependencies not used in the Windows binary. Build/update it through the release packager rather than manually declaring every dependency to be MIT.

The runtime does not bundle the legacy Python virtual environment, PySide6/Qt or a Python interpreter. It does not bundle FFmpeg/ffprobe; those optional external programs retain the license of the distribution a user installs.

## Browser preview

The static browser build includes its own `THIRD-PARTY.json` and dependency `licenses/`, generated from the locked `web/dsp` Cargo graph. WebAssembly bindings use wasm-bindgen and the browser clock uses web-time, under their original licenses. The browser build does not bundle FFmpeg, external fonts, or analytics. The optional example recording keeps its CC0 credits. Original software/artwork and audio-output ownership follow the same terms as the native app.

## Optional Studio components

The MCP companion and isolated VST3 worker include separate locked Cargo inventories in `MCP-dependencies/` and `Plugin-dependencies/`. VST3 bindings use the `vst3` crate under its upstream MIT OR Apache-2.0 terms. User-installed audio effects retain their own licenses; the app does not distribute or relicense those effects.

Neural separation is an optional download, outside the installer. Setup installs an isolated Python environment with Spleeter 2.4.2, TensorFlow 2.12.1 and their dependencies, retaining the packages' supplied license files. Spleeter's code is MIT; TensorFlow's code is Apache-2.0. The upstream model archive is fetched from the Spleeter v1.4.0 release and checked against a pinned SHA-256. The extracted cache retains provenance and per-file hashes. Model files are third-party material and are not relicensed by this project's MIT notice. See the [Studio guide](docs/STUDIO.md) and the upstream release referenced in the installed provenance.

## Separately licensed content

| Content | License and attribution |
|---|---|
| Example recording/excerpt | Technological Messup by Bärghest, CC0 1.0 Universal. Source, changes and hashes: [example credits](examples/audio/CREDITS.md). |
| Numerical audio regression fixture | Cipher by Kevin MacLeod, CC BY 4.0; a 256-sample modified numerical excerpt used only in tests. See [fixture attribution](rust/tests/fixtures/README.md). |
| Installer stub | NSIS 3.13, under its original applicable notices; the installer uses zlib compression. Its COPYING notice is included under the installed `licenses/nsis-3.13/` folder. The compiler itself is not installed with the app. |
| SlimeDragon interface artwork | Original project assets generated from the creator's references; MIT for the project's rights in those assets. Provenance and scope: [asset manifest](rust/assets/ASSETS.md). Windows fonts are loaded at runtime, not redistributed. |

## Legacy Python source

The older Python source is retained as a separate reference implementation. Its historical environment inventory is preserved in [the legacy notices](docs/legacy/python-third-party-notices.md); it is not the dependency list of the native installer. Installing or distributing that environment requires retaining its actual packages' notices and applicable terms.

## Audio outputs

The project claims no ownership, royalties or mandatory app credit in users' audio. MIT attribution belongs with software copies, not music exports. Existing third-party rights are not erased by processing a recording. See [OUTPUT_OWNERSHIP.md](OUTPUT_OWNERSHIP.md).
