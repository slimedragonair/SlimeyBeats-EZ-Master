// Test-only observation of the shipped compressed reader and native output.
window.checkArrangementStream = async ({ path, token, duration, mode, priming, expectedMarkers, mime, descriptor, projectId }) => {
    const { createCompressedPreview, supportsCompressedPreview } = await import(new URL('/audio/compressed-preview-source.js', location.href).href);
    const { NativeArrangementPlayer } = await import(new URL('/audio/native-arrangement-player.js', location.href).href);
    const { arrangementStreamFrame } = await import(new URL('/hybrid/arrangement-stream-contract.js', location.href).href);
    const compressed = mime.startsWith('audio/mp4');
    if (compressed && !supportsCompressedPreview()) throw new Error('This probe requires the timestamped MP4 decoder');
    const originalFetch = window.fetch, append = SourceBuffer.prototype.appendBuffer, NativeAudio = window.Audio;
    const result = { reads: 0, bytes: 0, appends: 0, maxAhead: 0, errors: [], events: [], clock: [], webAudioGraphs: 0 };
    let signal, audio = new Audio(), player, button, interval, bridge;
    const record = event => {
        const value = { event, at: performance.now(), time: audio.currentTime, duration: Number.isFinite(audio.duration) ? audio.duration : null,
            ended: audio.ended, paused: audio.paused, ready: audio.readyState, position: player?.position() };
        result.events.push(value); void window.arrangementStreamClock(value);
    };
    const prototypes = [...new Set([window.AudioContext?.prototype, window.webkitAudioContext?.prototype].filter(Boolean))];
    const methods = prototypes.map(prototype => [prototype, prototype.createMediaElementSource]);
    for (const [prototype] of methods) prototype.createMediaElementSource = () => {
        ++result.webAudioGraphs; throw new Error('Native arrangement probe cannot attach a Web Audio source');
    };
    window.fetch = async (url, options = {}) => {
        if (new URL(url, location.href).pathname !== path) return originalFetch(url, options);
        const response = await originalFetch(url, { ...options, headers: { Authorization: 'Bearer ' + token } });
        signal = options.signal;
        const reader = response.body.getReader();
        return { ok: response.ok, headers: response.headers, body: { cancel: () => reader.cancel(), getReader: () => ({
            async read() { ++result.reads; const value = await reader.read(); result.bytes += value.value?.length || 0;
                if (duration < 2 && value.value) (result.encodedChunks ||= []).push([...value.value]);
                if (value.done) record('stream-eof'); return value; },
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
    const diagnostic = () => ({ ...result, clock: result.clock.slice(-5), encodedChunks: undefined, media: { time: audio.currentTime, duration: Number.isFinite(audio.duration) ? audio.duration : null,
        ended: audio.ended, paused: audio.paused, ready: audio.readyState, network: audio.networkState,
        buffered: Array.from({ length: audio.buffered.length }, (_, i) => [audio.buffered.start(i), audio.buffered.end(i)]) } });
    window.arrangementStreamProbeSnapshot = () => ({ ...result, media: diagnostic().media });
    const until = async predicate => { const end = performance.now() + 15000;
        while (!predicate()) { if (result.errors.length || performance.now() > end) throw new Error(JSON.stringify({ ...result,
            media: { time: audio.currentTime, duration: audio.duration, ended: audio.ended, paused: audio.paused,
                ready: audio.readyState, network: audio.networkState,
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
            let address = path;
            if (!compressed) {
                const { RemoteMediaBridge } = await import(new URL('/hybrid/remote-media-bridge.js', location.href).href);
                bridge = new RemoteMediaBridge({ replyMediaAccess(port, requested) {
                    port.postMessage({ path: requested, endpoint: location.origin, token });
                } });
                await bridge.ready(); address = bridge.url(projectId, descriptor.stream_id, 'arrangement');
                result.native_service_worker = !!navigator.serviceWorker.controller;
            }
            player = new NativeArrangementPlayer(() => {}, error => result.errors.push(String(error)));
            player.setOriginal({ id: 'clock-probe', remote: true, seekable: false, previewMime: mime,
                mediaUrl: address, duration, sampleRate: 48000, numberOfChannels: 2 });
            window.Audio = function (...args) { audio = new NativeAudio(...args);
                for (const event of ['loadedmetadata', 'play', 'playing', 'pause', 'waiting', 'seeking', 'seeked', 'ended', 'error'])
                    audio.addEventListener(event, () => { if (event === 'ended') result.endedEvent = true; record(event); });
                return audio; };
            window.Audio.prototype = NativeAudio.prototype;
            interval = setInterval(() => result.clock.push({ at: performance.now(), time: audio.currentTime, position: player.position(),
                sourceFrame: arrangementStreamFrame(descriptor, player.position()), playing: player.playing }), 20);
            await new Promise((resolve, reject) => {
                button = document.createElement('button'); button.textContent = 'Play decoder probe'; document.body.append(button);
                button.onclick = () => {
                    record('play-request');
                    const timer = setTimeout(() => reject(new Error(JSON.stringify(diagnostic()))), 15000);
                    void player.play().then(resolve, reject).finally(() => clearTimeout(timer));
                };
            });
            if (mode === 'bounds') {
                await until(() => audio.currentTime > 4.1);
                result.buffered = [audio.buffered.start(0), audio.buffered.end(audio.buffered.length - 1)];
                result.position = audio.currentTime;
            } else {
                await until(() => result.endedEvent);
                result.expectedMarkers = expectedMarkers; result.priming = priming;
                result.ended = result.endedEvent; result.duration = audio.duration; result.finalPosition = player.position(); result.rate = 48000;
                result.finalFrame = arrangementStreamFrame(descriptor, player.position());
            }
        }
        return result;
    } finally {
        clearInterval(interval); preview?.dispose(); player?.clear(); result.aborted = signal?.aborted;
        button?.remove();
        audio.pause(); audio.removeAttribute('src'); audio.load();
        result.disposed = !audio.getAttribute('src') && !player?.current; record('disposed');
        bridge?.dispose();
        window.fetch = originalFetch; window.Audio = NativeAudio; SourceBuffer.prototype.appendBuffer = append;
        for (const [prototype, method] of methods) prototype.createMediaElementSource = method;
    }
};
