// One scan per immutable AudioBuffer. Multichannel min/max preserves right-only
// and opposite-polarity stereo. The hierarchy stays bounded at ~1/8 of PCM size.
const BLOCK_FRAMES = 128;

function buildEnvelope(buffer) {
    const channels = Array.from({ length: buffer.numberOfChannels }, (_, channel) => buffer.getChannelData(channel));
    const count = Math.ceil(buffer.length / BLOCK_FRAMES);
    const low = new Float32Array(count), high = new Float32Array(count);
    for (let block = 0; block < count; block++) {
        let min = 0, max = 0;
        const end = Math.min(buffer.length, (block + 1) * BLOCK_FRAMES);
        for (const samples of channels)
            for (let frame = block * BLOCK_FRAMES; frame < end; frame++) {
                min = Math.min(min, samples[frame]);
                max = Math.max(max, samples[frame]);
            }
        low[block] = min;
        high[block] = max;
    }
    const levels = [{ low, high }];
    while (levels.at(-1).low.length > 1) {
        const previous = levels.at(-1), length = Math.ceil(previous.low.length / 2);
        const low = new Float32Array(length), high = new Float32Array(length);
        for (let index = 0; index < length; index++) {
            low[index] = Math.min(previous.low[index * 2], previous.low[index * 2 + 1] ?? 0);
            high[index] = Math.max(previous.high[index * 2], previous.high[index * 2 + 1] ?? 0);
        }
        levels.push({ low, high });
    }
    return {
        range(first, last) {
            const start = Math.max(0, Math.floor(first));
            const end = Math.min(buffer.length, Math.max(start + 1, Math.ceil(last)));
            let min = 0, max = 0;
            const head = Math.min(end, Math.ceil(start / BLOCK_FRAMES) * BLOCK_FRAMES);
            const tail = Math.max(head, Math.floor(end / BLOCK_FRAMES) * BLOCK_FRAMES);
            for (const samples of channels) {
                for (let frame = start; frame < head; frame++) {
                    min = Math.min(min, samples[frame]); max = Math.max(max, samples[frame]);
                }
                for (let frame = tail; frame < end; frame++) {
                    min = Math.min(min, samples[frame]); max = Math.max(max, samples[frame]);
                }
            }
            let block = head / BLOCK_FRAMES;
            const limit = tail / BLOCK_FRAMES;
            while (block < limit) {
                let level = 0;
                while (levels[level + 1] && block % (2 ** (level + 1)) === 0 &&
                    block + 2 ** (level + 1) <= limit) level++;
                const index = block / (2 ** level);
                min = Math.min(min, levels[level].low[index]);
                max = Math.max(max, levels[level].high[index]);
                block += 2 ** level;
            }
            return [min, max];
        },
    };
}

export function createEnvelopeCache() {
    let cache = new WeakMap();
    return {
        get(buffer) {
            if (!cache.has(buffer)) cache.set(buffer, buildEnvelope(buffer));
            return cache.get(buffer);
        },
        reset() { cache = new WeakMap(); },
    };
}
