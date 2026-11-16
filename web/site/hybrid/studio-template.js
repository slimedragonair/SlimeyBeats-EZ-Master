export const studioTemplate = `
<section id="hybrid-studio" class="hybrid-studio" aria-label="Processing engine">
  <div class="hybrid-switch-row">
    <label class="hybrid-switch" for="hybrid-enabled"><input id="hybrid-enabled" type="checkbox" role="switch" aria-describedby="hybrid-mode-note" aria-controls="hybrid-panel" /><span>Use server processing</span></label>
    <span id="hybrid-availability" role="status" aria-live="polite" aria-atomic="true">[Not checked]</span>
    <button id="hybrid-check" type="button">Check server</button>
    <p id="hybrid-availability-detail" class="small"></p>
  </div>
  <p id="hybrid-mode-note" class="small">Local processing is selected. Turning this on checks the server; it never uploads or masters audio.</p>
  <p id="hybrid-local-note" class="small" hidden></p>
  <div class="hybrid-content">
    <div id="hybrid-panel" hidden>
      <h2>Server master</h2>
      <p>Main Master opens this review. Use the server settings here, then consent and choose Upload &amp; master. Local settings are paused while this switch is on.</p>
      <fieldset id="hybrid-options" disabled>
        <legend>Source and server settings</legend>
        <label for="hybrid-file">Choose a WAV or MP3</label>
        <input id="hybrid-file" type="file" accept=".wav,.mp3,audio/wav,audio/mpeg" aria-describedby="hybrid-file-note" />
        <p id="hybrid-file-note" class="small">Your main file selection is also used here. Server sources bypass local decoding; the advertised server limits apply. Selecting a file does not upload it.</p>
        <p id="hybrid-current-source" class="small"></p>
        <div class="hybrid-grid">
          <label for="hybrid-profile">Server tonal profile<select id="hybrid-profile"></select></label>
          <label for="hybrid-format">Server output format<select id="hybrid-format" aria-describedby="hybrid-download-notice"></select></label>
          <label for="hybrid-mode">Server loudness mode<select id="hybrid-mode"><option value="streaming_safe">Streaming safe</option><option value="loud">Loud</option></select></label>
        </div>
        <p class="small">Profile loudness target · −1 dBTP ceiling · correction intensity 1. These settings apply only to server processing.</p>
        <p id="hybrid-download-notice" class="small"></p>
      </fieldset>
      <details id="hybrid-limits"><summary>Server upload and duration limits</summary><p id="hybrid-limit-details" class="small">Connect to read server limits.</p></details>
      <section aria-label="Review server upload">
        <h3>Review before sending</h3>
        <dl class="hybrid-review">
          <dt>Upload destination</dt><dd id="hybrid-destination"></dd>
          <dt>Selected source</dt><dd id="hybrid-selection">No server file selected</dd>
          <dt>Server storage policy</dt><dd id="hybrid-retention">Connect to read the actual temporary-storage policy and limits.</dd>
        </dl>
        <label class="hybrid-consent" for="hybrid-consent"><input id="hybrid-consent" type="checkbox" disabled /><span>I consent to uploading this selected audio and its filename to the destination shown, for server mastering and temporary storage under the displayed policy. I have permission to send it.</span></label>
        <button id="hybrid-submit" class="primary" type="button" disabled>Upload &amp; master on server</button>
      </section>
    </div>
    <section id="hybrid-job" aria-label="Server job" hidden>
      <h3>Server job and downloads</h3>
      <dl class="hybrid-review">
        <dt>Job</dt><dd id="hybrid-job-id"></dd>
        <dt>Job destination</dt><dd id="hybrid-job-destination"></dd>
        <dt>Job source and settings</dt><dd id="hybrid-job-source"></dd>
        <dt>Scheduled expiry</dt><dd id="hybrid-expiry"></dd>
      </dl>
      <div id="hybrid-unavailable-job" hidden>
        <p id="hybrid-unavailable-note" class="small"></p>
        <button id="hybrid-forget" type="button" aria-describedby="hybrid-unavailable-note hybrid-expiry">Forget unavailable job</button>
      </div>
      <label for="hybrid-progress">Server job progress</label>
      <progress id="hybrid-progress" max="1" value="0"></progress>
      <p id="hybrid-progress-text" class="small"></p>
      <div class="hybrid-actions">
        <button id="hybrid-refresh" type="button" disabled>Check / resume job status</button>
        <button id="hybrid-delete" type="button" disabled>Cancel &amp; delete server files</button>
      </div>
      <p class="small">Switching off keeps access to this job in this tab and stops ongoing upload and status requests. Downloads and deletion remain available. Server work may continue. An expiry time alone does not confirm deletion. Reloading or closing the tab forgets this guest job; delete before leaving for confirmed cleanup.</p>
      <ul id="hybrid-artifacts" aria-label="Server downloads"></ul>
    </section>
    <p id="hybrid-forgotten-note" class="small" role="status" aria-live="polite" hidden></p>
    <div id="hybrid-operator">
      <details id="hybrid-advanced">
        <summary>Advanced operator connection settings</summary>
        <fieldset id="hybrid-connection">
          <legend>Operator connection</legend>
          <label for="hybrid-endpoint">Server address (HTTPS)</label>
          <input id="hybrid-endpoint" type="url" inputmode="url" autocomplete="off" autocapitalize="none" spellcheck="false" aria-describedby="hybrid-endpoint-note" />
          <p id="hybrid-endpoint-note" class="small"></p>
          <label for="hybrid-token">Owner or invite token</label>
          <input id="hybrid-token" type="password" autocomplete="off" autocapitalize="none" spellcheck="false" aria-describedby="hybrid-token-note" />
          <p id="hybrid-token-note" class="small">Kept only in this tab's memory. Never saved in preferences, recovery or links. Disconnect clears the token. An anonymous guest cannot recover an existing job with a newly issued token: download or delete it before disconnecting or reloading.</p>
          <div class="hybrid-actions">
            <button id="hybrid-connect" type="button">Connect with operator token</button>
            <button id="hybrid-disconnect" type="button" disabled>Disconnect &amp; clear token</button>
          </div>
        </fieldset>
      </details>
    </div>
    <p id="hybrid-status" role="status" aria-live="polite" aria-atomic="true"></p>
  </div>
</section>`;
