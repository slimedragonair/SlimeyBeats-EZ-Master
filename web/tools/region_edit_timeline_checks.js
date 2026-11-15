import { assert, equal } from "./region_edit_pcm_checks.js";

// Exercise production state/envelopes directly, independently of DOM gesture tests.
export async function verifyTimelineEdges(clientUrl) {
    const sibling = name => { const url = new URL(name, clientUrl); url.search = new URL(clientUrl).search; return url.href; };
    const { TimelineState } = await import(sibling("./studio-timeline-state.js"));
    const { createEnvelopeCache } = await import(sibling("./studio-timeline-envelope.js"));
    const model = new TimelineState(), rate = 8000;
    model.configure({ sampleRate: rate, length: 8001 });
    assert(model.setSeconds(.5 / rate, 1.5 / rate) === "", "Half-frame selection rejected");
    assert(model.start === 1 && model.end === 2, "Selection must round half frames upward");
    const edit = model.edit({ kind: "silence" });
    assert(edit.start_seconds === 1 / rate && edit.end_seconds === 2 / rate, "One-frame/end-exclusive request changed");
    const selection = [model.start, model.end];
    model.fitSelection();
    assert(model.span === 80, "Fit one-frame selection must use 10 ms minimum view");
    for (let i = 0; i < 32; i++) model.zoom(.5);
    assert(model.span === 80 && model.left >= 0, "Zoom crossed its 10 ms floor");
    model.setView(8000, 80);
    assert(model.right === 8001 && model.left === 7921, "View must clamp at exclusive EOF");
    model.pan(1);
    assert(model.right === 8001, "Later pan escaped EOF");
    model.pan(-10000);
    assert(model.left === 0 && model.right === 80, "Earlier pan escaped start");
    model.zoom(1e9);
    assert(model.left === 0 && model.right === 8001, "Zoom out escaped full song");
    equal([model.start, model.end], selection, "Navigation changed frame selection");
    model.selectFrames(8001, 8001);
    assert(model.start === 8000 && model.end === 8001, "EOF pointer selection must retain final complete frame");
    model.selectFrames(301, 201);
    assert(model.start === 201 && model.end === 301, "Backward drag selection lost exclusive endpoint");
    const selected = [model.start, model.end];
    for (const [start, end] of [[NaN, .5], [0, Infinity], [-1, .5], [.8, .2], [.2, 2]]) {
        assert(model.setSeconds(start, end), "Invalid timeline range accepted");
        equal([model.start, model.end], selected, "Invalid timeline range changed selection");
    }
    model.configure({ sampleRate: rate, length: 1 });
    model.fit(); model.zoom(.5); model.fitSelection();
    assert(model.minimumSpan === 1 && model.left === 0 && model.right === 1, "Shorter-than-10ms stem must still show its one frame");
    model.reset();
    assert(model.frames === 0 && model.rate === 0 && !model.hasSelection, "Reset retained selection/audio dimensions");

    const cache = createEnvelopeCache(), frames = 773;
    // Envelope/state logic only needs the read-only AudioBuffer interface. Keep
    // these pure checks usable on hosts whose WebKit has no Web Audio backend.
    const bufferOf = () => {
        const channels = [new Float32Array(frames), new Float32Array(frames)];
        return { length: frames, numberOfChannels: 2, sampleRate: rate, getChannelData: channel => channels[channel] };
    };
    const buffer = bufferOf();
    const left = buffer.getChannelData(0), right = buffer.getChannelData(1);
    right[128] = .75; right[511] = -.625; right[772] = .5;
    let reads = 0;
    const counted = { length: frames, numberOfChannels: 2,
        getChannelData(channel) { reads++; return buffer.getChannelData(channel); } };
    const envelope = cache.get(counted);
    equal(envelope.range(0, 128), [0, 0], "Envelope included excluded end impulse");
    equal(envelope.range(128, 129), [0, .75], "Right-only one-frame impulse disappeared");
    equal(envelope.range(772, 773), [0, .5], "Last partial block disappeared");
    equal(envelope.range(0, frames), [-.625, .75], "Right-only stereo extrema lost");
    // Independently scan windows crossing head/tail/hierarchy boundaries.
    for (const [first, last] of [[127, 129], [129, 511], [511, 512], [512, 773], [1, 772], [0, 512], [128, 768]]) {
        let low = 0, high = 0;
        for (let frame = first; frame < last; frame++) {
            low = Math.min(low, left[frame], right[frame]);
            high = Math.max(high, left[frame], right[frame]);
        }
        equal(envelope.range(first, last), [low, high], `Envelope ${first}..${last}`);
    }
    for (let i = 0; i < 100; i++) assert(cache.get(counted) === envelope, "Envelope cache identity changed during redraw");
    assert(reads === 2, `Redraw rescanned immutable PCM: ${reads} channel reads`);
    const opposite = bufferOf();
    opposite.getChannelData(0)[300] = .625; opposite.getChannelData(1)[300] = -.625;
    equal(cache.get(opposite).range(0, frames), [-.625, .625], "Opposite-phase stereo vanished through averaging");
    const replacement = { ...counted, getChannelData(channel) { reads++; return opposite.getChannelData(channel); } };
    assert(cache.get(replacement) !== envelope, "New immutable buffer reused stale envelope");
    cache.reset();
    assert(cache.get(counted) !== envelope && reads === 6, "Reset did not invalidate only the cached identity");
    return ["TimelineState half-up/one-frame/exclusive-EOF bounds, invalid retention, backwards selection, 10ms zoom floor and one-frame shorter stem",
        "envelope independent extrema/window checks: right-only/opposite-phase stereo, exclusive endpoints, last partial block; immutable cache identity/replacement/reset"];
}
