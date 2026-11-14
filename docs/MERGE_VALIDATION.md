# Studio merge validation and availability

This follow-up covers review fixes made after the [initial beta.3 larger-bank qualification](STUDIO_BETA3_VALIDATION.md). Desktop, MCP and browser versions remain **0.5.0-beta.3**, **0.2.0-beta.3** and **0.3.0-beta.3**. No new binary release is created by merging source.

## Review fixes

| Area | Behavior and regression coverage |
| --- | --- |
| Windows connection helper | Captures the companion's UTF-8 configuration correctly when package, library and output paths contain accented or CJK characters. Windows PowerShell 5.1 tests exercise code pages 437/932, exact JSON paths, encoding restoration, command failures and cancellation. Folder/save dialogs retain their authorization behavior; the generated JSON remains UTF-8 without a BOM. |
| Desktop pending faders | Voice cleanup, repair and plugin preview require the pending mix to be rendered first. Rejected edits retain both the previous audio and staged controls; export, project saving and mastering handoff remain blocked until the mix is applied. Update mix preview stays available on other Studio pages while faders are pending. Analysis can still inspect the selected audio. Regressions also render the changed mix, perform a later edit and verify consistent project controls. |
| Browser pending faders | Edit, history, preview export and mastering-promotion handlers preserve unapplied controls until **Stem mix → Update mix preview**. Shared mutation guards cover direct handler calls as well as disabled controls. Raw immutable-stem exports, original-source analysis and spectrogram inspection remain available. Tests cover cancellation, exact audio/history behavior and successful later edits/exports. |
| MCP project origin | Each preview retains its actual immutable origin and associated bank. Saving or opening a preview of recording A does not attach an unrelated bank from recording B, including when layouts or PCM happen to match. Handoff validates source, comparison and bank alignment before publishing a project. Source-only, bank-associated, chained, promoted and undo/redo cases exercise real project reopening. |

## Validation scope

- **117 Windows native tests passed** in the debug/test profile with default features plus `schemas`, including two new pending-mix regressions. Formatting and strict all-target Clippy passed.
- Both added processing regressions also passed on Linux with an intentionally unavailable ALSA output device. They require the expected completed revision, exact rendered PCM and consistent saved controls; only the specifically identified nonfatal playback-device notice is permitted. This does not qualify audible output.
- **Nine Windows PowerShell 5.1 connection-helper cases passed** against the actual companion; no GUI dialogs or personal files were used. The former helper reproduced path corruption under non-UTF-8 console encodings.
- **Chrome 156, Firefox 144 and Linux WebKit 26 each passed 25 Studio workflow groups**, with five additional pending-control groups and zero application errors. Test-tool screenshot diagnostics are recorded separately from application failures. Runtime-cache checks passed; the frozen static runtime is `9f53b4d1a651a506`, and the WASM bytes are unchanged by the browser UI fix.
- **15 Windows MCP Rust tests passed**, including six new origin regressions. Ten source-context cases exercised **120 actual native project loads** across explicit saves and desktop handoffs; two source-A projects also reopened in the desktop with preview/playback ready. Existing origin, handoff, Studio and large-bank checks passed. These checks retain source identity, omit unrelated banks from saved projects and preserve valid bank edits. Linux executes the same added matrix through required CI.

These results qualify the tested source and fixtures. Earlier installer, model/plugin, full-length recording and appearance results retain their original scope; they are not silently rerun by a source merge. Physical iPhone/Android playback and arbitrary native plugins remain separate qualification work. No personal recording is included in the repository or validation fixtures.

## Source, downloads and hosting

- The current repository source contains Studio and the optional 38-tool MCP companion. Use [Building from source](BUILDING.md) for this version.
- The stable Windows download remains **v0.4.2** and does not include Studio/MCP. Published **v0.5.0-beta.1** assets predate bulk/ZIP imports and the later fixes. Earlier downloaded local beta.3 candidates also retain the code from the commit used to build them; source fixes do not update an installed app automatically.
- A merge does not create a GitHub release, replace old installers or change music-output ownership. [Installation](INSTALLATION.md), [licensing](../LICENSE) and [output ownership](../OUTPUT_OWNERSHIP.md) remain explicit.
- Protected `main` requires successful **windows**, **linux** and **browser** checks and resolved review conversations. See the live [native checks](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/actions/workflows/ci.yml) and [browser checks](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/actions/workflows/web.yml) for current commits and artifacts.
- The browser workflow builds the static site from `main`, runs browser validation and then deploys to GitHub Pages. Source merging and successful deployment are separate events; the [browser guide](WEB.md) links the hosted app. Audio processing remains on the user's device.

## Reproduce the added checks

From the repository root:

```sh
cargo test --locked --manifest-path rust/Cargo.toml --lib pending_mix_tests
cargo test --locked --manifest-path mcp/server/Cargo.toml
python mcp/tools/verify_studio_origin.py --server PATH_TO_MATCHING_COMPANION
python web/tools/verify_studio_browser.py --out NEW_EVIDENCE_FOLDER
```

Use `--browser firefox` or `--browser webkit` for the other installed engines. The origin verifier builds its small native project-loader example when one is not supplied; use `--project-loader` to select a prebuilt matching loader.

The Windows-specific check uses **Windows PowerShell 5.1**, rather than PowerShell 7:

```powershell
powershell.exe -NoProfile -NonInteractive -STA -ExecutionPolicy Bypass -File mcp/tools/verify_connect_config.ps1 -Server PATH_TO_MATCHING_COMPANION -Evidence NEW_EVIDENCE_FOLDER
```
