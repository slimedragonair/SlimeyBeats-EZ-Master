// Test-only observation of the shipped compressed reader and actual decoder.
window.checkArrangementStream = async ({ path, token, duration, mode, priming, expectedMarkers, mime }) => {
    const { createCompressedPreview, supportsCompressedPreview } = await import(new URL('/audio/compressed-preview-source.js', location.href).href);
    const { StreamingPlayer } = await import(new URL('/audio/streaming-player.js', location.href).href);
    if (!supportsCompressedPreview()) throw new Error('This probe requires the timestamped MP4 decoder');
    const originalFetch = window.fetch, append = SourceBuffer.prototype.appendBuffer, NativeAudio = window.Audio;
    const result = { reads: 0, bytes: 0, appends: 0, maxAhead: 0, errors: [] };
    let signal, audio = new Audio(), context, capture, player, button, chunks = [], captureStart, playingAt;
    window.fetch = async (url, options = {}) => {
        if (new URL(url, location.href).pathname !== path) return originalFetch(url, options);
        const response = await originalFetch(url, { ...options, headers: { Authorization: 'Bearer ' + token } });
        signal = options.signal;
        const reader = response.body.getReader();
        return { ok: response.ok, headers: response.headers, body: { cancel: () => reader.cancel(), getReader: () => ({
            async read() { ++result.reads; const value = await reader.read(); result.bytes += value.value?.length || 0;
                if (duration < .5 && value.value) (result.encodedChunks ||= []).push([...value.value]); return value; },
            cancel: () => reader.cancel(), releaseLock: () => reader.releaseLock() }) } };
    };
    SourceBuffer.prototype.appendBuffer = function (bytes) {
        ++result.appends;
        this.addEventListener('updateend', () => {
            if (this.buffered.length) result.maxAhead = Math.max(result.maxAhead, this.buffered.end(this.buffered.length - 1) - audio.currentTime);
        }, { once: true });
        return append.call(this, bytes);
    };
    const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
    const until = async predicate => { const end = performance.now() + 15000;
        while (!predicate()) { if (result.errors.length || performance.now() > end) throw new Error(JSON.stringify({ ...result,
            media: { time: audio.currentTime, duration: audio.duration, ended: audio.ended, paused: audio.paused,
                ready: audio.readyState, network: audio.networkState, context: context?.state, clock: context?.currentTime,
                buffered: Array.from({length:audio.buffered.length}, (_,i)=>[audio.buffered.start(i),audio.buffered.end(i)]) } })); await sleep(10); } };
    const preview = mode === 'cancel' ? createCompressedPreview(audio, path, duration, error => result.errors.push(String(error))) : null;
    if (preview) { audio.preload = 'auto'; audio.src = preview.url; }
    try {
        await preview?.ready;
        if (mode === 'cancel') {
            await sleep(1500); result.pausedReads = result.reads; await sleep(1000);
            result.pausedReadsAfter = result.reads;
            result.bufferedEnd = audio.buffered.end(audio.buffered.length - 1);
        } else {
            player = new StreamingPlayer(() => {}, error => result.errors.push(String(error)));
            player.setOriginal({ id: 'clock-probe', remote: true, seekable: false, previewMime: mime,
                mediaUrl: path, duration, sampleRate: 48000, numberOfChannels: 2 });
            context = player.context();
            await context.audioWorklet.addModule('/__arrangement_capture.js');
            capture = new AudioWorkletNode(context, 'studio-capture', { outputChannelCount: [2] });
            capture.port.onmessage = ({ data }) => {
                if (captureStart === undefined) captureStart = data.time;
                if (chunks.length < 1600) chunks.push(data.channels[0]);
            };
            player.mix.connect(capture); capture.connect(context.destination);
            window.Audio = function (...args) { audio = new NativeAudio(...args);
                audio.addEventListener('ended', () => result.endedEvent = true);
                audio.addEventListener('playing', () => { playingAt ??= context.currentTime; }); return audio; };
            window.Audio.prototype = NativeAudio.prototype;
            await new Promise((resolve, reject) => {
                button = document.createElement('button'); button.textContent = 'Play decoder probe'; document.body.append(button);
                button.onclick = () => { void player.play().then(resolve, reject); };
            });
            if (mode === 'bounds') {
                await until(() => audio.currentTime > 4.1);
                result.buffered = [audio.buffered.start(0), audio.buffered.end(audio.buffered.length - 1)];
                result.position = audio.currentTime;
            } else {
                await until(() => result.endedEvent); await sleep(100);
                const values = chunks.flatMap(chunk => [...chunk]), peaks = [];
                result.captureFrames = values.length; result.captureStart = captureStart; result.playingAt = playingAt;
                result.capturePeak = values.reduce((peak, value) => Math.max(peak, Math.abs(value)), 0);
                result.raw = values;
                for (let i = 0; i < values.length; ++i) if (Math.abs(values[i]) > .12 && (!peaks.length || i - peaks.at(-1).end > 128))
                    peaks.push({ index: i, end: i, amplitude: Math.abs(values[i]) });
                else if (Math.abs(values[i]) > .12) {
                    const peak = peaks.at(-1); peak.end = i;
                    if (Math.abs(values[i]) > peak.amplitude) { peak.index = i; peak.amplitude = Math.abs(values[i]); }
                }
                result.peaks = peaks.map(peak => ({ sample: peak.index, amplitude: peak.amplitude,
                    fromPlaying: captureStart + peak.index / context.sampleRate - playingAt }));
                result.expectedMarkers = expectedMarkers; result.priming = priming;
                result.ended = result.endedEvent; result.duration = audio.duration; result.rate = context.sampleRate;
            }
        }
        return result;
    } finally {
        preview?.dispose(); player?.clear(); result.aborted = signal?.aborted;
        button?.remove();
        audio.pause(); audio.removeAttribute('src'); audio.load();
        capture?.port.close(); capture?.disconnect(); await context?.close();
        window.fetch = originalFetch; window.Audio = NativeAudio; SourceBuffer.prototype.appendBuffer = append;
    }
};
