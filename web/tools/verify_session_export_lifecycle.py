"""Regression-check packaging cleanup with a fake Worker and real structuredClone.

This exercises the current client module in Node, without browser processes,
WASM, audio, sleeps, or generated files. Actual ZIP contents have separate tests.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]

PROBE = r"""
const {packageSession} = await import(process.argv[1]);
const native = {Worker: globalThis.Worker, setTimeout: globalThis.setTimeout, clearTimeout: globalThis.clearTimeout};
const workers = new Set(), timers = new Map(), signals = [];
let lastWorker, created = 0, terminateCalls = 0, timerId = 0, failConstructor = false;
class FakeWorker {
    constructor() {
        if (failConstructor) throw new Error('Worker constructor failed');
        created++;
        workers.add(this);
        lastWorker = this;
    }
    postMessage(message, transfer) {
        // Exercise actual DataCloneError and transfer-list semantics, rather than
        // merely assuming that the client calls a mocked exception correctly.
        this.transferCount = transfer.length;
        this.message = structuredClone(message, {transfer});
    }
    terminate() { terminateCalls++; workers.delete(this); }
}
globalThis.Worker = FakeWorker;
globalThis.setTimeout = (callback, milliseconds) => {
    const id = ++timerId;
    timers.set(id, {callback, milliseconds});
    return id;
};
globalThis.clearTimeout = id => timers.delete(id);
function controller() {
    const result = new AbortController(), listeners = new Set();
    const add = result.signal.addEventListener.bind(result.signal);
    const remove = result.signal.removeEventListener.bind(result.signal);
    result.signal.addEventListener = (name, callback, options) => {
        if (name === 'abort') listeners.add(callback);
        add(name, callback, options);
    };
    result.signal.removeEventListener = (name, callback, options) => {
        if (name === 'abort') listeners.delete(callback);
        remove(name, callback, options);
    };
    signals.push(listeners);
    return result;
}
function counts() {
    return {active_workers: workers.size, active_timeouts: timers.size,
        abort_listeners: signals.reduce((sum, listeners) => sum + listeners.size, 0), created, terminate_calls: terminateCalls};
}
function assert(value, message) { if (!value) throw new Error(message); }
function clean() {
    const current = counts();
    assert(current.active_workers === 0 && current.active_timeouts === 0 && current.abort_listeners === 0,
        'Leaked lifecycle resources: ' + JSON.stringify(current));
}
async function observe(promise) {
    let watchdog;
    try {
        return await Promise.race([
            promise.then(value => ({kind: 'resolved', value}), error => ({kind: 'rejected', error})),
            new Promise(resolve => { watchdog = native.setTimeout(() => resolve({kind: 'pending'}), 100); }),
        ]);
    } finally { native.clearTimeout(watchdog); }
}
async function rejected(promise, expected) {
    const result = await observe(promise);
    assert(result.kind === 'rejected' && expected.test(String(result.error)),
        `Expected rejection ${expected}; received ${result.kind}: ${result.error}`);
}
const payload = () => ({audio: {name: 'master.wav', bytes: new Uint8Array([1, 2, 3])}, files: []});
function pending(workspace = 'mastering') {
    const abort = controller(), value = payload();
    const promise = packageSession(workspace, value, {signal: abort.signal});
    assert(workers.size === 1 && timers.size === 1 && signals[0].size === 1, 'Missing active lifecycle resources');
    return {promise, abort, value, worker: lastWorker};
}
const cases = [
    ['null_payload', async () => { await rejected(packageSession('mastering', null), /payload/); }],
    ['malformed_files', async () => { await rejected(packageSession('studio', {files: {}}), /payload/); }],
    ['malformed_file_entry', async () => {
        const abort = controller();
        await rejected(packageSession('studio', {files: [null]}, {signal: abort.signal}), /TypeError|payload/);
    }],
    ['unknown_workspace', async () => { await rejected(packageSession('unknown', payload()), /payload|workspace/); }],
    ['message_clone_failure', async () => {
        const abort = controller(), value = {...payload(), uncloneable: () => {}};
        await rejected(packageSession('mastering', value, {signal: abort.signal}), /DataCloneError/);
    }],
    ['normal_worker_result', async () => {
        const run = pending(), blob = new Blob(['ZIP fixture'], {type: 'application/zip'});
        run.worker.onmessage({data: {blob}});
        const result = await observe(run.promise);
        assert(result.kind === 'resolved' && result.value === blob, 'Expected the worker Blob');
        assert(run.value.audio.bytes.byteLength === 0, 'Disposable exported buffer was not transferred');
    }],
    ['abort_before_start', async () => {
        const abort = controller(); abort.abort();
        await rejected(packageSession('mastering', payload(), {signal: abort.signal}), /AbortError/);
        assert(created === 0, 'Pre-aborted request created a worker');
    }],
    ['abort_during_packaging', async () => {
        const run = pending(); run.abort.abort();
        await rejected(run.promise, /AbortError/);
    }],
    ['worker_error_result', async () => {
        const run = pending(); run.worker.onmessage({data: {error: 'ZIP validation rejected payload'}});
        await rejected(run.promise, /ZIP validation rejected/);
    }],
    ['worker_runtime_error', async () => {
        const run = pending(); run.worker.onerror({message: 'Worker crashed'});
        await rejected(run.promise, /Worker crashed/);
    }],
    ['worker_message_deserialization_failure', async () => {
        const run = pending(); run.worker.onmessageerror({});
        await rejected(run.promise, /unreadable data/);
    }],
    ['invalid_worker_result', async () => {
        const run = pending(); run.worker.onmessage({data: {blob: {}}});
        await rejected(run.promise, /Invalid session download/);
    }],
    ['packaging_timeout', async () => {
        const run = pending();
        const [id, timeout] = timers.entries().next().value;
        assert(timeout.milliseconds === 120000, 'Unexpected packaging deadline');
        timers.delete(id); timeout.callback();
        await rejected(run.promise, /timed out/);
    }],
    ['late_result_after_abort', async () => {
        const run = pending(); run.abort.abort();
        run.worker.onmessage({data: {blob: new Blob(['late'])}});
        await rejected(run.promise, /AbortError/);
        assert(terminateCalls === 1, 'Late result settled cleanup twice');
    }],
    ['duplicate_result_and_late_abort', async () => {
        const run = pending('studio'), blob = new Blob(['first']);
        run.worker.onmessage({data: {blob}});
        run.worker.onmessage({data: {error: 'late'}}); run.abort.abort();
        const result = await observe(run.promise);
        assert(result.kind === 'resolved' && result.value === blob && terminateCalls === 1, 'Completion was not idempotent');
    }],
    ['shared_buffer_transfer_deduplicated', async () => {
        const bytes = new Uint8Array([1, 2, 3, 4]);
        const promise = packageSession('studio', {audio: {bytes: bytes.subarray(0, 2)}, files: [{bytes: bytes.subarray(2)}]});
        assert(lastWorker.transferCount === 1, 'Duplicate ArrayBuffer in transfer list');
        lastWorker.onmessage({data: {blob: new Blob(['zip'])}});
        assert((await observe(promise)).kind === 'resolved', 'Deduplicated transfer did not finish');
    }],
    ['worker_constructor_failure', async () => {
        failConstructor = true;
        await rejected(packageSession('mastering', payload()), /constructor failed/);
    }],
];
const results = [];
try {
    for (const [name, run] of cases) {
        created = terminateCalls = 0; failConstructor = false; lastWorker = undefined;
        try {
            await run(); clean();
            results.push({name, passed: true, ...counts()});
        } catch (error) { results.push({name, passed: false, error: String(error), ...counts()}); }
        finally {
            // A failed assertion must not contaminate the next independent case.
            workers.clear(); timers.clear(); signals.length = 0;
        }
    }
} finally { Object.assign(globalThis, native); }
console.log(JSON.stringify({passed: results.every(result => result.passed), cases: results}, null, 2));
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--client', type=Path, default=ROOT / 'web/site/session-export-client.js')
    args = parser.parse_args()
    client = args.client.resolve(strict=True)
    completed = subprocess.run(['node', '--input-type=module', '--eval', PROBE, client.as_uri()],
        capture_output=True, text=True, timeout=15, check=True)
    result = json.loads(completed.stdout)
    result['runtime'] = subprocess.run(['node', '--version'], capture_output=True, text=True, check=True).stdout.strip()
    print(json.dumps(result, indent=2))
    if not result['passed']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
