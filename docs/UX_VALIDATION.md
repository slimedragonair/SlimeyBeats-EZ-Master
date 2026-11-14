# UI and learning validation — November 14, 2026

This record qualifies the unmerged UI/learning preview. It does not update published Windows packages or the public browser deployment. Desktop, MCP and browser source still identify as 0.5.0-beta.3, 0.2.0-beta.3 and 0.3.0-beta.3. Hashes distinguish the tested builds.

## Final candidate

Native runtime source: 8f9f7c2b55ec5bbb5d5c457f71623d4bf09375f9. Later documentation and packaging edits do not change that runtime source.

| Artifact | SHA-256 / runtime |
|---|---|
| Windows GUI | b25b1c84d33bb2dc28028b5b5e8e76a17a4f69f5adeaabdbb5f083f04f145d1e |
| Linux GUI | 7e362ecb663797839b7b6f9332ee8a455937aeeba57eb9d7e26ff91d41201b55 |
| MCP companion, Windows | 1f8c76df2f88270659cd23644ae3dad736f14dbc06909fe4b39a88f5c2b6b8fd |
| VST3 worker, Windows | 689da6490837104453bece67d9024932e641a8471d4a60826082a60fb402f4b8 |
| Final browser runtime | a2f5011426f04809 |
| Browser WASM | 45e3cbc665dba9fa096dc35293db038026bd5bda0caa39b83dd3723f379af060 |

The independently scored browser runtime was b71aebbf97338c92. Comparing all 47 runtime files after removing cache query strings found exactly one content difference in the final build: a removed trailing blank line in learning-content.js. WASM is byte-identical. Final-build smoke and viewport checks are recorded separately from that earlier grade.

## Automated and renderer checks

| Scope | Result |
|---|---|
| Windows native | 171 tests across library and integration targets; formatting, strict all-target Clippy and optimized app/CLI/companion/worker builds passed. |
| Linux native | 173 tests, strict all-target Clippy and optimized GUI/CLI builds passed. The final qualification used verified source copies and non-incremental compilation. |
| Windows renderer | 50 fresh states at 1100×720 and 1480×940, Basic/Advanced: learning, Help, Master/EQ, Studio mix/voice/repair/plugins, spectrogram, import and long names. All state sidecars reported no application error. |
| Linux renderer | 18 final isolated X11/software-OpenGL states at both native sizes, with platform fonts and an ALSA null sink; all state sidecars reported no application error. This is synthetic output, not hardware playback. |
| Browser learning | Reviewed v3 passed 20 groups each in Linux Chromium 143, Firefox 144 and WebKit 26 with actual Web Audio. The final build passed 20 Chromium groups, eight layout cases, Help/search and session preservation, with one runtime version and no application errors. |
| Additional browser viewports | Independent final-build checks passed 568×320 and 844×390 landscape, 768×1024 and 820×1180 tablet, and 980×720 desktop at normal and root-200% text, using ordinary input. All 354 static files remained unchanged. |
| Source and offline documentation | Guides, images and canonical JSON are included by explicit source/runtime packaging lists. Release verification scans every shipped guide's local targets, original dependency notices and the source audio allowlist. |

Native totals above use the ordinary test profile. Optimized binaries were built separately. Hosted release-profile CI and installer lifecycle results are separate qualification; a local test total is not a hosted-CI result.

The checks retain exact high-precision settings across untouched rendering, Basic/Advanced and page changes. They cover terminal tutorial choices against stale windows/tabs, damaged/unavailable storage, partial write failures and manual replay. Pending mix edits and changed mastering directions remain distinct from the prepared preview.

## Independent appearance and input

The final native images independently scored **9/10** for readability, task clarity, layout/usability and professional polish at both native sizes. Eight final images were inspected directly, including the corrected Plugins state, revealed spectrogram, compact introduction, EQ, Mix and Voice. These grades qualify those inspected states, not every image or possible plugin.

The earlier f1 native review independently completed the eight-step tour and an explicit decline in fresh profiles, verified suppression after immediate restart, exercised regular/compact Voice preview, muted comparison and separate export, and checked unique accessible gain names and final-part editing. Actual compact client size was measured at 1100×720 logical pixels, 1375×900 physical pixels at 120 DPI.

Additional parent checks closed compact graph opening/automatic reveal, Help Close/Escape focus return and keyboard reopening, Analyze → Master → typed EQ → explicit WAV export, Repair region editing/visualization/preview/muted comparison/export, compact final-part edits and pending guards, optional plots, and MCP folder selection/generation/copy. The copied Windows clipboard JSON matched the same companion's independent print-config output with equivalent canonical folder and desktop-executable arguments. No AI-client configuration was changed or server connection started.

The final native candidate additionally verified the new spectrogram reveal and independent page positions, visible workspace plugin choices, and Alt+Space leaving playback unchanged. Earlier f1/7dd flow evidence is scoped to those revisions; their processing methods were unchanged by the final navigation correction.

The browser v3 independent review scored **9/10** in the same four criteria. It used ordinary pointer/keyboard input, normal/enlarged text, search edits, Help focus/return, real preview playback and pending-mix updates. Final a2 landscape/tablet checks found no actionable defect. Browser emulation is not physical-phone qualification.

## Corrections and evidence boundaries

- Compact traces retain usable height after labels/ticks. Playback reserves its actual layout height.
- Active Voice, Repair and Plugins editors occupy the workspace; Preview remains separate from Export. Compact stem controls identify their actual part.
- Exact native button/header measurements handle platform fonts and fractional-DPI rounding without relaxing pane-width assertions.
- Modal content paints above skin artwork. Renderer diagnostics wait for elapsed presentation time as well as layout passes.
- Modified/repeating Space input does not toggle transport. Alt+Space does not activate a focused first-run choice.
- Completed spectrograms reveal once; later manual scrolling is respected. Studio pages keep independent positions.
- Browser Help reveals the selected answer and preserves search focus; compact playback avoids a nested primary-control scroll area.

One older Plugins-labelled capture actually showed Stems because its project restored that page. Final evidence uses a Plugins-view project and visibly selected tab. A Linux count audit also found a cached runner omitted the newest modal test despite current copied source bytes. Fresh qualification explicitly listed and executed it; the older 166-test result is not used as the final total. No compiler-bug cause is claimed.

## Reproduce and release checks

```text
cargo fmt --manifest-path rust/Cargo.toml --check
cargo test --locked --manifest-path rust/Cargo.toml --all-targets
cargo clippy --locked --manifest-path rust/Cargo.toml --all-targets -- -D warnings
python web/tools/verify_learning_browser.py --browser chromium --site YOUR_FRESH_STATIC_BUILD --out NEW_REPORT_FOLDER
python rust/tools/verify_release_contents.py --folder YOUR_PORTABLE_FOLDER --source YOUR_SOURCE_ZIP --report NEW_REPORT.json
```

Installer lifecycle qualification uses the production NSIS template and a byte-identical runtime payload under a separate QA registration/menu identity when an existing Preview must be preserved. It checks native WAV processing, installed MCP workflows, project reopen, upgrade, actual restored settings and data-preserving uninstall. Diagnostic captures suppress the automatic offer, so the live restart checks above establish tutorial persistence separately. See [release verification](RELEASING.md); artifact checksums and lifecycle reports belong beside the prepared artifacts.

Remaining boundaries are physical iPhone/Android behavior, hardware listening quality, complete screen-reader sessions and Linux file-picker portals. This UI pass does not requalify long-track memory maxima, neural model quality or arbitrary VST3 compatibility. Existing named Studio/Linux/audio reports retain their historical scope.
