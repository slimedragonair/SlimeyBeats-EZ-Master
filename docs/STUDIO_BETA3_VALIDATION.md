# Initial Studio beta.3 validation

This record covers desktop **0.5.0-beta.3**, MCP **0.2.0-beta.3** and browser Studio **0.3.0-beta.3**, following removal of the fixed imported-part ceiling. Earlier beta.2 results remain separately identified in the [Studio guide](STUDIO.md#validation-status--november-13-2026).

This record describes the larger-bank candidate before the subsequent merge-review fixes. Current merge regressions and release/deployment distinctions are recorded in [Merge validation](MERGE_VALIDATION.md).

## Confirmed results

| Scope | Measured result and boundary |
| --- | --- |
| Windows native | **115 tests passed** with `cargo test --all-targets --features schemas`, using the default debug/test profile. Format checks and strict Clippy passed. This count is not a release-profile unit-test run. |
| Linux native | **117 tests passed** with `cargo test --locked --all-targets`, using the default debug/test profile. Format checks, strict all-target Clippy and the locked release build passed. |
| MCP regressions | **9 Windows / 10 Linux tests passed**. Live discovery exposed **38 tools**. Protocol workflows exercised Windows debug/release binaries and the Linux companion separately from the native unit-test counts. MCP formatting and strict Clippy passed on Windows and Linux. |
| Host / plugin faults | The host strict build/Clippy checks passed. Actual plugin-fault checks passed null-factory failure, worker exit and cancellation. This establishes the tested fault-handling cases, not arbitrary-plugin compatibility. |
| Larger-bank protocol | **24-, 64- and 100-part loose banks and paired WAV/MP3 ZIPs** passed import-all defaults, extensible/canonical IDs, sample-exact raw output and rejection/recovery checks. A 64-part project retained identities/faders; editing the custom last part preserved the other 63 raw parts exactly. Distinct source folders with equal basenames remained distinct. Mastering was not invoked by these import/edit checks. |
| Per-input semantics | Approximately **9.6 MB combined input** imported with an **8 MiB per-input setting**, demonstrating that the setting applies to each file rather than the sum of the bank. The native aggregate bank guard remains separate. |
| Release data/project workflow | An authorized full-length **11-part** bank and a **96-part** synthetic bank passed release MCP import-all, raw PCM comparison, identity/name preservation, targeted fader changes, project persistence and working float-mix export/native reopening checks. Original input files remained unchanged. This is a data/protocol result; GUI review is separate. No private recording or identifying path is published here. |
| Web DSP bindings | **11 native-host binding/library tests passed**, including 24/64-part banks, exact raw/mixed float export, history, identity/alignment rejection, the combined **128 MiB** input boundary and rejection of the next sample over it. These binding checks do not qualify physical phones. |
| Browser UI | **Chrome 156, Firefox 144 and Linux WebKit 26 each passed all 20 workflow groups** in the latest complete larger-bank suites. Coverage includes 24 manual rows, 64-part import/export, a twelve-part all-selected ZIP, names/manifests, distinct-folder pairing, selection/omission controls, normal clicks, error retention, history and mastering handoff. |
| Final independent visual review | The four actual v2 compact native views (11/96-part review and loaded projects) and the synthetic 96-part wide import view each scored **9/9/9** for look, polish and readability. Reviewed browser captures also scored **9/9/9**. Compact collection density and immediate fader access were resolved in these inspected states. [Actual synthetic 96-part import illustration](images/studio-large-import.png). |
| Fresh-agent discovery | An agent without project/chat history used live instructions/schemas/help to import **96 of 96 ZIP parts**, lower `part_96` by **2 dB** and prepare a preview while leaving the other 95 controls unchanged. No exports were called during that phase. Only after a separate explicit request did the agent save **24-bit WAV, all 96 raw stems and a self-contained project**. The other 95 controls remained unchanged, and the bridge closed cleanly with exit code 0. This qualifies the tested protocol harness, not every AI host/model. |

The confirmed counts describe the tested fixtures, not a new maximum or an unlimited-capacity promise. Automatic neural extraction remains **two/four outputs**; this follow-up does not requalify every model, plugin or recording.

## Resource policy

Native imports retain **512 MiB decoded per input by default**, with an independent **4 GiB decoded-bank guard**. The owner-configured MCP per-input limit does not replace that bank guard. Browser Studio retains **128 MiB combined decoded input** and **15 minutes per track**. ZIP byte limits, the **8 MiB filename-plus-per-entry metadata budget**, decode timeouts and existing project JSON/per-asset safeguards still apply. Mixes, history, audition and export require additional memory.

All parts must have identical decoded rate, channel count and frame count. Removing the count ceiling does not add a free timeline, beat matching, automatic alignment or new resampling controls. See the [current workflow and exact limits](STUDIO.md#import-a-whole-stem-set).

## Qualification limits

Independent review passed the supplied native v2 compact/wide-import and browser states at **9/9/9**. This is scoped screenshot acceptance, not certification of every UI state or interaction; full-name hover, middle/end scrolling and dirty-state interaction were not retested by that review. WebKit's corrected complete suite and both fresh-agent phases passed. The recorded native totals remain **115 Windows / 117 Linux**.

These checks do not establish physical audibility, physical iPhone/Android behavior or arbitrary-plugin compatibility. Beta.3 packaging/installer evidence is recorded separately; earlier beta.2 installer/visual evidence remains historical.

## Reproduction

Windows native count:

```sh
cargo test --manifest-path rust/Cargo.toml --all-targets --features schemas
```

Linux native and MCP regression counts:

```sh
cargo test --locked --manifest-path rust/Cargo.toml --all-targets
cargo test --locked --manifest-path mcp/server/Cargo.toml --all-targets
```

Larger-bank protocol checks use a separately built matching companion:

```sh
python mcp/tools/verify_unbounded_stems.py --server PATH_TO_BUILT_SERVER
```

Browser checks use `web/tools/verify_studio_browser.py` with the chosen browser and an isolated output folder. Test fixtures and reports stay outside public source packages; only explicitly licensed examples belong in shipped artifacts.
