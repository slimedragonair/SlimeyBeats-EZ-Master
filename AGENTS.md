# Working branch and checkpoints

Use `current-work` as the active development and integration branch. The owner
uses GitHub while mobile and wants ongoing work available there promptly.

- Inspect the current checkout and remote before switching branches. Preserve
  unrelated work and other contributors' commits; do not force-push by default.
- Commit and push small, coherent checkpoints early and throughout substantial
  tasks. Do not leave completed work only in a local checkout or feature branch
  until the entire task or a long validation suite is finished.
- Run the relevant quick checks before pushing when practical. Mark unfinished
  checkpoints clearly and distinguish passed, running, unrun and device checks.
- Continue broader validation after publishing a checkpoint and promptly push
  resulting fixes. Keep the README's current development status accurate.
- The owner authorized automatic live preview publication from `current-work`
  at `/current-work/`. Preserve the published main files and isolated preview
  storage; see `docs/MOBILE_PREVIEW.md`. Keep preview checks and broader CI
  qualification distinct.
- Preview pushes are not approval to merge app changes into `main` or publish
  an installer. Obtain that authorization from the task/session. The released
  browser app is still built from `main`.

Git branch names cannot contain spaces; `current-work` is the branch requested
as "current work". Native and browser checks run for pushes to this branch.
