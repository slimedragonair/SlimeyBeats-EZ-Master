export const arrangementTimelineTemplate = `
<section class="arrangement-timeline" data-arr="workspace" aria-label="Multitrack arrangement" hidden>
 <header class="arr-heading"><div><h3>Arrange your tracks</h3><p data-arr="layout" class="small"></p></div>
  <div class="arr-actions"><button type="button" data-arr="add-files" class="primary">Add tracks</button><button type="button" data-arr="add-zip">Add ZIP</button></div>
 </header>
 <input data-arr="files" type="file" accept=".wav,.mp3,audio/wav,audio/mpeg" multiple hidden>
 <input data-arr="zip" type="file" accept=".zip,application/zip" hidden>
 <p data-arr="status" class="arr-status" role="status" aria-live="polite">Add tracks to begin. Source audio stays intact.</p>
 <p data-arr="server-buffering" class="small" hidden>Server preview is buffered. Applied edits may take a few seconds to reach the audio.</p>
 <p data-arr="persistence-note" class="small">Keep this page open to retain your arrangement. Legacy recovery copies do not include these tracks; project saving is not available yet.</p>
 <details data-arr="file-actions" hidden><summary>Save, export &amp; open</summary>
 <div data-arr="project-actions" class="arr-actions" hidden><button type="button" data-arr="project-save">Save project</button><button type="button" data-arr="project-open">Open project</button><span class="small">Project ZIP keeps applied edits, source assets and Undo history.</span></div>
 <input data-arr="project-file" type="file" accept=".zip,application/zip" hidden>
 <div data-arr="audio-actions" class="arr-actions" hidden><label>Delivery format<select data-arr="audio-format"><option value="wav24">24-bit WAV</option><option value="wav16">16-bit WAV</option><option value="wav32f">32-bit float WAV</option></select></label><button type="button" data-arr="audio-export">Export whole arrangement</button></div>
 <p data-arr="mp3-note" class="small" hidden>Uses the current render. MP3 is checked after encoding and may be attenuated for peak safety. No mastering pass is run.</p>
 </details>
 <section data-arr="import-review" class="arr-import" aria-label="Review tracks to add" hidden>
  <h4>Review your tracks</h4><p data-arr="import-summary" class="small"></p>
  <div class="arr-actions"><button type="button" data-arr="import-all">Select all</button><button type="button" data-arr="import-none">Select none</button>
   <label>Preferred version<select data-arr="import-format"><option value="wav">WAV</option><option value="mp3">MP3</option></select></label></div>
  <div data-arr="import-parts"></div><p data-arr="import-omissions" class="small"></p>
  <div class="arr-actions"><button type="button" data-arr="import-commit" class="primary">Add selected tracks</button><button type="button" data-arr="import-cancel">Discard selection</button></div>
 </section>
 <div class="arr-actions arr-toolbar" role="group" aria-label="Arrangement navigation and history">
  <button type="button" data-arr="mode-move" aria-pressed="true">Move clips</button><button type="button" data-arr="mode-select" aria-pressed="false">Select range</button>
  <button type="button" data-arr="fit">Fit song</button><button type="button" data-arr="fit-selection">Fit selection</button><button type="button" data-arr="zoom-in">Zoom in</button><button type="button" data-arr="zoom-out">Zoom out</button>
  <button type="button" data-arr="earlier">Earlier</button><button type="button" data-arr="later">Later</button>
  <button type="button" data-arr="undo" disabled>Undo</button><button type="button" data-arr="redo" disabled>Redo</button>
 </div>
 <p class="small">Each track has its own lane. Drag a clip to move it or use its edge handles to trim. Exact times below also work with touch and keyboard. Release applies one edit; Escape cancels the drag.</p>
 <div class="arr-fields"><label>Selected track<select data-arr="lane-select"></select></label><label>Selected clip<select data-arr="clip-select"></select></label></div>
 <div class="arr-lanes" data-arr="lanes" role="region" aria-label="Arrangement track and clip waveforms" tabindex="0">
  <div class="arr-ruler"><span>Tracks</span><canvas data-arr="ruler" aria-hidden="true"></canvas></div><div data-arr="rows"></div>
 </div>
 <p data-arr="selection" class="arr-readout"></p>
 <p class="small">Source waveforms show raw sample amplitude. Track volume and EQ are separate. Clip ends and selection ends are exclusive.</p>
 <div class="arr-inspectors">
  <details data-arr="clip-inspector" open><summary>Selected clip · exact timing</summary>
   <p data-arr="clip-label" class="small">Select a clip.</p><div class="arr-fields">
    <label>Clip name<input data-arr="clip-name" type="text" maxlength="512" placeholder="Use source name"></label>
    <label>Track<select data-arr="clip-lane"></select></label>
    <label>Start in song / s<input data-arr="clip-start" type="number" min="0" step="any" inputmode="decimal"></label>
    <label>Start in source / s<input data-arr="clip-source" type="number" min="0" step="any" inputmode="decimal"></label>
    <label>End in song / s<input data-arr="clip-end" type="number" min="0" step="any" inputmode="decimal"></label>
    <label>Clip gain / dB<input data-arr="clip-gain" type="number" min="-60" max="12" step="0.25" inputmode="decimal"></label>
    <label>Nudge / s<input data-arr="nudge" type="number" min="0" value="0.1" step="any" inputmode="decimal"></label>
    <label>Fade in / s<input data-arr="fade-in" type="number" min="0" step="any" inputmode="decimal"></label>
    <label>Fade out / s<input data-arr="fade-out" type="number" min="0" step="any" inputmode="decimal"></label>
   </div>
   <p data-arr="clip-error" class="arr-error" role="status"></p>
   <div class="arr-actions"><button type="button" data-arr="clip-timing">Update timing</button><button type="button" data-arr="clip-level">Update name / gain / fades</button><button type="button" data-arr="clip-reload">Reload saved clip values</button>
    <button type="button" data-arr="nudge-left">Move earlier</button><button type="button" data-arr="nudge-right">Move later</button>
    <button type="button" data-arr="split">Split at playhead</button><button type="button" data-arr="duplicate">Duplicate after clip</button><button type="button" data-arr="delete-clip">Delete clip</button></div>
  </details>
  <details data-arr="lane-inspector"><summary>Selected track</summary><div class="arr-fields">
   <label>Name<input data-arr="lane-name" type="text" maxlength="512"></label>
   <label>Track gain / dB<input data-arr="lane-gain" type="number" min="-60" max="12" step="0.25" inputmode="decimal"></label>
   <label>Balance · −1 left / +1 right<input data-arr="lane-pan" type="number" min="-1" max="1" step="0.05" inputmode="decimal"></label>
   <label class="arr-check"><input data-arr="lane-mute" type="checkbox">Mute track</label><label class="arr-check"><input data-arr="lane-solo" type="checkbox">Solo track</label>
  </div><div class="arr-actions"><button type="button" data-arr="lane-update">Update track</button><button type="button" data-arr="lane-reload">Reload saved track values</button><button type="button" data-arr="lane-up">Move track up</button><button type="button" data-arr="lane-down">Move track down</button><button type="button" data-arr="delete-lane">Delete track</button></div></details>
 </div>
 <section class="arr-eq" data-arr="eq" aria-label="EQ for an explicit arrangement target">
  <h4>Shape this part</h4><div class="arr-fields">
   <label>EQ scope<select data-arr="eq-scope"><option value="mix">Whole mix</option><option value="lane">Selected track</option><option value="clip">Selected clip</option><option value="range">Time range</option></select></label>
   <label data-arr="range-target-label" hidden>Range affects<select data-arr="eq-range-target"><option value="lane">Selected track</option><option value="clip">Selected clip</option><option value="mix">Whole mix</option></select></label>
   <label>Range start / s<input data-arr="range-start" type="number" min="0" step="any" inputmode="decimal"></label><label>Range end / s<input data-arr="range-end" type="number" min="0" step="any" inputmode="decimal"></label>
  </div>
  <p data-arr="eq-target" class="arr-target" role="status"></p>
  <canvas data-arr="eq-plot" class="arr-eq-plot" aria-label="Current and proposed EQ response in decibels"></canvas>
  <p class="small"><span class="arr-current-key">━ Current EQ</span> · <span class="arr-proposed-key">┄ Proposed · not applied</span></p>
  <div class="arr-fields"><label>EQ effect<select data-arr="eq-effect"></select></label><label>Band<select data-arr="eq-band"></select></label>
   <label>Gain / dB<input data-arr="eq-gain" type="number" min="-18" max="18" step="0.25" inputmode="decimal"></label>
   <label>Frequency / Hz<input data-arr="eq-frequency" type="number" min="20" max="20000" inputmode="decimal"></label>
   <label>Q / width<input data-arr="eq-q" type="number" min="0.1" max="18" step="0.05" inputmode="decimal"></label>
   <label>Shape<select data-arr="eq-shape"><option value="bell">Bell</option><option value="low_shelf">Low shelf</option><option value="high_shelf">High shelf</option><option value="low_cut">Low cut</option><option value="high_cut">High cut</option></select></label>
   <label>EQ output / dB<input data-arr="eq-output" type="number" min="-24" max="12" step="0.25" inputmode="decimal"></label>
   <label class="arr-check"><input data-arr="eq-enabled" type="checkbox" checked>Enable band</label><label class="arr-check"><input data-arr="eq-bypass" type="checkbox">Bypass this EQ effect</label>
  </div>
  <div class="arr-actions"><button type="button" data-arr="eq-suggest">Suggest EQ</button><button type="button" data-arr="eq-apply-proposal" hidden>Apply proposed EQ</button><button type="button" data-arr="eq-update">Update EQ preview</button><button type="button" data-arr="eq-reload">Reload saved EQ values</button><button type="button" data-arr="eq-remove">Remove this EQ</button></div>
  <details class="small" data-arr="eq-details"><summary data-arr="eq-note">Review an EQ proposal before applying it.</summary><p data-arr="eq-notes"></p></details>
 </section>
</section>`;
