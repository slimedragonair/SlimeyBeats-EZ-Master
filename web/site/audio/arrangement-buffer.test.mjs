import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
import { ARRANGEMENT_BLOCK_FRAMES as BLOCK, ARRANGEMENT_MAX_QUEUE_BLOCKS as MAX, arrangementQueueBlocks } from './arrangement-buffer.js';
function sink(blocks) {
    let Constructor;
    const context = vm.createContext({ Float32Array, Number, Math, currentFrame: 0, ARRANGEMENT_MAX_QUEUE_BLOCKS: MAX,
        AudioWorkletProcessor: class { constructor() { this.messages = []; this.port = { postMessage: m => this.messages.push(m) }; } },
        registerProcessor: (_, value) => { Constructor = value; } });
    vm.runInContext(readFileSync(new URL('./arrangement-sink.js', import.meta.url), 'utf8').replace(/^import .*;\n/, ''), context);
    const value = new Constructor({ processorOptions: { queueBlocks: blocks } });
    const send = message => value.port.onmessage({ data: { generation: 1, ...message } });
    send({ kind: 'reset', frame: 0 });
    const block = (level = .1, final = false, frames = BLOCK) => send({ kind: 'block', frames, channels: 2, final, buffer: new Float32Array(frames * 2).fill(level).buffer });
    const render = frames => { const samples = []; for (let i = 0; i < frames; i += 128) {
        const outputs = [[new Float32Array(128), new Float32Array(128)]];
        value.process([], outputs); context.currentFrame += 128; samples.push(...outputs[0][0]);
    } return samples; };
    return { value, send, block, render };
}
test('rate-aware scheduling headroom is bounded in PCM memory and live-edit latency', () => {
    for (const rate of [44100, 48000, 88200, 96000, 192000]) {
        const count = arrangementQueueBlocks(rate);
        assert.ok(count <= MAX && count * BLOCK * 2 * 4 <= 524288);
        assert.ok(count * BLOCK / rate >= .32 && count * BLOCK / rate <= .32 + BLOCK / rate);
    }
    assert.equal(arrangementQueueBlocks(48000), 8);
    assert.throws(() => arrangementQueueBlocks(NaN));
});
test('250 ms producer delay preserves sample continuity; old three-block queue fails this same test', () => {
    for (const count of [3, arrangementQueueBlocks(48000)]) {
        const f = sink(count); for (let i = 0; i < count; i++) f.block();
        f.send({ kind: 'run', running: true });
        const pcm = f.render(12032); // Producer cannot service messages during this interval.
        if (count === 3) { assert.ok(pcm.some(x => x === 0)); assert.ok(f.value.underrunFrames > 0); }
        else { assert.ok(pcm.every(x => Math.abs(x - .1) < 1e-7)); assert.equal(f.value.underrunFrames, 0); }
    }
});
test('live gain reaches sink after bounded queued old PCM, with no lost or duplicated source frames', () => {
    const count = arrangementQueueBlocks(48000), f = sink(count);
    for (let i = 0; i < count; i++) f.block(.1);
    f.send({ kind: 'run', running: true });
    const first = f.render(BLOCK); f.block(.05);
    const rest = f.render(count * BLOCK);
    assert.ok(first.every(x => Math.abs(x - .1) < 1e-7));
    assert.ok(rest.slice(0, (count-1)*BLOCK).every(x => Math.abs(x-.1)<1e-7));
    assert.ok(rest.slice((count-1)*BLOCK).every(x => Math.abs(x-.05)<1e-7));
    assert.equal(f.value.frame, (count+1)*BLOCK); assert.equal(f.value.underrunFrames, 0);
});
test('queue limit, generation reset, final partial block, and paused silence remain bounded', () => {
    const f = sink(MAX + 100); for (let i = 0; i < MAX + 5; i++) f.block();
    assert.equal(f.value.queue.length, MAX);
    f.send({ kind: 'reset', generation: 2, frame: 99 });
    f.block(); assert.equal(f.value.queue.length, 0); // stale generation
    f.send({ kind: 'block', generation: 2, frames: 40, channels: 2, final: true, buffer: new Float32Array(80).fill(.25).buffer });
    assert.ok(f.render(128).every(x => x === 0));
    f.send({ kind: 'run', generation: 2, running: true });
    const pcm = f.render(128); assert.ok(pcm.slice(0,40).every(x=>x===.25)); assert.ok(pcm.slice(40).every(x=>x===0));
    assert.equal(f.value.frame,139); assert.equal(f.value.running,false); assert.equal(f.value.underrunFrames,0);
});
