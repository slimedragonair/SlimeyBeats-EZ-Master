# Hotfix4 desktop refresh

Desktop remains **0.5.0-beta.3**, with build revision **2026111501**;
the matching MCP companion remains **0.2.0-beta.3** and browser Studio
**0.3.0-beta.3**. The release stays under **v0.5.0-beta.3**. The original tag,
Hotfix3 binaries, 42-tool discovery and validation records remain historical.

## Scope

The refreshed desktop includes shared live-EQ filter preparation in bounded work,
the [stem timeline and time edits](STEM_EDITOR.md), desktop loop auditioning,
[Stem Doctor](STEM_DOCTOR.md) scans and explicit repair previews, and the
[45-tool MCP companion](MCP.md). Source builds already contain these features;
earlier installed binaries require the refreshed package.

The [main web app](https://slimedragonair.github.io/SlimeyBeats-EZ-Master/)
already includes the shared DSP, time edits, Doctor, local 320 kbps MP3,
portable EQ presets and [session ZIP exports](WEB_EXPORTS.md). Browser loop
auditioning, neural separation and native VST3 hosting remain outside its scope.
Browser delivery tools do not imply native project portability or an MCP bridge.

## Immutable assets

Publish these new assets on the existing
[release](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/tag/v0.5.0-beta.3):

| Asset | Purpose |
| --- | --- |
| `SlimeyBeats-EZ-Master-0.5.0-beta.3-Hotfix4-Setup.exe` | Windows x64 Studio Preview installer |
| `SlimeyBeats-EZ-Master-0.5.0-beta.3-Hotfix4.zip` | Matching portable Windows package |
| `SlimeyBeats-EZ-Master-0.5.0-beta.3-Hotfix4-Source.zip` | Clean source snapshot for this refresh |
| `SlimeyBeats-MCP-0.2.0-beta.3-Hotfix4-preview-Windows-x64.zip` | Standalone matching 45-tool companion bundle |

Record the final source commit, runtime identities, asset hashes and validation
scope with this candidate. Upload all referenced assets before publishing the
revision's `UPDATE-MANIFEST-2026111501.json`. Preserve earlier assets, catalogs
and checksums. GitHub's original tag archive does not reproduce the Hotfix4 source.

## Release validation

Consult the release's
[HOTFIX4-VALIDATION.json](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/releases/download/v0.5.0-beta.3/HOTFIX4-VALIDATION.json)
for final source/runtime hashes, build identity, native checks, actual 45-tool MCP
workflows, package contents/checksums, installer lifecycle and publication results.
The release owner finalizes that external report after QA; these bundled guides
describe scope without asserting test counts or pass results for the refresh.

Installer QA uses a **private app identity** with the production NSIS template
and **byte-identical runtime payload**, including an actual **Hotfix3 → Hotfix4
upgrade** in that private channel. It preserves the user's running production
app and MCP. Private registration/shortcut checks do not establish that production
registration was tested. Consult the report for completed steps and outcomes.

Earlier source/editor/Doctor/browser checks and synthetic native screenshots
qualify their named builds; they do not qualify this installer. Hardware listening,
physical-phone behavior and Linux runtime acceptance retain their separately
documented limits. See [release procedure](RELEASING.md).
