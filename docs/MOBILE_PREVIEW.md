# Live current-work preview

[Open the mobile development preview](https://slimedragonair.github.io/SlimeyBeats-EZ-Master/current-work/)

The preview follows the `current-work` branch. Push a small checkpoint and the **Current work mobile preview** workflow builds it, checks the actual subdirectory, and publishes it. The separate native/browser workflows continue broader validation. A preview build passing its smoke checks does not mean every feature or physical phone has been qualified.

## Check which build you are testing

The page shows **current-work preview** and a short commit ID. Expand **Build details** for the full commit, build time, preserved main-site commit and workflow run. Reload after an update. If metadata and loaded runtime disagree, the page asks you to reload instead of claiming the newer commit.

The normal app remains at the [main site](https://slimedragonair.github.io/SlimeyBeats-EZ-Master/). Native installers and `main` source are not promoted by a preview push. This is the responsive browser app, not a native iOS/Android package. Audio is processed locally; phone codec, memory, playback interruptions and speaker/headphone behavior still need device testing.

## Private storage

Preview preferences, tutorial dismissal, recovery database, recovery messages and recovery locks use a separate `current-work:` namespace. Testing or clearing preview recovery does not change main-site restore points. Both paths share a browser origin and its storage quota; namespacing is accidental-data-loss protection, not a separate security origin. Existing private recovery files can still be imported deliberately through the normal controls.

## How publication preserves main

GitHub Pages publishes one artifact for this repository. The preview workflow retains the last successfully published main files at the root and adds the new build under `/current-work/`. It checks the live main index against that retained artifact and records every stable/preview file hash. It refuses to publish if main is being built, the selected main snapshot changes, or `current-work` advances during preparation.

Immediately before deployment it repeats the identity and main checks. After deployment it checks the public preview's commit/runtime and the hashes of key main/preview files. Production deploys and preview publication use a shared concurrency group once the updated workflow reaches `main`.

The original Pages artifact expires after one day. Each successful preview retains an exact, hash-checked main baseline for up to 90 days and refreshes it on later preview builds. Fallback accepts only the same latest successful main identity from a successful preview workflow in this repository. If all matching snapshots expire or are removed, rerun the existing main browser workflow to refresh its artifact; the publisher fails rather than guessing or rolling back the live site.

Before this infrastructure workflow is merged into the default branch, the `workflow_run` callback cannot refresh a preview after an unrelated main deployment. Such a deployment can remove the subdirectory until the next `current-work` push or rerun. After the workflow reaches `main`, successful main publications trigger preview refresh automatically. Setting up this preview does not merge the app's development features into main.

## Files and checks

- `.github/workflows/current-work-preview.yml`: fast build, smoke/storage checks, retained main baseline and Pages deployment.
- `web/tools/pages_artifacts.py`, `pages_baseline.py`, `compose_pages_preview.py`: bounded retrieval/extraction, source provenance and root-preserving composition.
- `web/tools/verify_pages_publication.py`: public identity and critical-file verification.
- `web/site/preview-channel.js`, `preview-info.js`, `preview.css`: storage channel and readable build identity.
- `verify_mobile_preview.py`: actual import/master/live EQ/MP3/report/ZIP workflow at phone widths, locally or over HTTPS.
- `verify_preview_storage.py`: real browser storage, cross-tab and root/preview isolation checks.
- `verify_region_edit_browser.py --worker-only`: generated-fixture time-edit engine,
  proposal/history/recovery and ZIP checks before publication. The complete
  timeline UI checks run separately in broad browser CI.

The browser app's existing input, recovery and export limits still apply. No original recordings, private recovery points, tokens or local paths belong in the static artifact or build metadata.

## First live validation — November 15, 2026

The first public build, `ba1687c`, passed its [GitHub publishing workflow](https://github.com/slimedragonair/SlimeyBeats-EZ-Master/actions/runs/43711943707). Its runtime was `a426d2a82f48600c`; later build IDs appear on the page.

Chromium 143, Firefox 144 and Linux WebKit 26 each passed eight checks against the actual HTTPS URL: build identity/runtime consistency, 320/390px layout, touch import/master/live EQ/play/stop, MP3/report/session ZIP downloads and same-origin static GET-only requests. These are desktop engines with phone-sized viewports, not physical iPhone/Android or hardware listening qualification.

All 372 stable files retained their original hashes in the composed artifact. Public verification checked five critical stable files and four preview files after deployment. Nine storage/identity groups verified independent root/preview policies, terminal markers, exact saved recovery data, cross-tab events and locks. The publishing/cache/hash helpers passed 24 tests, including expired-baseline fallback and rejection of stale or unrelated snapshots. Actual long-idle snapshot expiry remains an operational condition covered by fixtures rather than waiting 90 days.

Two earlier broad-CI failures were investigated separately: the live EQ probe now waits for enough actual post-settle PCM rather than assuming a timer delivered it; the native compact time-edit inspector needed to keep its fields and actions together with Linux font metrics. These fixes do not turn preview smoke checks into full native/browser qualification. Read each commit's separate CI results.

The next CI run exposed two further harness mismatches: MP3 integration defaulted
to the unbuilt source directory instead of `web/dist`, and optional-tool discovery
still expected the older 42-tool catalog after `edit_stem_region` was added.
The verifier now defaults to the built site, reports missing WASM before creating
fixtures, and checks the exact current 43-tool set. These are test-harness fixes;
they do not change app behavior or qualify a phone. Read the subsequent run's
results rather than interpreting preview publication as full-suite success.
