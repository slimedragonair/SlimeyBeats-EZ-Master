# Prepare a GitHub release

1. Update `rust/Cargo.toml` and the changelog. Keep the README/features consistent with the actual native release. Verify every bundled asset's source/license; do not add a personal recording as an example without permission.
2. Run formatting, Clippy, the Rust suite and the appropriate DSP/export checks. Regenerate actual native screenshots when visible behavior changes; do not use a concept image as proof of the app.
3. Build the binaries, then run `python rust/tools/build_release.py --out dist` from the repository root. Versions and artifact names are read from Cargo metadata. NSIS is pinned/checksummed.
4. Check the clean source folder/ZIP before publishing. It is an allowlist export, not the current development checkout. Confirm `LICENSE`, `OUTPUT_OWNERSHIP.md`, third-party notices, example credits and relative README images are present. No personal audio, settings or private reports should be included.
5. Test the **actual Setup.exe** in a fresh writable folder: install, verify Start menu/Apps registration, launch/analyze/master/export, reinstall without losing user data, and uninstall. Confirm settings, a custom profile and an exported-audio sentinel remain. Test installation paths containing spaces. Do not disturb an existing installation to perform QA.
6. Verify the portable binaries, source build and archive checksums. Signing is optional and requires a certificate controlled by the publisher; an unsigned build must be described honestly.
7. Create the GitHub repository from the clean source export, or apply the curated source to the intended public repository. Review before pushing. Add a version tag and a GitHub release, then attach the versioned `Setup.exe`, portable ZIP and source ZIP. No repository owner/URL is invented by these tools.

The included CI builds/tests native code on Windows. The manually triggered packaging workflow builds release artifacts; it does not automatically publish a GitHub release. Release tooling does not upload anything automatically. Verify the public README, release downloads and Actions checks after each publication.

The hosted packaging workflow uses `verify_installer.py --skip-gui` to test installation, native CLI processing, upgrade and data-preserving uninstall. Its report explicitly marks GUI capture as untested. A hosted native GUI capture did not complete successfully; the workflow does not claim that coverage. Run the verifier without `--skip-gui` on an interactive Windows desktop for the GUI processing/capture check, which passed locally for v0.4.2.

The public repository is [slimedragonair/SlimeyBeats-EZ-Master](https://github.com/slimedragonair/SlimeyBeats-EZ-Master). README download buttons point to the published versioned Windows installer; update their version together with Cargo metadata and release artifacts. The latest-release badge always follows the newest stable release.

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
