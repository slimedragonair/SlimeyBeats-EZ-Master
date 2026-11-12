# Contributing

Bug reports, documentation, tests, accessibility improvements and focused code contributions are welcome.

For a bug, describe the version, Windows/audio-device setup, exact steps, expected result and observed result. Include a minimal settings/report excerpt if useful. Remove private paths and personal material. Prefer a tiny synthetic or clearly licensed reproduction to uploading somebody else's recording.

The native app lives in `rust/`; shared built-in profiles are in `mastercraft/data/`. The older Python implementation is separate. Keep modules focused, follow nearby naming and preserve unrelated work. Rendering, analysis, processing, export and UI state have distinct owners; see [architecture](rust/docs/ARCHITECTURE.md).

Before proposing a Rust change:

```powershell
cd rust
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --release --all-targets
```

For DSP changes, add an observable regression and independently check decoded exports where relevant. State whether a timing includes decoding/export. A passing test or meter does not establish better sound. For UI changes, include actual empty/loaded and small-window captures.

Contributions are offered under the project's MIT license unless explicitly identified as a separately licensed asset. You keep your copyright. Do not submit code, recordings, samples, artwork or other material you lack permission to share. Preserve third-party attribution and license notices.
