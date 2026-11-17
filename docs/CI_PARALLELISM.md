# Full browser validation in parallel

The Browser preview workflow builds once, then runs Chromium, Firefox and
WebKit in separate runners, each split into core, studio and learning suites.
Arrangement Studio builds once and runs one isolated runner per engine.
Tests remain sequential within each runner to avoid interference between audio
sessions. No assertions, timeouts, real audio fixtures or required engines are
removed. Native/hybrid release workflows are unchanged.

## Coverage and publication

Browser retains 57 browser invocations (18 suites on all three engines plus
three browser-neutral invocations), together with existing Rust, JavaScript and
Python checks. Arrangement retains 25 invocations on push/PR and 26 on manual
runs, including its manual-only large guest test. The existing `browser` job is
now a fail-closed aggregate: all build/shard results must be successful. A
failure, cancellation or skipped required shard cannot publish Pages.

Each workflow builds its own inputs once. Consumers download the exact artifact
ID, verify the tar SHA-256 emitted by that build, then verify a complete payload
manifest against the checked-out commit SHA, workflow run ID and original build
attempt. The manifest binds every site file and both native workers. Caches only
accelerate builds; cached inputs are rebuilt and validated, never published as a
substitute for the current commit. Archive transport retains executable modes.

A failed-job-only rerun can reuse the successful build from the same run and
commit: verification uses the build's attempt output, not the consumer's newer
attempt. Validation report names include engine, suite, run and attempt.
The old one-off browser-publication-resume workflow stays source-guarded and
cannot be used on this new workflow layout. Its historical command-tail contract
is retained against the frozen baseline fixture.

## Measurement

The serial Browser run 44079172138 reached its 50-minute cap during learning.
Recorded preceding sections included imports 289s, remote/native 340s, Firefox
436s and EQ/exports 429s; Chromium learning alone took 281s. Arrangement run
44079172157 spent 818s in the serial three-engine guest section. These timings
motivate independent runner shards rather than smaller test fixtures.

The expected wall-clock improvement is approximately 15–25 minutes for the
longest full workflow, subject to hosted runner capacity; this is an estimate,
not a measured result. Each browser shard writes `shard-timings.jsonl` alongside
its reports so later grouping decisions can use observed timings. Increased
parallelism trades more runner setup/minutes for less elapsed waiting. Browser
allows nine parallel shards and Arrangement three, within GitHub account limits.
Do not lower the inherited 75-minute browser timeout until hosted evidence
establishes reliable headroom. Full CI on the final rebased commit remains the
release gate.
