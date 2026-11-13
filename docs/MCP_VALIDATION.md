# MCP preview validation

The companion is a development preview. Validation targets the real stdio transport and shared desktop DSP; it does not establish that every AI client is configured correctly or that an assistant can hear audio from measurements.

## Local results — November 13, 2026

| Check | Observed result |
|---|---|
| Native regression suite | 82 Windows / 84 Ubuntu Linux tests passed; format, Clippy with warnings denied and release builds passed |
| Companion regressions | 8 Windows / 9 Linux tests passed, including Unix symlink escape rejection and cancellation/shutdown checkpoints |
| Actual stdio client | All 16 tools discoverable; initialization, schemas, embedded help topics and matching text/structured results passed |
| Full processing/export | Synthetic stereo passed on Windows/Linux; 45-second credited CC0 music passed on Windows, including six formats, independent FFmpeg loudness/peak checks, unchanged source and non-clobber names |
| Desktop handoff | Windows launched the trusted desktop executable, restored settings, prepared the EQ preview and rejected changed source content; diagnostic state reported no error and a ready playback source |
| Browser isolation | WASM binding lint/compile passed; MCP dependencies stay outside the browser and GUI dependency graphs |
| Packaging/setup | Clean public-source allowlist includes MCP source/help/tests/tools; configuration example works with paths containing spaces; Windows helper passes PowerShell syntax parsing |

GUI readiness is a technical check, not a listening-quality verdict. Physical listening remains a user acceptance step. The source suite includes the existing native live-audio tests; MCP EQ requests deliberately render background revisions.

## Reproduce

```sh
cargo fmt --manifest-path mcp/server/Cargo.toml -- --check
cargo clippy --locked --manifest-path mcp/server/Cargo.toml --all-targets -- -D warnings
cargo test --locked --release --manifest-path mcp/server/Cargo.toml --all-targets
cargo build --locked --release --manifest-path mcp/server/Cargo.toml
python mcp/tools/verify_mcp.py --server mcp/server/target/release/slimeybeats-mcp --codecs
```

Use `.exe` on Windows and `python3` where needed on Linux. The decoder-monitor regression requires Python on PATH. `--codecs` requires FFmpeg/ffprobe with MP3/FLAC encoding. Add `--input examples/audio/demo-input.wav` to run the credited CC0 music fixture instead of the deterministic synthetic stereo signal.

The protocol validator uses an isolated temporary folder containing spaces. It initializes a real server subprocess, discovers tool schemas, exercises jobs and checks source integrity, output collisions, stale IDs, malformed imports, invalid options, root traversal, deterministic suggestions, repeated EQ, preview cleanup and final decoded peak measurements. It confirms that Master creates only a temporary preview and that final audio survives Clear/shutdown. Separate Rust regressions cover job invalidation, overlap rejection, cancellation, late durable commits, bounded history, input budgets, Unix symlink escapes and a decoder child that closes stdout before hanging.

The core suite compares the shared preview's samples with the existing engine/manual-EQ chain, verifies repeated EQ does not accumulate, checks session fingerprints and rejects an export cancelled immediately before final persistence. Native GUI/CLI and WASM checks remain required; MCP is not linked into the browser.

## Remaining acceptance

Configure at least one actual AI client using the generated example and try a conversational analyze/master/audition/export session. Confirm physical audio playback on the intended desktop output. The protocol harness is a real MCP client for automated tests, but it is not certification of every desktop AI host or model's tool-use quality.

Forced-process-termination cleanup, adversarial same-user filesystem races and a universal long-track memory maximum are outside this qualification. Export codec calls retain the desktop exporter's cancellation boundaries. See [limits and privacy](MCP.md#limits-and-privacy).
