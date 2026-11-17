import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createSessionTrace, traceAdapter, traceFetch, safeMessage, wireSessionTrace } from './session-trace.js';
import { DiagnosticMd5, fileTraceMetadata, detectTraceFormat } from './session-trace-file.js';
test('schema, JSONL retention, safe control ids and errors', () => {
 let time=1000; const t=createSessionTrace({maxEvents:10,now:()=>time,sessionId:'test'});
 t.action('https://secret'); const end=t.begin('edit','server'); time+=20; end('failure',{status:429,code:'arrangement_capacity',token:'secret'}); end();
 t.message('Server arrangement unavailable: token=SECRET HTTP 429 file.wav');
 const rows=t.exportJsonl().trim().split('\n').map(JSON.parse);
 for(const row of rows) assert.deepEqual(Object.keys(row),['session','timestamp','message','extra','level']);
 assert.equal(rows[2].extra.action_sequence,1); assert.equal(rows[3].extra.duration_ms,20);
 assert.equal(rows[4].extra.http_status,429); assert.ok(!t.exportJsonl().includes('SECRET'));
 for(let i=0;i<30;i++) t.action('support-close'); assert.equal(t.snapshot().length,10); assert.equal(JSON.parse(t.exportJsonl().split('\n')[0]).extra.dropped_events,24);
 t.clear(); assert.equal(t.snapshot().length,0);
});
test('known messages retained, raw or malicious text omitted',()=> {
 assert.equal(safeMessage('File selection discarded. Existing tracks and source audio are unchanged.').message_code,'app_message');
 assert.equal(safeMessage('my private file.wav token abc person@example.com').message_code,'redacted');
});
test('adapter preserves private receiver, sync/async types and thrown errors',async()=>{
 const t=createSessionTrace(); class A { #v=3; read(){return this.#v;} async edit(){throw Object.assign(new Error('secret'),{status:429,code:'arrangement_capacity'});} }
 const a=traceAdapter(new A(),'server',t); assert.equal(a.read(),3); await assert.rejects(a.edit(),/secret/); assert.ok(!t.exportJsonl().includes('secret')); assert.equal(t.snapshot().at(-1).extra.error_code,'arrangement_capacity');
});
test('transport preserves responses and never captures URLs, headers or bodies',async()=>{
 const t=createSessionTrace(); const fn=traceFetch(async()=>new Response('secret',{status:429}),t);
 assert.equal(await (await fn('https://secret',{headers:{Authorization:'secret'},body:'private'})).text(),'secret');
 assert.equal(t.snapshot().at(-1).extra.http_status,429); assert.ok(!t.exportJsonl().includes('secret'));
});
test('incremental MD5 matches independent implementation incl boundaries',()=>{
 for(const n of [0,1,55,56,63,64,65,119,120,129,262145]) {const bytes=Uint8Array.from({length:n},(_,i)=>i%251), md=new DiagnosticMd5(); for(let p=0;p<n;p+=13) md.update(bytes.subarray(p,p+13)); assert.equal(md.digest(),createHash('md5').update(bytes).digest('hex')); }
});
test('file metadata distinguishes float WAV from integer WAV and unknown',async()=>{
 const b=new Uint8Array(44),v=new DataView(b.buffer); b.set(new TextEncoder().encode('RIFF'),0);b.set(new TextEncoder().encode('WAVEfmt '),8);v.setUint32(16,16,true);v.setUint16(20,3,true);v.setUint16(34,32,true);
 const meta=await fileTraceMetadata(new Blob([b]),{yieldTask:async()=>{}}); assert.equal(meta.format,'wav 32f');assert.equal(meta.size,44); assert.equal(meta.file_hash,createHash('md5').update(b).digest('hex'));
 v.setUint16(20,1,true);assert.equal(detectTraceFormat(b),'wav 32pcm'); assert.equal(detectTraceFormat(new Uint8Array([1,2,3])),'unknown');
});
test('cancelled hashing cannot repopulate cleared trace',async()=>{
 const t=createSessionTrace();const promise=t.file(new Blob([new Uint8Array(600000)]));t.clear();await promise;assert.equal(t.snapshot().length,0);
});
test('DOM hook captures only identifiers not input values or link URLs and removes listeners',()=>{
 const listeners=new Map();const t=createSessionTrace();const stop=wireSessionTrace({addEventListener:(k,v)=>listeners.set(k,v),removeEventListener:k=>listeners.delete(k)},t);
 listeners.get('click')({type:'click',target:{closest:()=>({id:'support-close',value:'private',getAttribute:()=>null})}});assert.equal(t.snapshot()[0].extra.control,'support-close');stop();assert.equal(listeners.size,0);
});
