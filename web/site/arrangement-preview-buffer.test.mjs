import test from 'node:test';
import assert from 'node:assert/strict';
import { createArrangementPreview } from './arrangement-preview.js';
import { arrangementQueueBlocks } from './audio/arrangement-buffer.js';

test('real preview primes bounded queue before run and refills only consumed slots', async () => {
    const original = { context: globalThis.AudioContext, node: globalThis.AudioWorkletNode };
    let sink; const sent = []; let rendered = 0;
    globalThis.AudioContext = class {
        constructor({sampleRate}) { this.sampleRate=sampleRate; this.currentTime=0; this.state='running'; this.audioWorklet={addModule:async()=>{}}; this.destination={}; }
        createGain() { return {gain:{value:0,setTargetAtTime(){}},connect(){},disconnect(){}}; }
        addEventListener(){} async resume(){} async close(){}
    };
    globalThis.AudioWorkletNode = class {
        constructor(_ctx,_name,options) { this.options=options; this.port={postMessage:m=>sent.push(m)}; sink=this; }
        connect(){} disconnect(){}
    };
    const info = { frames: 480000, prepared_revision: 1, side:'after', position_frame:0 };
    const client = { request: async(action) => {
        if(action==='playback-info' || action==='prepare') return info;
        if(action==='render-block') { rendered++; return {frames:2048,buffer:new ArrayBuffer(2048*2*4)}; }
        throw new Error(action);
    } };
    let transport;
    try {
        transport=createArrangementPreview(client);
        await transport.adopt({revision:1,epoch:'test',arrangement:{sample_rate:48000,channels:2},preview:{frames:480000,identity:'test'}});
        await transport.facade.play();
        const count=arrangementQueueBlocks(48000);
        assert.equal(sink.options.processorOptions.queueBlocks,count);
        assert.equal(transport.facade.max_buffered_frames,count*2048);
        assert.equal(rendered,count);
        const run=sent.findIndex(m=>m.kind==='run' && m.running);
        assert.equal(sent.slice(0,run).filter(m=>m.kind==='block').length,count);
        const generation=sent.filter(m=>m.kind==='reset').at(-1).generation;
        sink.port.onmessage({data:{kind:'consumed',generation,frame:2048,buffer:new ArrayBuffer(16384)}});
        await new Promise(resolve=>setImmediate(resolve)); assert.equal(rendered,count+1);
        transport.facade.stop();
        sink.port.onmessage({data:{kind:'consumed',generation,frame:4096,buffer:new ArrayBuffer(16384)}});
        await new Promise(resolve=>setImmediate(resolve)); assert.equal(rendered,count+1);
        assert.equal(transport.facade.playing,false); assert.equal(transport.facade.positionFrame(),0);
    } finally { transport?.facade.clear(); globalThis.AudioContext=original.context; globalThis.AudioWorkletNode=original.node; }
});

test('audio audit distinguishes a fresh generation from queued live edits', async () => {
    const { readFile } = await import('node:fs/promises');
    const { runInNewContext } = await import('node:vm');
    class AudioNode { connect() {} }
    class Anchor { click() {} }
    const scope = { window: { Worker: class {}, AudioWorkletNode: class {} },
        URL: { createObjectURL() {} }, HTMLAnchorElement: Anchor, AudioNode };
    runInNewContext(await readFile(new URL('../tools/arrangement_audio_audit.js', import.meta.url), 'utf8'), scope);
    const audit = scope.window.arrangementAudit;
    audit.context = { currentTime: 2, sampleRate: 48000 };
    audit.maxBufferedFrames = 16384;
    audit.reset(18000);
    assert.equal(audit.after, 2 + (16384 / 48000 + .003));
    audit.reset(18000, {freshPlayback:true});
    assert.equal(audit.after, 2.08);
    assert.equal(audit.limitFrames, 18000);
    assert.equal(audit.frames, 0);
    assert.equal(audit.frozen, false);
    audit.reset(18000); // Fresh mode is not sticky for subsequent live edits.
    assert.equal(audit.after, 2 + (16384 / 48000 + .003));
});
