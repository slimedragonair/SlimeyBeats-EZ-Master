# Retained-work deployment preflight

Run `python3 hybrid/deploy/validate_deployment.py retained-work --storage PATH` as an account allowed to inspect the storage directory. Output contains counts only, not project IDs, paths, uploads or audio. Nonzero exit means **do not interrupt**.

Storage creates `job-<32 hexadecimal characters>` directories. Matching only bare hexadecimal directory names reports zero even when work exists. The shared inventory counts every directory and refuses unknown files, links, missing/unreadable storage or inspection races. It never deletes work.

**An empty inventory is not an admission lock or permission to restart.** New projects can arrive immediately afterward. Before using it as a deployment preflight, establish a separately authorized admission-drain mechanism that rejects new sessions/projects without interrupting existing requests. Then allow existing work to expire normally or have its owner export it, repeat the preflight, and verify no requests/workers remain. If no safe drain exists, defer deployment. Do not stop the public listener or change firewall/access rules as an unapproved substitute.

The server currently keeps projects in memory and deletes retained storage during normal shutdown; startup also cleans stale owned directories. Copying storage alone does not provide a supported project restore. An idle process list or old file timestamps does not prove work is disposable. Do not shorten retention, kill workers, delete projects, or restart merely to clear capacity.

The deployed private operations helper is outside this repository. Operators must replace its bare-hex-only count with this fail-closed preflight and retain a separate admission barrier before using restart/stop/activate. This module does not silently patch that helper or deploy anything.
