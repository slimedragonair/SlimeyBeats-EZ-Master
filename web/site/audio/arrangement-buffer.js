// Bounded scheduling headroom for the worker -> main thread -> worklet path.
// At 48 kHz eight 2048-frame blocks are 341 ms and 128 KiB of stereo PCM.
// Never grow the queue in response to load: memory and live-edit latency stay bounded.
export const ARRANGEMENT_BLOCK_FRAMES = 2048;
export const ARRANGEMENT_MAX_QUEUE_BLOCKS = 32;
export function arrangementQueueBlocks(sampleRate) {
    if (!Number.isFinite(sampleRate) || sampleRate <= 0) throw new Error("Invalid arrangement sample rate");
    return Math.max(3, Math.min(ARRANGEMENT_MAX_QUEUE_BLOCKS, Math.ceil(sampleRate * .32 / ARRANGEMENT_BLOCK_FRAMES)));
}
