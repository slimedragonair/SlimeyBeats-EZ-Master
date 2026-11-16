# Server processing switch

Local processing is selected on every page load. `studio.js` wires the server
controls independently of local-engine startup. `main-controls.js` bridges only
mode, main-file selection and the primary Master action; it never owns local
decoded audio, player results or recovery storage. Main Master opens server
review when enabled. The panel's **Upload & master on server** is the explicit
transfer action; its own profile, format and loudness controls apply.

`processing-controller.js` acquires a scoped guest session only on opt-in or an
explicit retry. It uses `POST /v1/session` with JSON `{}`, then the existing
authenticated capabilities and job API. Advanced settings retain the operator
token flow. Both use the exact endpoint policy in `endpoint-policy.js`.
`PUBLIC_API_ORIGIN` in that file is the single deployment pin used by health,
the UI default address and address guidance. The policy also preserves the
exact `LEGACY_API_ORIGIN` and this site's HTTPS origin. No host/domain wildcard
is allowed. The deployment owner must update `PUBLIC_API_ORIGIN` and add the
same exact HTTPS origin to `index.html`'s CSP `connect-src` once authenticated;
no provider account, tunnel or hosting setup is started by the switch. The
browser fixture reads the configured UI address instead of hardcoding a host.
No mode or credential is saved.

`server-availability.js` uses unauthenticated `GET /v1/status`, with no audio or
filename. `public-request.js` bounds fetch and response reading to five seconds.
`bounded-json.js` validates JSON MIME and declared size, then reads into one
fixed buffer before UTF-8 decoding and parsing: 16 KiB for status/session and
64 KiB for authenticated capabilities, jobs and upload acknowledgements.
Oversized, malformed or unavailable streams fail closed and are cancelled;
there is no unbounded JSON/body fallback. The public deadline remains active
through body reading and bounded parsing.
Health repeats every 30 seconds only while enabled and visible; hidden pages,
mode changes and newer checks invalidate and abort stale probes. Health never
uploads, starts, resumes or retries an audio job. Busy blocks new submission but
does not interrupt an existing job. Network failure retains the job and clears
consent; recovery requires explicit upload/resume or status refresh.

A reachable API with public processing disabled shows **[Server disabled]**;
incompatible API/setup responses show **[Setup needed]**. Neither is reported as
a network outage. Public-disabled mode blocks guest credentials and audio
submission, including stale in-flight access replies. A private operator can
still explicitly authenticate when public guests are disabled; its availability
detail identifies that private connection and keeps public access qualified.
Health-only recovery to **[Server online]** does not create a guest session.
If no processing session exists, the UI explains **Check & connect** explicitly;
this requests access and capabilities without uploading audio. No public
deployment or uptime is implied by the configured endpoint or these fixtures.

Switching off stops pending upload/poll requests and invalidates their replies,
but retains the authenticated session, job reference, source and expiry. Job
downloads, refresh and deletion remain available. Advanced Disconnect and
pagehide clear credentials. Disconnect cannot recover anonymous ownership with
a newly minted token; the warning recommends download/deletion before leaving.
An expiry is server-reported and does not confirm deletion.

Node tests cover availability, stale responses, consent, mode changes and job
recovery alongside the existing upload/download bounds. The browser verifier
supports `--guest`, viewport dimensions and enlarged text, and exercises actual
Rust WAV/MP3 jobs through an owned fixture. Its exact-public-origin route is a
test adapter, not a tunnel, DNS or live-deployment qualification.
