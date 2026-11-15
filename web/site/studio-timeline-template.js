export const timelineTemplate = `
<section class="s-timeline" aria-labelledby="s-editor-heading">
 <header class="s-time-heading"><h3 id="s-editor-heading">Time edits</h3><span class="s-time-badge">Raw stems · seconds</span></header>
 <p id="s-time-help" class="s-time-help">Choose a stem and a range, then run a tool. Click a waveform to seek; drag horizontally to select. Typing times works too.</p>
 <label class="s-time-stem" for="s-editor-stem">Edit this stem<select id="s-editor-stem" aria-describedby="s-time-reason"></select></label>
 <div class="s-time-navigation" role="group" aria-label="Timeline view">
  <button type="button" id="s-time-fit">Fit song</button><button type="button" id="s-time-fit-selection">Fit selection</button>
  <button type="button" id="s-time-zoom-in">Zoom in</button><button type="button" id="s-time-zoom-out">Zoom out</button>
  <button type="button" id="s-time-left">Earlier</button><button type="button" id="s-time-right">Later</button>
 </div>
 <p id="s-time-view" class="s-time-readout"></p>
 <div id="s-time-lanes" class="s-time-lanes" role="region" aria-label="Aligned raw stem waveforms" aria-describedby="s-time-help" tabindex="0">
  <div class="s-time-ruler"><canvas aria-hidden="true" width="1" height="1"></canvas><span class="s-time-playhead" hidden></span></div>
  <div class="s-time-rows"></div>
 </div>
 <p class="s-time-help">Raw sample amplitude, ±1 full scale. Faders and peak protection are not shown. All parts keep their position and length.</p>
 <div class="s-time-fields">
  <label for="s-edit-start">Selection start / s<input id="s-edit-start" type="number" inputmode="decimal" min="0" value="0" aria-describedby="s-time-selection s-time-reason"></label>
  <label for="s-edit-end">Selection end / s<input id="s-edit-end" type="number" inputmode="decimal" min="0" value="0" aria-describedby="s-time-selection s-time-reason"></label>
 </div>
 <p id="s-time-selection" class="s-time-readout">Select at least one frame.</p>
 <button type="button" id="s-time-seek-selection">Seek to selection start</button>
 <p id="s-time-reason" class="s-time-reason" role="status" aria-live="polite"></p>
 <div class="s-time-tools" role="group" aria-label="Apply an edit to the selected stem range">
  <button type="button" id="s-edit-silence" title="Replace selected frames with silence; keep duration.">Silence</button>
  <button type="button" id="s-edit-fade-in" title="Linear fade from silence to the original level. A one-frame fade is silent.">Fade in</button>
  <button type="button" id="s-edit-fade-out" title="Linear fade from the original level to silence. A one-frame fade is silent.">Fade out</button>
  <button type="button" id="s-edit-reverse" title="Reverse selected frames, preserving stereo channel positions.">Reverse</button>
 </div>
 <div class="s-time-gain">
  <label for="s-edit-gain">Selection gain / dB<input id="s-edit-gain" type="number" inputmode="decimal" min="-60" max="12" step="0.5" value="0" aria-describedby="s-time-gain-help s-time-gain-error"></label>
  <button type="button" id="s-edit-apply-gain">Apply gain</button>
 </div>
 <p id="s-time-gain-error" class="s-time-reason" role="status" aria-live="polite"></p>
 <p id="s-time-gain-help" class="s-time-help">−60 to +12 dB; 0 dB keeps the level. Gain can clip; audition the result. Select or type without rendering. Only the tool buttons apply edits; Studio Undo restores the previous revision.</p>
</section>`;
