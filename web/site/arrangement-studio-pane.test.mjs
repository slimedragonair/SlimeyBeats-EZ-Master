import test from 'node:test';
import assert from 'node:assert/strict';
import { createArrangementStudioPane } from './arrangement-studio-pane.js';

test('pane locks legacy controls immediately through deferred first read and releases on failure', async () => {
    let rejectRead, reads = 0, clears = 0, changes = [];
    const adapter = { read() { reads++; return new Promise((resolve,reject) => { rejectRead=reject; }); }, clear() { clears++; } };
    const pane = createArrangementStudioPane({ mode:'local', createAdapter:()=>adapter,
        host:null, volume:()=>1, pauseMain(){}, changed(){changes.push(pane.busy);}, notice(){} });
    assert.equal(pane.busy,false);
    const first=pane.initialise();
    assert.equal(pane.busy,true, 'must not advertise an enabled export window before the first read finishes');
    assert.equal(changes[0],true, 'notify the host of the lock before yielding');
    const second=pane.initialise();
    assert.equal(reads,1,'repeated activation shares the existing opening');
    rejectRead(new Error('expected fixture failure'));
    const results=await Promise.allSettled([first,second]);
    assert.ok(results.every(value=>value.status==='rejected'));
    assert.equal(pane.busy,false); assert.equal(changes.at(-1),false); assert.equal(clears,1);
    const retry=pane.initialise(); assert.equal(pane.busy,true); assert.equal(reads,2);
    rejectRead(new Error('retry fixture failure')); await assert.rejects(retry);
    assert.equal(pane.busy,false);
});
