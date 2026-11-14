const assert = (ok, message) => { if (!ok) throw Error(message); };
const resource = (name) => performance.getEntriesByType("resource")
    .find((entry) => entry.name.includes(`/${name}`)).name;
function tones(samples, rate) {
    const tone = (hz) => {
        let real = 0, imag = 0;
        samples.forEach((sample, i) => {
            real += sample * Math.cos(i * hz * 2 * Math.PI / rate);
            imag += sample * Math.sin(i * hz * 2 * Math.PI / rate);
        });
        return 2 * Math.hypot(real, imag) / samples.length;
    };
    assert(samples.every(Number.isFinite), "Nonfinite PCM");
    return { vocal: tone(440), instrumental: tone(880), peak: Math.max(...samples.map(Math.abs)) };
}

export async function install() {
    const { StudioPlayback } = await import(resource("studio-player.js"));
    const { StudioClient } = await import(resource("studio-client.js"));
    const { Player } = await import(resource("audio/player.js"));
    const probe = window.studioProbe = { calls: [], transfers: [], pcm: [], frames: 0 };
    const setAudio = StudioPlayback.prototype.setAudio;
    StudioPlayback.prototype.setAudio = function(...args) {
        probe.playback = this;
        return setAudio.apply(this, args);
    };
    const play = Player.prototype.play;
    Player.prototype.play = function(...args) {
        probe.main = this;
        return play.apply(this, args);
    };
    const call = StudioClient.prototype.call;
    StudioClient.prototype.call = async function(action, ...args) {
        probe.calls.push(action);
        const result = await call.call(this, action, ...args);
        if (action === "export") probe.exported = result.bytes;
        if (result?.stems) probe.transfers.push({
            action, changed: result.stems.filter((stem) => stem.samples).map((stem) => stem.kind),
        });
        return result;
    };
    probe.attach = async () => {
        const p = probe.playback, ctx = p.context();
        await ctx.audioWorklet.addModule("/__studio_capture_processor.js");
        probe.tap = new AudioWorkletNode(ctx, "studio-capture", { outputChannelCount: [2] });
        probe.tap.port.onmessage = ({ data }) => {
            if (data.time < probe.captureAfter) return;
            probe.pcm.push(data.channels);
            probe.frames += data.channels[0].length;
            if (probe.pcm.length > 1024) probe.pcm.shift();
        };
        p.gain.connect(probe.tap);
        probe.tap.connect(ctx.destination); // The tap itself outputs silence.
        probe.rate = ctx.sampleRate;
    };
    probe.resetCapture = () => {
        probe.pcm = []; probe.frames = 0;
        probe.captureAfter = probe.playback.context().currentTime + .03;
    };
    probe.measure = () => {
        const samples = probe.pcm.flatMap((block) => [...block[0]]).slice(-9600);
        assert(samples.length === 9600, "Insufficient captured output PCM");
        return { ...tones(samples, probe.rate), position: probe.playback.position() };
    };
    probe.savedMeasure = async () => {
        const buffer = await probe.playback.context().decodeAudioData(probe.exported.slice().buffer);
        return tones(buffer.getChannelData(0).slice(-9600), buffer.sampleRate);
    };
    probe.remember = () => {
        probe.graph = probe.playback.mixer;
        probe.sources = [...probe.graph.tracks.values()].map((track) => track.source);
        probe.generation = probe.playback.generation;
        probe.start = probe.playback.started;
        probe.callCount = probe.calls.length;
        probe.bank = probe.playback.stems;
    };
    probe.unchanged = () => {
        const p = probe.playback;
        assert(p.playing && p.mixer === probe.graph, "Fader restarted the running graph");
        assert(p.generation === probe.generation && p.started === probe.start, "Transport restarted");
        assert(p.stems === probe.bank, "Fader copied the immutable bank");
        assert(probe.calls.length === probe.callCount, "Fader requested a worker render");
        assert([...p.mixer.tracks.values()].every((track, i) => track.source === probe.sources[i]), "Stem sources replaced");
    };
    probe.budget = () => {
        const p = probe.playback, original = p.stems;
        let rejected = false;
        try {
            p.prepareAudio({ stems: [{ kind: "vocals", length: 128 * 1024 * 1024 / 4 + 1 }] });
        } catch (error) { rejected = error.message.includes("128 MiB"); }
        assert(rejected && p.stems === original, "Live bank budget failed or changed committed stems");
        return { one_sample_over_128_MiB_rejected_before_allocation: true };
    };
}

export async function offlineChecks() {
    const { StudioStemMixer, stemPeaks, STEM_SMOOTH_SECONDS } =
        await import(resource("studio-stem-mixer.js"));
    const ctx = new OfflineAudioContext(2, 48000 / 4, 48000);
    const buffer = ctx.createBuffer(2, 12000, 48000);
    buffer.getChannelData(0).fill(.2);
    buffer.getChannelData(1).fill(.1);
    const settings = { output_gain_db: 0, protect_peaks: false, ceiling_dbtp: -1,
        tracks: [{ kind: "vocals", gain_db: 0, balance: 0, mute: false, solo: false }] };
    const bank = new Map([["vocals", { buffer, peaks: stemPeaks(buffer) }]]);
    const mixer = new StudioStemMixer(ctx, ctx.destination, bank, settings, 0, () => {});
    // Exercise the fallback even in browsers supporting cancelAndHoldAtTime.
    for (const gain of mixer.tracks.get("vocals").gains) gain.gain.cancelAndHoldAtTime = undefined;
    // Schedule control instants before rendering, including in Firefox where
    // OfflineAudioContext.suspend is unavailable. The actual graph renders PCM.
    Object.defineProperty(ctx, "currentTime", { configurable: true, value: .05 });
    settings.tracks[0].balance = 1;
    mixer.update(settings);
    Object.defineProperty(ctx, "currentTime", { configurable: true, value: .10 });
    settings.tracks[0].solo = settings.tracks[0].mute = true;
    mixer.update(settings);
    delete ctx.currentTime;
    const rendered = await ctx.startRendering();
    const [left, right] = [0, 1].map((ch) => rendered.getChannelData(ch));
    const sample = (data, seconds) => data[Math.floor(seconds * 48000)];
    assert(Math.abs(sample(left, .02) - .2) < 1e-7, "Initial left gain");
    assert(Math.abs(sample(right, .02) - .1) < 1e-7, "Initial right gain");
    assert(Math.abs(sample(left, .08)) < 1e-7 && Math.abs(sample(right, .08) - .1) < 1e-7, "Stereo balance crossed channels");
    assert(Math.abs(sample(left, .15)) < 1e-7 && Math.abs(sample(right, .15)) < 1e-7, "Mute did not override solo");
    let step = 0;
    // Skip the fixture's DC source onset; measure the requested gain ramps.
    for (const channel of [left, right]) for (let i = 1440; i < channel.length; i++) {
        assert(Number.isFinite(channel[i]), "Nonfinite smoothed PCM");
        step = Math.max(step, Math.abs(channel[i] - channel[i - 1]));
    }
    assert(step < .2 / (48000 * STEM_SMOOTH_SECONDS) + 1e-6, "Gain change clicked instead of ramping");
    return { ramp_seconds: STEM_SMOOTH_SECONDS, maximum_adjacent_step: step,
        stereo_balance: true, mute_overrides_solo: true, fallback_ramp: true };
}

export async function asyncChecks() {
    const { playback: p, main } = window.studioProbe;
    p.stop();
    const delayResume = (ctx) => {
        const resume = ctx.resume.bind(ctx);
        let release;
        ctx.resume = () => new Promise((resolve) => { release = () => resume().then(resolve); });
        return { release: () => release(), restore: () => { delete ctx.resume; } };
    };
    for (const player of [p, main]) {
        const delay = delayResume(player.context());
        const pending = player.play();
        player.stop();
        await delay.release();
        await pending;
        delay.restore();
        assert(!player.playing, "Late resume undid Stop");
    }
    const releases = [], ctx = p.context(), resume = ctx.resume.bind(ctx);
    ctx.resume = () => new Promise((resolve) => releases.push(() => resume().then(resolve)));
    const older = p.play(), newer = p.play();
    await releases[1]();
    await newer;
    const graph = p.mixer;
    await releases[0]();
    await older;
    delete ctx.resume;
    assert(p.playing && p.mixer === graph, "Older resume replaced or duplicated the latest graph");
    p.stop();
    const delayedMain = delayResume(main.context());
    const pendingMain = main.play();
    await p.play();
    await delayedMain.release();
    await pendingMain;
    delayedMain.restore();
    assert(p.playing && !main.playing, "Late hidden main resume overlapped Studio");
    const delayedStudio = delayResume(p.context());
    p.pause();
    const pendingStudio = p.play();
    await main.play();
    await delayedStudio.release();
    await pendingStudio;
    delayedStudio.restore();
    assert(main.playing && !p.playing, "Late Studio resume overlapped main");
    main.stop();
    const closing = delayResume(p.context());
    const pendingClose = p.play();
    p.clear();
    await closing.release();
    await pendingClose;
    closing.restore();
    assert(!p.playing && !p.before && !p.after && !p.stems, "Close resumed or retained PCM");
    return { studio_and_main_stop: true, exclusive_pending_resumes: true,
        latest_play_wins_out_of_order_resumes: true, close: true };
}
