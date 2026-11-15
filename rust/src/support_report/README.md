# Private support report backend

This module creates a local Markdown document. It performs no network requests,
browser launches, audio reads, project reads, log collection or uploads. It does
not read environment variables. OS and architecture are compile-time constants.

## Integration

Register `pub mod support_report` on native targets in the parent library. Use
the actual running desktop library, rather than a companion server's identity.

For the MCP adapter use `ReportContext::for_mcp(env!("CARGO_PKG_VERSION"))?`
in the companion crate. Its version argument is a bounded, semver-validated
compile-time `&'static str`; do not pass a user-supplied string. This factory
emits `component: mcp_companion`, `companion_version`, `linked_engine_build`
and `installed_desktop_version: null`, not the desktop `application` field.
The linked library does not identify a separately installed GUI. It labels the
scope as this connection only, says readiness is not probed, and adds an explicit
AI-client/model-provider caveat to the report itself. Desktop `current()` keeps
its existing identity shape. Neither factory queries an executable or cache.

Delivery wording follows the context: desktop previews stay local, while MCP
arguments and returned text may already reach the AI provider before review.
The shared data/redaction notice makes no unconditional on-device promise.

```rust,ignore
use mastercraft::support_report::{self, IssueCategory, ReportContext, ReportDraft, Workflow};

let mut context = ReportContext::current();
context.workflow = Workflow::StemStudio;
context.counts.stems = 9;
context.activity.preview_available = true;
let draft = ReportDraft::new(IssueCategory::Stems, user_title, user_description)?;
let report = support_report::build(&context, &draft)?;
// Display the whole document for review. Copy exactly this string.
show_preview(report.markdown());
// Only on the user's Save action, to their chosen fresh absolute filename:
report.write_new(&destination)?;
```

`ReportContext` has a fixed allowlist: actual app version/build revision/channel,
OS/architecture enums, a built-in workflow enum, track/stem/EQ-band counts,
yes/no/unknown tool readiness and activity booleans. Its identity fields cannot be replaced
by callers. It has no raw string, path, username, error or log field and no
deserializer. Do not convert user names or custom labels to diagnostics. Tool
fields are `Option<bool>` and default to `None` (JSON `null`: unknown/not checked).
Use `Some(true)` only for an already-known ready observation, or `Some(false)`
only for an already-known not-ready observation. Do not infer missing or failed
tools from absence of a check. Both constructors and report rendering probe and
download nothing. The MCP adapter leaves all four readiness observations unknown.

`ReportDraft::new` accepts only deliberately user-entered title/description and
an issue-category enum. It rejects title input above 512 bytes or description
input above 16 KiB before allocating a copy. It normalizes control/directional
characters and conservatively removes obvious absolute Windows/POSIX/UNC paths,
email addresses, credential assignments, recognizable token-like text and PEM
private-key blocks. Unquoted paths redact the rest of their line because real
paths may contain spaces. Only the sanitized strings are retained.

Automatic sanitization is best-effort. Relative filenames, names embedded in
ordinary prose and unknown secret formats may survive. Always display
`REVIEW_NOTICE`, show if `text_was_changed()` is true, let the user revise their
description and review the rebuilt preview before copying/saving. Never present
the report as guaranteed anonymous. Do not autofill typed fields with raw logs,
errors, song names, plugin names, profile names, paths or project data.

`build` is deterministic and caps the entire report at 64 KiB. User-authored
sections are indented Markdown; the automatic diagnostics block is trusted typed
JSON. There are no hidden attachments. Keep one immutable `Report` for preview,
clipboard and save: `write_new` writes precisely `markdown().as_bytes()`. There
is intentionally no upload/submission method or alternate JSON attachment.

Saving stages a synced tempfile in the chosen existing directory, publishes
without replacing an existing file/symlink and cleans failed staging. No parent
folders are created. Unix temporary files use mode 0600; directory permissions
and platform ACLs still matter, and reports are not encrypted. Save errors do
not format the selected filesystem path into their message.

`PRIVATE_SECURITY_REPORT_URL` identifies the official GitHub private advisory
form. The GUI may offer an explicit separate action for security-sensitive
reports; it must not open that URL or submit anything automatically. GitHub
account/login and the repository's private-report availability are outside this
local backend's control.

## Tests and extension points

Tests cover actual identity, allowlist-only diagnostics, seeded private material,
sanitization, text bounds, repeated short paths, deterministic rendering,
preview/save byte equality, non-overwrite, cleanup and path-free save errors.
Unix additionally verifies symlink non-overwrite and mode 0600. This module does
not claim clipboard/UI review testing; the parent owns those integrations.

Add future automatic diagnostics as typed enum/count/boolean fields with privacy
regressions. Do not add arbitrary maps, log attachments or free-form environment
and error strings to `ReportContext`.
