# Build and package

The current release is the native Rust application. The older Python implementation has a [separate historical guide](legacy/python-guide.md).

For the Ubuntu source-build and validation commands, see [Linux checks](LINUX.md). The published v0.4.2 installer is for Windows.

## Native development

Use a current stable Rust MSVC toolchain, Microsoft C++ Build Tools/Windows SDK, and Git. From the repository root:

```powershell
cd rust
cargo build --release --bins
cargo test --release --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

Run `target/release/slimeybeats-ez-master-live.exe` or the batch CLI `target/release/slimeybeats-ez-master-cli.exe --help`. Built-in profiles compile from the shared `mastercraft/data/builtin_profiles.json`; preserve that relative source layout.

## Release artifacts

Packaging requires Python 3.11+ and the already built release binaries. No Python packages are needed to assemble a release. The committed example WAVs/screenshots/icon are reused; regenerating those assets is a separate development step.

```powershell
python rust/tools/build_release.py --out dist
```

This creates a portable ZIP/folder, a clean GitHub source ZIP/folder, and a Windows Setup.exe. Output destinations must be new; earlier releases are not overwritten. The tool downloads a pinned portable NSIS 3.13 compiler from its official SourceForge project, verifies its SHA-256 and keeps it under `rust/target/release-tools/`. An existing compiler can be passed with `--nsis PATH-TO-makensis.exe`.

The installer is per-user, uses HKCU registration and does not require administrator rights. Program-file removal is generated from its actual payload. It does not recursively delete the installation root or the user's settings, exports, custom profiles or Examples folder. See [release verification](RELEASING.md).

The license inventory resolves locked Cargo metadata for all platforms, including packages a Windows build may not have cached. First-time packaging therefore needs network access. When packaging the original `v0.4.2` tag, prefetch these packages with `cargo fetch --locked --manifest-path rust/Cargo.toml`; current tooling fetches missing metadata automatically and reports its errors.

FFmpeg is not bundled or silently installed. It is optional at application runtime for compressed formats/resampling and is needed by the separate export-validation tool:

```powershell
python rust/tools/verify_exports.py --input YOUR-STEREO-WAV
```

## Public source boundary

`rust/tools/release_packaging/public_source.py` owns the clean source allowlist. It includes native source/tests, the shared profiles, reference Python source, selected guides, licensed assets and the published example files. It excludes `.git`, virtual environments, build output, personal settings, private tests/reports, recordings and unrelated scratch folders. Use the clean source export for a new public repository rather than archiving the development directory.
