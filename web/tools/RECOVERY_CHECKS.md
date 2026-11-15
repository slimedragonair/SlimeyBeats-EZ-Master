# Browser recovery and private reporting checks

Run `python web/tools/verify_recovery_browser.py --site <built-site> --out <new-output-folder>`.
Repeat with `--browser firefox` and `--browser webkit`. The harness uses the real
browser workers, decoders, audio exports and IndexedDB. Chromium additionally
crashes a disposable renderer, then restores a completed point in a fresh tab.
The harness does not change installed applications or use personal audio.

The Chromium crash check observes the actual renderer crash event, closes that
target, then waits for its browser-owned Web Lock to be released before restoring.
It never expires or steals a lease. A navigation error or completed `page.close()`
alone is not treated as proof of lock release; live-tab denial is tested separately.

`recovery_store_checks.js` exercises transaction rollback, retention, stale
writers, payload/index separation, binary files, privacy canaries, quota fallback
and asynchronous controller races. `recovery_studio_checks.py` covers actual
editable stem restoration, pending faders, preview equality, and a newer import
arriving during a staged restore. No browser uploads are permitted by the checks.

The site modules separate responsibilities:

- `recovery-format.js` bounds and encodes portable files and PCM descriptors.
- `recovery-store.js` atomically publishes a metadata row and its binary payload;
  `recovery-payload.js` keeps audio out of the lightweight point index. ArrayBuffer
  payloads avoid Blob persistence failures observed in WebKit 26's test context.
- `recovery-controller.js` owns opt-in policy, per-tab identities, completed
  points, revision checks and staged restoration. `recovery-bridge.js` attaches
  the app's two workspaces without making the standalone Help entrypoint depend
  on a working audio engine.
- `master-recovery.js` and `studio-recovery*.js` capture/stage actual workspaces.
  Studio retains an immutable native origin and a maximum 128-operation recipe,
  rebuilds in a separate worker, and checks the exact stored preview before
  adoption. It does not write through native read-only PCM pointers.
- `support-report.js` constructs a typed allowlist of technical context and a
  bounded, best-effort-filtered description. `support-dialog.js` shares only the
  exact preview after explicit Copy/Download. `support.js` boots independently
  of the main DSP app so startup failure reports remain accessible.

Audio storage is off by default, separate from interface localStorage. Recovery
uses three points per tab, 128 MiB per snapshot and 256 MiB / 12 points globally.
Exceeding these limits pauses recovery, not editing. Explicit deletion changes
the policy generation in the same transaction, invalidating older writers.
Each tab writes its own session. Web Locks are never expired or stolen; where
they are unavailable a restore copies data and does not mutate the source.
Restoration requires an empty workspace and never starts playback.

No new full audio reads start during teardown. The last completed point is the
recovery boundary; cleanup/eviction, device memory and mobile hardware remain
separate qualification limits. Keep exported audio as lasting copies.
