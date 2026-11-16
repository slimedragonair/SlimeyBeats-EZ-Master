export const studioTemplate = `
<section id="hybrid-studio" class="hybrid-studio" aria-label="Mastering engine">
  <label class="hybrid-switch" for="hybrid-enabled">
    <input id="hybrid-enabled" type="checkbox" role="switch" aria-describedby="hybrid-mode-note" />
    <span>Server processing</span>
  </label>
  <span id="hybrid-availability" role="status" aria-live="polite" aria-atomic="true">Local</span>
  <details id="hybrid-details" class="hybrid-details">
    <summary aria-label="About server processing" title="How server processing works">ⓘ</summary>
    <div class="hybrid-info">
      <p id="hybrid-mode-note">Off: audio stays on this device. On: Master sends decoded audio to the server, then loads the result here. Analyze, live EQ and exports stay on your device.</p>
      <p>Use the same Add audio → Analyze → Master → tweak → Export controls. The switch alone never sends audio. Studio tools still run locally.</p>
      <p id="hybrid-storage" class="small"></p>
      <p id="hybrid-limit-details" class="small">The editor's normal duration and memory limits still apply.</p>
      <p id="hybrid-destination" class="small"></p>
      <p id="hybrid-availability-detail" class="small"></p>
      <p id="hybrid-job-note" class="small" hidden></p>
      <button id="hybrid-delete" type="button" hidden>Delete remaining server files</button>
    </div>
  </details>
  <button id="hybrid-check" type="button" hidden>Retry</button>
  <p id="hybrid-status" class="small" role="status" hidden></p>
</section>`;
