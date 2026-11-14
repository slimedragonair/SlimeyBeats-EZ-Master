# Prepare a GitHub release

The beta.3 Preview uses desktop **0.5.0-beta.3**, optional MCP companion **0.2.0-beta.3** and browser Studio **0.3.0-beta.3**. Its release page is [v0.5.0-beta.3](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/tag/v0.5.0-beta.3), with [Windows Setup](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.5.0-beta.3/SlimeyBeats-EZ-Master-0.5.0-beta.3-Setup.exe) and [source ZIP](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.5.0-beta.3/SlimeyBeats-EZ-Master-0.5.0-beta.3-Source.zip). Keep these versions for this release, include the Basic/Advanced interface, persistent optional eight-step tour, Help and desktop MCP setup, and preserve stable v0.4.2 and the previously published beta.1 Preview history. Linux remains source-build only.

1. For subsequent releases, update the applicable versions in `rust/Cargo.toml`, `mcp/server/Cargo.toml` and `web/dsp/Cargo.toml`, plus the changelog. Keep the README/features consistent with the actual release channel. Verify every bundled asset's source/license; do not add a personal recording as an example without permission.
2. Run formatting, Clippy, the Rust suite and the appropriate DSP/export checks. Regenerate actual native screenshots when visible behavior changes; do not use a concept image as proof of the app.
3. Build the binaries, then run `python rust/tools/build_release.py --out dist` from the repository root. Versions and artifact names are read from Cargo metadata. NSIS is pinned/checksummed.
4. Check the clean source folder/ZIP before publishing. It is an allowlist export, not the current development checkout. Confirm `LICENSE`, `OUTPUT_OWNERSHIP.md`, third-party notices, example credits and relative README images are present. No personal audio, settings or private reports should be included.
5. Test the **actual Setup.exe** in a fresh writable folder: install, verify Start menu/Apps registration, launch/analyze/master/export, reinstall without losing user data, and uninstall. Confirm settings, a custom profile and an exported-audio sentinel remain. Test installation paths containing spaces. Do not disturb an existing installation to perform QA.
6. Verify the portable binaries, source build and archive checksums. Signing is optional and requires a certificate controlled by the publisher; an unsigned build must be described honestly.
7. Review and commit the final source/docs on the release PR. Prepare the version tag and a draft GitHub prerelease with the qualified versioned `Setup.exe`, portable ZIP, source ZIP and checksums. Publish only after the final PR head passes its checks. For beta.3, publish the assets before merging PR #10 so the primary download links work when the updated README reaches `main`. No repository owner/URL is invented by these tools.

The included CI is configured to build/test native code on Windows and Linux, with MCP and isolated-worker checks. The manually triggered packaging workflow builds release artifacts; it does not automatically publish a GitHub release. Release tooling does not upload anything automatically. Verify the actual Actions results, public README and release downloads for the candidate being published; documentation alone does not establish a passing run or deployment.

For Studio betas, build the companion and plugin worker before packaging. Use `verify_installer.py --channel studio-preview` with a fresh test folder outside the checkout. The verifier checks the preview identity, program removal, actual installed MCP workflows, a saved Studio project and preservation of the other installed channel. It also checks a test-owned foreign-channel marker, without attempting to install over the user's stable folder. Prereleases retain their SemVer display string and use a separate numeric Windows version resource.

For the beta.3 learning interface, `verify_release_contents.py` checks every bundled guide, its local image/file targets, canonical guide JSON in the source archive, component notices and the clean audio allowlist. Installer verification preserves separate master/Studio export destinations, desktop/interface preferences and `tutorial-dismissed.marker` through upgrade/uninstall. The interactive run also reopens the installed app and saves its restored preferences, checking that music/projects remain unchanged. `--skip-gui` does not exercise that reopen path. Separately test actual first-run completion/decline and restart, manual replay, Help focus and the OS clipboard; diagnostic captures suppress the automatic offer and cannot establish those input behaviors.

The [UI validation record](UX_VALIDATION.md) freezes the prerelease runtime hashes, earlier installer identity and hosted results on source `48015f0a7a8afac22b89a8986c51c274e4029233`. Native checks #50 and Browser preview #33 passed for that source; they are separate from qualification of new archives or Setup.exe. The earlier GUI-enabled lifecycle used the production NSIS template and byte-identical 2,012-file payload under a private QA identity, reinstalling the same candidate. It did not install production Preview registration or prove migration from every earlier version. Rebuilding with final guides/images changes packaging: rerun content and actual-installer checks, publish new artifact checksums, and retain the earlier hashes as historical evidence. A documentation edit cannot establish those new results.

For beta.3, include loose-file/ZIP imports beyond the earlier fixed count cap, matching rate/channels/frame count rejection and live discovery of the optional companion's 38 tools. Keep two/four-stem model separation unchanged. Verify the distinct budgets: native defaults of 512 MiB decoded PCM per input and a 4 GiB decoded bank versus browser Studio's 128 MiB combined input and 15-minute per-track guard. ZIP/metadata and working-memory limits still apply. Use the [Studio guide](STUDIO.md) and [beta.3 validation record](STUDIO_BETA3_VALIDATION.md) to identify qualification scope and remaining checks.

The hosted packaging workflow uses `verify_installer.py --skip-gui` to test installation, native CLI processing, upgrade and data-preserving uninstall. Its report explicitly marks GUI capture as untested. A hosted native GUI capture did not complete successfully; the workflow does not claim that coverage. Run the verifier without `--skip-gui` on an interactive Windows desktop for the GUI processing/capture check, which passed locally for v0.4.2.

The public repository is [slimedragonair/SlimeyBeats-EZ-Master](https://github.com/slimedragonair/SlimeyBeats-EZ-Master). The primary beta.3 download must be labeled **Studio Preview**; keep **stable v0.4.2** as a clearly labeled fallback. Publish the qualified assets under the exact linked tag and verify the release downloads before announcing them. A merge or Cargo version alone does not create those assets. The latest-release badge follows the newest stable release, so it does not identify the current Preview.

For a fork, update the repository metadata/download destination locally:

```powershell
python rust/tools/configure_repository.py --repository YOUR-ACCOUNT/SlimeyBeats-EZ-Master
```

That helper does not upload files. Verify its links against the fork's actual release before publishing.

## Artifact contents

| Artifact | Purpose |
|---|---|
| `SlimeyBeats-EZ-Master-VERSION-Setup.exe` | Current-user Windows installer, offline guides, notices and demo |
| `SlimeyBeats-EZ-Master-VERSION.zip` | Portable application, source, documentation and demo |
| `SlimeyBeats-EZ-Master-VERSION-Source.zip` | Clean GitHub-ready source, including relative images/audio |

Keep dependency/stub notices with redistributions. MIT attribution belongs with software copies, not users' music outputs. Third-party material retains its own license.
