# Optional private FFmpeg

Lookup never downloads or changes PATH. `locate("ffmpeg" | "ffprobe")` returns an
absolute prepared system executable first, then a receipt/hash-verified private
tool. System selection requires functional preparation in this process. Private
lookup accepts a valid installed smoke-test receipt and compiled per-file SHA-256
pins, including in fresh MCP/CLI processes that never called `ensure`. Lookup
never downloads or starts processes; hashes are cached against path, size and
modification time, so still keep integrity I/O out of GUI render loops.

`ensure(&dyn Fn(f32, &str), Arc<AtomicBool>) -> anyhow::Result<Runtime>` is the
explicit download entrypoint. `Runtime` exposes absolute `ffmpeg`, `ffprobe`, and
`private: bool`. `supported()` describes the compiled private-bundle target, not
whether an unrelated system FFmpeg is installed. The caller owns permission and
policy: GUI import/export/Studio jobs may prepare tools; MCP background jobs must
not call `ensure` without their explicit owner policy.

`Paths::user_cache() -> Result<Paths>` selects
`%LOCALAPPDATA%/slimeybeats-ez-master/optional-tools/ffmpeg` on Windows, or
`$XDG_CACHE_HOME/slimeybeats-ez-master/optional-tools/ffmpeg` on Linux, falling back
to `$HOME/.cache`. Environment roots must be absolute. `Paths::with_root(root)`
supports fixtures and explicit overrides. It has `root()`, `locate`, `available`,
and `ensure`. These instance methods operate on this private root, so a custom
root never selects system tools; the global API retains system preference.
A private root must be empty or carry the exact ownership marker;
relative paths, parent traversal, symlinks and Windows reparse points are rejected.
The default root uses the shared `crate::paths::cache_root()` policy.

## Files and publication

- `pins.rs`: fixed upstream URLs, archive and extracted-file SHA-256 pins.
- `paths.rs`, `cache.rs`: private paths, ownership, cancellable OS file locks,
  bounded receipts and readiness checks.
- `install.rs`: shared verified downloader, staging, smoke tests and publication.
- `archive.rs`: reusable hash-first fixed-entry ZIP extraction, streaming metadata
  validation, CRC/size checks and output budgets.
- `tar.rs`: strict regular-file/directory ustar reader for the Linux bundle.
- `probes.rs`: direct hidden `Command` construction, shared bounded supervision,
  cached banners and prepared system selection.
- `smoke.rs`, `readiness.rs`: tiny real FLAC/libmp3lame MP3 encodes, ffprobe JSON
  metadata and PCM decoding, with a bounded positive/negative pair cache.
- `tests.rs`, `archive_tests.rs`, `readiness_tests.rs`: offline failure, safety,
  publication and format-readiness fixtures;
  opt-in tests exercise downloaded upstream bundles without network access.

The shared `crate::downloads::fetch_verified` writes only into a unique staging
directory. Archive hashes are checked again before parsing or starting `xz`, and
compiled per-file hashes are checked before any FFmpeg execution. Extract only
the exact two binaries and `LICENSE.txt`; never install ffplay, shared libraries,
plugins or arbitrary archive paths. ZIP budgets are 192 MiB compressed, 256
entries, 192 MiB per member and 640 MiB summed advertised output including skipped
members. Linux's compressed cap is 160 MiB; system `xz` has a 256 MiB decompression
memory limit, 120-second deadline and 640 MiB stdout cap. Its output goes to a
temporary tar file, then the strict reader writes the three fixed files.

After FLAC and MP3 encoding, ffprobe JSON metadata, and PCM decode checks succeed,
a unique directory moves into `installs/`. An fsynced temporary `ready.json` replaces the
previous receipt atomically as the final commit point. Cancellation/errors before
that point preserve the previous receipt and files. OS locks release on process
exit; abandoned staging is never readiness. Existing installations are retained;
this module intentionally has no old-install garbage collector. Future pin
updates should retain accepted historical pins if old runtimes must remain
discoverable across application versions. No updater, GUI or helper-recovery
policy is owned here.

Concurrent setup waits for the OS file lock for at most 120 seconds, polls every
100 ms, and honors cancellation. Actual contention emits
"Another window is preparing codecs; waiting..." once, without failing the job
immediately. After acquiring the lock, setup rechecks the receipt so the second
worker reuses the first worker's completed installation.

System preference requires both formats to work; a version banner alone never
approves a runtime. A custom system build missing `flac` or `libmp3lame` is cached
as unusable until either executable changes, so subsequent lookup cannot override
a prepared private copy with that failed system pair. PATH changes invalidate
system selection. Explicit `ensure` functionally checks an existing private
install once per process; read-only lookup relies on its receipt and pinned file
integrity. Receipts record both encoders.
The smoke test uses a 100 ms mono PCM WAV, avoiding any `lavfi` dependency. Each
child has a 10-second deadline and explicit output caps; readiness caches hold at
most 16 pairs, and hash/banner caches hold at most 64 checks.

## Provenance (verified November 14, 2026)

FFmpeg's official download page links BtbN/FFmpeg-Builds, now redirected to the
FFmpeg-Builds organization. These are provider builds, not binaries produced by
the FFmpeg project. The selected monthly release is
`autobuild-2026-10-31-12-59`; provider recipes commit:
`5031c6f1082414e53688d39e521849905071070d`. FFmpeg source commit:
`8dca1586f468bfcbb78ae3ed618af1361d9f140a` (release branch 8.1).

Base asset URL:
`https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/autobuild-2026-10-31-12-59/`

| Target | Exact asset | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| Windows x64 | `ffmpeg-n8.1.3-27-g8dca1586f4-win64-lgpl-8.1.zip` | 167778744 | `d4c2a42673917b4a97104a3e9f082729a9730796b98664e37c0b7eba6c6c613d` |
| Linux x64 GNU | `ffmpeg-n8.1.3-27-g8dca1586f4-linux64-lgpl-8.1.tar.xz` | 135188284 | `a64f3171e6ad1f7d358d28890a907ee478c331d1177ec696a93418f79ac3f1f3` |

Both archives were downloaded to ignored test storage; computed hashes matched
GitHub's asset digests. Exact binary/license hashes are compiled in `pins.rs` and
were computed from those verified archives. GitHub reports `immutable: false`:
fixed URLs avoid rolling `latest`, and SHA-256 pins enforce content identity even
if assets change. BtbN retains the last monthly build for two years; removal or
replacement must fail visibly rather than switching to an unpinned asset.

The selected LGPL variants omit GPL-only/nonfree dependency variants. Both carry
the GNU LGPL version 3 text as `LICENSE.txt`; LGPL v3 incorporates GPL v3. The
installation includes this original license and `NOTICE.txt` with asset, FFmpeg
source, recipe/dependency source and license URLs. Private downloading does not
change this application's own license. Consult upstream component licenses
before redistributing binaries.

Provider requirements: Windows 10 22H2 minimum supported; Linux glibc >= 2.28 and
kernel >= 4.18. Linux automatic setup also requires existing system `xz`, which is
never installed by this module. Windows execution and both actual archives were
tested; native Linux execution requires separate Linux qualification.

For a downloaded native bundle, set `SLIMEYBEATS_FFMPEG_TEST_ARCHIVE` to its
absolute path, then run `cargo test --lib --no-default-features
optional_tools::tests::live_native_bundle -- --ignored`. The same test selects the
Windows or GNU Linux pin on its native target, uses a temporary custom cache,
performs encode/probe/decode, and verifies reuse without network access.

## Reusable archive API

`archive::validate_cache_path(&Path) -> Result<()>` checks absolute paths and
existing symlink/reparse-point ancestors without mutation or ownership adoption.

`archive::extract_zip_fixed(archive: &Path, sha256: &str, destination: &Path,
entries: &[FixedEntry<'_>], limits: Limits, cancel: &AtomicBool) -> Result<()>`
checks the archive hash before parsing, validates all member metadata before
writing any selected file, and creates only mapped files in a fresh empty staging
directory. `FixedEntry { archive_path, file_name }` maps an exact archive path to
a plain fixed output name. `Limits` fields are `max_archive_bytes: u64`,
`max_entries: usize`, `max_entry_bytes: u64`, and `max_total_bytes: u64`.
The caller discards staging on failure and verifies component-specific binary
hashes before execution. This supports official same-application companions;
it is not a third-party VST download facility.
