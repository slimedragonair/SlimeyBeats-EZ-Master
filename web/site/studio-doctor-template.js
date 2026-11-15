export const doctorTemplate = `
<section id="s-doctor-page" aria-labelledby="s-doctor-heading" hidden>
 <h3 id="s-doctor-heading">Stem Doctor</h3>
 <p>Find moments worth listening to in the original, saved preview, or a selected stem. Scanning only measures audio; it never changes it.</p>
 <p id="s-doctor-target" class="doctor-target"></p>
 <div class="edit-actions"><button id="s-doctor-scan" class="primary" disabled>Scan issues</button><button id="s-doctor-download" disabled>Download issue report JSON</button></div>
 <p id="s-doctor-status" class="small" role="status" aria-live="polite">Choose a source, then scan.</p>
 <p class="small">Silence can be valid: nothing is automatically trimmed or cut. Broad bass can be intentional. These conservative flags can miss problems and do not replace listening.</p>
 <ol id="s-doctor-issues" class="doctor-issues" aria-label="Timestamped audio issues"></ol>
 <details id="s-doctor-measurements" hidden><summary>Scan measurements and limits</summary><pre id="s-doctor-limits"></pre><ul id="s-doctor-notes" class="small"></ul></details>
 <div class="doctor-repair">
  <label for="s-doctor-strength">Fix strength <output id="s-doctor-strength-value" for="s-doctor-strength">50%</output></label>
  <input id="s-doctor-strength" type="range" min="0" max="1" step="0.05" value="0.5">
  <button id="s-doctor-fix" class="primary" disabled>Preview selected fix</button>
  <p id="s-doctor-fix-help" class="small">Select an issue to listen. Only supported repairs offer a preview. No automatic fix-all or missing-audio reconstruction.</p>
  <p class="small">Compare A Before with B Preview and use Undo if needed. A stem fix first makes a separate preview; use “Apply edit to selected stem” to update its bank. Downloaded reports are metadata snapshots, separate from session ZIPs and recovery.</p>
 </div>
</section>`;

export const doctorWaveOverlay = `<div class="doctor-wave-surface"><canvas id="s-wave" width="640" height="100" aria-label="Before and preview waveform"></canvas><div id="s-doctor-markers" class="doctor-markers" aria-label="Issue markers over waveform"></div></div><ul id="s-doctor-legend" class="doctor-legend" aria-label="Issue marker colors" hidden><li class="doctor-key-clipping">Clipping candidate</li><li class="doctor-key-silence">Silence</li><li class="doctor-key-ending">Possible ending</li><li class="doctor-key-low-mids">Low-mid buildup</li></ul><p id="s-doctor-marker-label" class="small" hidden></p>`;
