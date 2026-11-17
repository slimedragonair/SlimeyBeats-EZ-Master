# Shared EQ proposal transport

The shared engine owns analysis and proposal rules. Adapters transport the
same seven bands and optional high-frequency evidence; requesting a proposal
does not render, replace audio, or accept it for export.

* `rust/src/dsp/eq_suggestion.rs`: `proposal` returns `EqSuggestion` with
  `bands`, human-readable `notes`, and optional `high_frequency` evidence.
  `suggest` retains its bands-only API.
* `web/dsp/src/session.rs`: `Session.suggest` retains its JSON array;
  `Session.suggest_detailed` and worker action `suggest_detailed` return an
  object with all three fields. Evidence is explicitly null when unavailable.
  Both measure the immutable pre-EQ master when available, otherwise input.
* `rust/src/hybrid_worker`: the native v2 response uses the shared proposal
  type, along with its source analysis. No new operation or API version is
  required. Existing consumers can continue reading `suggestion.bands`.
* `hybrid/server/eq_suggestions.py` validates bands and evidence for
  `command.result`. It creates fixed safe notes from validated evidence;
  native free text and filesystem metadata do not cross this boundary.
* `web/site/hybrid/remote-mastering.js` normalizes current and older wrapped
  server responses to `{bands, notes, high_frequency}` for the browser UI.
* MCP `suggest_eq` retains `bands`, `applied`, and `basis`, adding `notes` and
  `high_frequency`. Analysis summaries also expose the optional evidence.

`Spectrum.high_frequency` is optional when reading older analyses. Both
browser-supplied v1 references and project-owned v2 references explicitly
validate this extension through `hybrid/server/high_frequency.py` instead of
silently accepting an untrusted numeric tree. Native worker and CLI reference
ingress also call the shared summary's `valid` method. Public analysis keeps
the `high_frequency_excess` issue with fixed safe explanatory text.

This evidence concerns sampled 10 kHz+ content; a candidate is a listening
review heuristic. Its observed seconds do not establish continuous coverage,
and unavailable evidence does not establish that a recording is clean.
Doctor's timestamped issue kinds and explicit repair operations are unchanged.
Ship the generated WASM/bindings, worker and page together through the existing
runtime cache versioning build when publishing the new browser method.

## Rollout mapping

Status on November 16, 2026: commit
`aee0275259cb0b3bc3bb6f3e830947d779da8e3c` passed isolated Linux staging.
Both release binaries built; 233 headless native tests and all 119 backend
tests passed. Two optional downloaded-bundle native tests remained ignored.
Independent balanced/boosted fixtures verified real HTTP/native suggestion
and HF-evidence parity, reference round trips, unchanged source audio, and
bit-identical float-PCM export. The backend suite exercised all six formats.
The independent export probe initially rejected a valid extensible float-WAV
header; its parser was corrected and the targeted probe passed without
changing product source. All 1,071 staged source files retained their committed
bytes. This qualifies the staged service, not the deployed preview backend:
live file hashes, service identity and HTTP health remained unchanged. The
code/binary rollout bundle is prepared; production rollout and live feature
verification remain separate operator steps.

1. Parent checkpoints `current-work` after quick native/WASM/backend checks.
   `.github/workflows/current-work-preview.yml` publishes `/current-work/`
   while retaining main; the separate hybrid workflow tests Linux boundaries.
2. Archive the exact supplied full commit, verify its archive/source hashes,
   and stage it in a new secondary-disk directory. Keep Cargo downloads,
   target outputs, temporary files and evidence there. Never build into the
   live worker's target directory. Run synthetic fixtures in separate local
   test services/storage, with bounded CPU/memory and no production credentials.
   Build both native binaries with `cargo build --locked --release
   --no-default-features --manifest-path rust/Cargo.toml --bin
   slimeybeats-ez-master-cli --bin slimeybeats_hybrid_worker`. Run
   `python -m hybrid.tests --cli <cli> --studio-cli <worker> --ffmpeg <ffmpeg>`.
3. Deploy the Python backend and both binaries together through the existing
   operator-managed preview service. Preserve its environment, storage,
   model/codec runtime and ingress configuration. Retain the prior revision
   for rollback. Deployment follows test review and handoff; it is not part
   of staging. Back up the code and both executables together, including
   hashes and permissions, before any authorized replacement. A browser-only
   publication does not update server DSP.
4. After an explicitly authorized backend rollout, verify a real v2 suggestion
   returns version-1 `command.result.high_frequency`, safe notes and seven
   bands; verify a summary-bearing reference/master round trip and explicit
   proposal application/export. Generic readiness alone is insufficient.

This work does not restart the desktop, deploy the backend, or merge into main.

## Conservative upper-treble check

The existing bounded spectrum pass supplies per-window, summed channel powers;
no second FFT pass or instrument model is added. The detector compares mean
power density in **10–min(16 kHz, 0.49 × sample rate)** against **2–8 kHz**.
It needs at least two seconds and 4 kHz of usable upper-band width. A usable
midrange anchor, broadband shape within each window, and repeated or sustained
support are required. A single brief crash, a pure high note or an isolated
high-band effect is insufficient. Codec cutoffs and unavailable octaves are
respected; unavailable 16 kHz measurements cannot influence 32 kHz proposals.

| Product heuristic | Initial threshold |
|---|---|
| Midrange anchor | At least 1% of measured audible-band power |
| Upper-band density contrast | Greater than 3 dB |
| Upper-band integrated fraction | Greater than 18% |
| Per-window flatness / top-three-bin fraction | Greater than 0.18 / less than 0.35 |
| Measured support | At least eight matching windows and 0.5 seconds of their union |
| Sustained support | At least 30% matching active windows, with a run of six |
| Repeated support | At least three groups of two matching windows, separated by measured nonmatches; at least 5% matching active windows |

These are conservative product rules, not validated instrument profiles or
proof of audible damage. A moderate treble boost can remain below this extra
detector's threshold while the ordinary tonal proposal still suggests changes.
Flatness describes noise-like versus tonal structure, not quality or AI origin.
The [spectral-flatness definition](https://librosa.org/doc/0.11.0/generated/librosa.feature.spectral_flatness.html)
and [spectrum/density distinction](https://docs.scipy.org/doc/scipy-1.16.1/reference/generated/scipy.signal.welch.html)
are primary implementation references; the thresholds above are our own.

Evidence can request **0.75–2.5 dB attenuation** at the existing 10 kHz shelf.
It uses the stronger existing cut rather than adding a second correction.
The complete proposed high-frequency response is checked against a 3 dB cut
budget; overlapping active cuts are eased toward zero when needed. A shelf
changes everything in that region and cannot recreate missing drum detail.
Proposals use the unchanged pre-user-EQ master where available, otherwise the
original. Existing mastering processing is not automatically changed by this
new issue flag.

## November 16 checkpoint validation

- **12 native integration regressions** passed: 32/44.1/48/96 kHz positives,
  balanced white/pink noise, clean tones/sweep, single brief crash, missing
  anchor, silence/short/low-rate abstention, gain and channel invariance,
  repeatability, source preservation, overlap-excluded measured support,
  invalid-octave masking and a non-additive cascade.
- A separate private-helper regression forces the rescaling branch with
  overlapping custom bands, compares a dense independent response grid, and
  preserves disabled bands. The five suggestion unit tests passed.
- **Three native GUI/worker tests** passed for explicit Apply and rejection of
  cancelled, replaced-source or changed-profile proposals. **51 Node state
  checks** passed; a parent rerun of the ten proposal/remote adapter checks passed.
- Chromium 143, Firefox 144 and Linux WebKit 26 passed four actual-UI workflow
  groups at each of **320/390px**, using private runtime `e791b37b36569e07`.
  They exercised a positive high-frequency fixture, running A/B, explicit Apply,
  late replies and same-name source replacement. The compact explanation stays
  collapsed by default. These are browser-engine checks, not physical phones or
  a listening-quality verdict; the deployed preview reports its own build.
- Matching transport tests passed: MCP 2, WASM 3, native worker 1, CLI 4,
  backend 18 with one platform skip, remote adapter 3, documentation 18 and
  MCP packaging 5. Production-service deployment remains a separate check.
- A frozen `aee0275` Windows all-target run passed: **373 library tests**, two
  existing ignored tests, and all binary/integration targets. An owner-provided
  **179.8-second, 48 kHz stereo WAV** completed full native suggestion analysis;
  its file hash stayed unchanged and no extra upper-treble cut was requested.
  The private test exported no audio and supplies no listening-quality verdict.
- The `aee0275` hosted development preview passed its publication smoke, and
  Windows/Linux and backend CI passed. Broader browser CI passed DSP and native
  streaming in all three engines, then stopped because the newly added UI-test
  command omitted its required `--site` argument. The command was corrected;
  later steps from that run remain unrun rather than being counted as passed.
- Portable session analysis now retains the upper-treble measurements and
  proposed-cut guidance in separate sections. Two focused packaging regressions
  passed, including old reports and exclusion of unexpected fields.

## Arrangement integration on current-work

The later arrangement work adds whole-mix, track and clip targets, with optional
time ranges, through the shared Rust core, desktop interface, local browser
editor and matching MCP companion. Targets use stable IDs and exact frame
bounds; clip EQ ranges refer to immutable source frames. The proposal still
requires explicit acceptance and does not replace an existing effect until
applied. Applied effects can be revisited, bypassed or removed.

See [the shared contract](ARRANGEMENT_CORE.md),
[desktop workflow](ARRANGEMENT_NATIVE_UI.md) and
[browser workflow and qualification](WEB_ARRANGEMENT_UI.md) for current behavior
and validation. Server arrangement editing remains disabled in the browser
pending short-preview, EOF and source-clock qualification. This work does not
update the published Hotfix4 installer or an already installed MCP companion.
