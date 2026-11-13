# Linux validation and source builds

Linux checks target **Ubuntu 24.04 x86_64**. The published v0.4.2 installer remains Windows-only; Linux build/render/export validation is separate from a packaged Linux release and physical audio-device qualification.

## Build dependencies

Install a current stable Rust toolchain and these Ubuntu packages:

```bash
sudo apt-get update
sudo apt-get install --no-install-recommends build-essential pkg-config libasound2-dev libdbus-1-dev libxkbcommon-dev libwayland-dev libxcb-shape0-dev libxcb-xfixes0-dev libgl1-mesa-dev libudev-dev fonts-dejavu-core xdg-utils
```

From the repository root:

```bash
cd rust
cargo build --locked --release --bins
./target/release/slimeybeats-ez-master-cli --help
./target/release/slimeybeats-ez-master-live
```

WAV input/export is native. Install `ffmpeg` for compressed inputs, FLAC/MP3 exports and resampling. The desktop needs a graphical session, a working OpenGL implementation, and an audio output service/device for listening. File selection uses the desktop's XDG portal; install the appropriate `xdg-desktop-portal` backend for your desktop. A headless server can run the CLI without a display or sound device. ALSA/PipeWire/PulseAudio setup is a property of that host, not proof supplied by a passing CLI test.

## User files and desktop integration

- Settings: `$XDG_CONFIG_HOME/slimeybeats-ez-master/rust-settings.json`, defaulting to `~/.config/slimeybeats-ez-master/rust-settings.json`.
- GUI data/profiles/default exports: `$XDG_DATA_HOME/slimeybeats-ez-master/`, defaulting to `~/.local/share/slimeybeats-ez-master/`. Exports go in its `output/` folder unless you choose another location. The CLI retains its explicit `--out`/working-directory behavior.
- Relative XDG environment values are ignored. Absolute overrides are useful for isolated tests. If no absolute XDG/HOME location exists, the legacy executable/development-root fallback remains available and filesystem errors remain visible.
- Open folder uses `xdg-open` with one literal absolute path. DejaVu Sans/Serif are loaded from installed Linux fonts; no Windows fonts are redistributed.

## Repeatable checks

```bash
cd rust
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --release --all-targets
sudo apt-get install --no-install-recommends ffmpeg xvfb xauth
xvfb-run -a python3 tools/verify_linux.py --gui target/release/slimeybeats-ez-master-live --out /tmp/slimeybeats-new-evidence
```

Use a new evidence directory. The verifier checks native WAV with no FFmpeg on PATH, unchanged native duration/rate/channels, six independently metered formats, actual 48 kHz → 44.1 kHz conversion, source preservation, collisions and errors. GUI captures exercise welcome, default EQ and compact EQ, isolated XDG paths and preview without audio export. The hidden capture-only settings flag invokes the same explicit Save settings action used by the app; settings are not assumed to autosave on exit. Software-rendered captures do not establish physical playback, portal dialogs, every distribution or device latency. Those limits are explicit in the JSON report.

The `linux` job in Native checks performs these commands on Ubuntu. Keep its artifacts with any platform-specific findings; do not replace a failing measurement with a looser limit to obtain a pass.
