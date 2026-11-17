# Future owner-controlled admission drain

Development implementation only. It cannot be activated in an already-running server that predates this feature by changing files.

Authenticated **owner only**:

- `POST /v1/maintenance/drain` with JSON `{"enabled":true}` closes admission.
- `GET /v1/maintenance/drain` returns only drain state and retained job/project counts.
- `POST` with `{"enabled":false}` reopens admission.

Do not put bearer credentials in command lines, logs, URLs or scripts. Use the existing protected owner authentication mechanism.

Admission changes share the exact job/project creation lock. After enabling returns, previously started creates are either already retained and counted or rejected. New guest-session, legacy-job and v2-project requests return 503 `maintenance_draining` with Retry-After 60. Existing authentication, project uploads, processing, downloads and owner deletion remain available; the gate does not set the worker's stopping flag, cancel work, delete work, or change retention. Idempotent retries of an already-admitted legacy job remain available. The gate blocks owners too, so an owner cannot accidentally create work between inspection and restart.

The response is NOT an automatic restart approval or a claim about active HTTP responses. Allow users to finish/export, wait for ordinary expiry, confirm both retained counts are zero, inspect the fail-closed disk inventory, and wait for existing downloads/request handlers to settle. Keep the drain enabled throughout. If any check fails, defer; never delete retained work to make checks pass. Gate state is in memory and resets after service restart; keep ingress held during maintenance/rollback if that is part of the approved procedure. Cancelling maintenance simply disables the gate.

## First rollout on a server without this feature

No safe hot insertion is available. The current Super Server has Tailscale Funnel directly forwarding to the app, not a configurable per-route reverse proxy. Do not attach a debugger, mutate Python memory, or assume writing new source changes the running code.

Least-complex first rollout requires explicit approval for a brief public maintenance interruption **and the concrete Funnel routing/access change**. Proposed sequence, not executed:

1. Wait for retained work to naturally reach zero, with no processing/downloads observed. Prepare tested code and rollback source separately first.
2. After approval, temporarily suspend public ingress using the existing Funnel control, without killing/restarting the app. This may disconnect an in-flight request; explain that risk in the approval.
3. Let already-admitted app requests settle, then repeat retained counts and fail-closed inventory. If any work appeared in the gap, do not restart or delete it. Restore ingress or wait for its normal expiry within the approved maintenance window.
4. Only with ingress held and no retained work/processing/in-flight app requests, switch source, apply the separately approved memory settings, restart, validate loopback health and capabilities, then restore the original public routing and verify it.
5. If startup/validation fails, roll back while ingress remains held. Do not attempt rollback after reopening public access without draining again.

A second option is an independently vetted streaming proxy that blocks only the three create routes while passing existing requests. Inserting it still requires routing approval and proof that switching Funnel targets preserves existing streams. This has NOT been proven or implemented; do not present it as zero-downtime.

The new drain endpoint makes subsequent planned maintenance safer; it does not eliminate the first-rollout authorization requirement.
