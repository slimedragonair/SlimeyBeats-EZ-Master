export const bulkImportTemplate = `
<section class="edit-bulk-import" id="s-stem-drop" aria-label="Import a stem set">
 <h3>Import a whole stem set</h3>
 <p class="small">Choose several files at once, or drop a Suno/stem ZIP here. Matching WAV and MP3 names are one part. Review the selected parts before replacing your Studio audio.</p>
 <div class="edit-actions"><button id="s-bulk-choose" class="primary">Choose stem files</button><button id="s-zip-choose">Choose Suno / stem ZIP</button></div>
 <input id="s-bulk-files" type="file" accept="audio/*,.wav,.mp3,.flac,.m4a" multiple hidden><input id="s-zip-file" type="file" accept=".zip,application/zip" hidden>
 <div id="s-bulk-review" hidden>
  <p id="s-bulk-summary" role="status"></p>
  <label>Preferred encoding<select id="s-bulk-format"><option value="wav">WAV preferred</option><option value="mp3">MP3 preferred</option></select></label>
  <p class="small">Only one encoding of each selected part is imported. If the preferred encoding is missing, its available version is used. Select 2–9 parts; uncheck one before adding a tenth.</p>
  <div id="s-bulk-parts" class="edit-bulk-parts"></div>
  <div class="edit-bulk-commit">
   <p id="s-bulk-action-summary" class="edit-bulk-action-summary" role="status" aria-live="polite" aria-atomic="true"></p>
   <details id="s-bulk-omissions" class="edit-bulk-omissions" hidden><summary id="s-bulk-omitted-summary"></summary><ul id="s-bulk-omitted-parts" aria-label="Parts not selected"></ul></details>
   <div class="edit-actions"><button id="s-import-batch" class="primary">Import selected stems</button><button id="s-bulk-discard">Discard file selection</button></div>
  </div>
 </div>
</section>`;
