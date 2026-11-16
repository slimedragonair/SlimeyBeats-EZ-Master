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
      <p id="hybrid-mode-note">Off: new files process on this device. On: importing uploads the original audio. Analyze, Master, heavy Studio tools and final exports run on the server. Your browser streams previews, with live EQ and faders.</p>
      <p>Use the same Add audio → Analyze → Master → tweak → Export controls. The switch alone never sends audio. Existing server previews still stream when the switch is off; turn it back on to edit them.</p>
      <p id="hybrid-storage" class="small"></p>
      <p id="hybrid-limit-details" class="small">Server admission: up to 1 hour / 2 GiB decoded per source, subject to tool, memory and temporary-storage limits. Neural separation has a separate 512 MiB decoded limit. Local limits still apply with the switch off.</p>
      <p id="hybrid-destination" class="small"></p>
      <p id="hybrid-availability-detail" class="small"></p>
      <p id="hybrid-job-note" class="small" hidden></p>
      <button id="hybrid-delete" type="button" hidden>Delete remaining server files</button>
    </div>
  </details>
  <button id="hybrid-check" type="button" hidden>Retry</button>
  <p id="hybrid-status" class="small" role="status" hidden></p>
</section>`;
