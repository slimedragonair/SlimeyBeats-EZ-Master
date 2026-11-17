# GitHub check scopes

Pushes and pull requests select checks from the changed paths and their shared
dependencies. This is dependency-based selection, not an assumption that an old
passing result validates new source. Manual workflow dispatch keeps the full
checks available.

| Change | Automatic checks |
| --- | --- |
| MCP code or tools | MCP format, lint, unit/integration tests, build and compact/legacy protocol smoke on Windows and Linux |
| Desktop-only code | Native checks and GUI validation |
| Shared Rust audio code or Cargo dependency changes | Native, MCP, browser/arrangement and server checks |
| Browser runtime, WASM or browser probes | Browser/arrangement checks and mobile preview publication |
| Server code | Server and affected browser integration checks |
| VST3 worker | Worker checks and MCP integration |
| Guides or documentation packaging | Documentation links/payload and companion packaging checks |
| Workflow-selection code | Selector regressions; application builds only when their source also changed |

The selector lives in `.github/ci/check_scope.py`, with deterministic path/event
regressions beside it. `.github/workflows/change-scope.yml` exposes the same
outputs to each workflow. Existing platform job names remain stable; irrelevant
jobs skip before toolchain or browser installation. The selector regression suite
runs once when CI policy changes, rather than repeating in every caller. Missing or invalid comparison
history selects the full checks rather than silently skipping validation.

For an MCP-only catalog update, desktop/GUI builds, browser suites, hybrid server
suites and mobile publication are unnecessary. The older extended DSP/model/
codec/plugin smoke runs remain available for relevant shared-engine changes or
manual full validation. The compact protocol smoke checks the current 33-tool
catalog, typed routing, preserved work, persistence and the legacy profile.
Optional-tool safety also runs for MCP changes on both platforms; the generated
connection configuration receives the Windows PowerShell 5.1 Unicode checks.


## Resume an unchanged application's publication

The manual **Resume qualified main browser publication** workflow can finish the
specific main run that stopped in the stem-editor check. It verifies the trusted
repository/main run, its actual successful prefix and artifact, and unchanged
application inputs. The rebuilt JS/WASM/CSS/HTML runtime must match the passed
baseline reports. It reruns the failed stem-editor check and every later check
that did not run before publishing. A product/runtime change refuses reuse and
requires the normal full browser workflow.

Only the explicitly listed validator/publisher-policy files select CI-only
checks; their offline regressions still run. This exception does not apply to
application JS/CSS, Rust/WASM, audio, build or encoder changes. Main and preview
publication retain their source, origin, artifact and rollback-protection guards.
