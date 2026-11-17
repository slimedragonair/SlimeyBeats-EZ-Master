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
