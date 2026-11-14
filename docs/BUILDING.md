# Build and package

The native Rust application is the maintained desktop. Current source targets Studio **0.5.0-beta.3**, optional MCP companion **0.2.0-beta.3** and browser Studio **0.3.0-beta.3**; beta.3 binaries have not been publicly released. Stable v0.4.2 remains separate. The older Python implementation has a [separate historical guide](legacy/python-guide.md).

For the Ubuntu source-build and validation commands, see [Linux checks](LINUX.md). The published v0.4.2 installer is for Windows.

## Native development

Use a current stable Rust MSVC toolchain, Microsoft C++ Build Tools/Windows SDK, and Git. From the repository root:

```powershell
cargo build --locked --release --bins --manifest-path rust/Cargo.toml
cargo test --locked --release --all-targets --manifest-path rust/Cargo.toml
cargo clippy --locked --all-targets --manifest-path rust/Cargo.toml -- -D warnings
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
```

Run `rust/target/release/slimeybeats-ez-master-live.exe` or the batch CLI `rust/target/release/slimeybeats-ez-master-cli.exe --help` from the repository root. Built-in profiles compile from the shared `mastercraft/data/builtin_profiles.json`; preserve that relative source layout.

## Release artifacts

Packaging requires Python 3.11+ and the already built release binaries. No Python packages are needed to assemble a release. The committed example WAVs/screenshots/icon are reused; regenerating those assets is a separate development step.

```powershell
cargo build --locked --release --manifest-path mcp/server/Cargo.toml
cargo build --locked --release --manifest-path plugins/host/Cargo.toml
python rust/tools/build_release.py --out dist
```

This creates a portable ZIP/folder, a clean GitHub source ZIP/folder, and a Windows Setup.exe. Output destinations must be new; earlier releases are not overwritten. The tool downloads a pinned portable NSIS 3.13 compiler from its official SourceForge project, verifies its SHA-256 and keeps it under `rust/target/release-tools/`. An existing compiler can be passed with `--nsis PATH-TO-makensis.exe`.

The installer is per-user, uses HKCU registration and does not require administrator rights. Program-file removal is generated from its actual payload. It does not recursively delete the installation root or the user's settings, exports, custom profiles or Examples folder. See [release verification](RELEASING.md).

The full Studio distribution includes the MCP companion (38 tools, optional to use), isolated VST3 worker, connection helper and separate dependency inventories. Neural model/runtime downloads remain optional and outside the installer. Imported banks need no model and have no fixed count cap in beta.3; neural extraction remains two/four stems. Native defaults are 512 MiB decoded PCM per input and a separate 4 GiB decoded-bank budget; browser Studio retains 128 MiB combined and 15 minutes per track. Matching rate/channels/frame count and additional working memory are still required; see [Studio limits](STUDIO.md). Prerelease Cargo versions use the **Studio Preview** installation identity, with separate program folders, registry entries and shortcuts from stable builds. Windows numeric version resources are derived from the numeric core of the SemVer version.

The license inventory resolves locked Cargo metadata for all platforms, including packages a Windows build may not have cached. First-time packaging therefore needs network access. When packaging the original `v0.4.2` tag, prefetch these packages with `cargo fetch --locked --manifest-path rust/Cargo.toml`; current tooling fetches missing metadata automatically and reports its errors.

FFmpeg is not bundled or silently installed. It is optional at application runtime for compressed formats/resampling and is needed by the separate export-validation tool:

```powershell
python rust/tools/verify_exports.py --input YOUR-STEREO-WAV
```

## MCP companion preview

Build the desktop, MCP and VST3 worker as above. On Windows, `python mcp/tools/package_companion.py --out dist/MCP` produces a separate companion ZIP containing the server, matching desktop, worker, folder-selection setup helper, docs, CC0 input example and dependency notices. Destinations must be new. This does not modify or publish the v0.4.2 installer. See [MCP setup and privacy](MCP.md) and [validation](MCP_VALIDATION.md).

## Public source boundary

`rust/tools/release_packaging/public_source.py` owns the clean source allowlist. It includes native source/tests, the shared profiles, reference Python source, selected guides, licensed assets and the published example files. It excludes `.git`, virtual environments, build output, personal settings, private tests/reports, recordings and unrelated scratch folders. Use the clean source export for a new public repository rather than archiving the development directory.

## Browser preview

See [WEB.md](WEB.md) for the locked Rust/WebAssembly build, local static-server command, privacy model, browser checks and GitHub Pages workflow. The browser build needs no server DSP or FFmpeg at runtime.
