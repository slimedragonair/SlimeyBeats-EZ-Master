// Worker pointer/length values are local hints, not cross-worker audio identities.
export async function verifyWorkerBufferReplacement({ clientUrl, playbackUrl }) {
    const { StudioClient } = await import(clientUrl);
    const { StudioPlayback } = await import(playbackUrl);
    const first = new StudioClient(), second = new StudioClient();
    const playback = new StudioPlayback(() => {});
    const assert = (value, message) => { if (!value) throw new Error(message); };
    async function bank(client, value) {
        await client.call("init");
        const stems = ["vocals", "instrumental"].map((kind, index) => ({
            kind, rate: 48000, channels: 1,
            samples: new Float32Array(48000).fill(value + index * 0.01),
        }));
        const result = await client.call("stems", { stems }, stems.map(stem => stem.samples.buffer));
        await client.call("commit", { proposal: result.proposal });
        return result;
    }
    try {
        const original = await bank(first, 0.05);
        playback.setAudio(playback.prepareAudio(original), true);
        const originalBank = playback.stems;
        const sameWorker = await first.call("mix", { settings: original.info.controls });
        const reused = playback.prepareAudio(sameWorker);
        for (const stem of sameWorker.stems) {
            assert(stem.samples === undefined, "Unchanged worker stem must omit replacement PCM");
            assert(reused.stems.get(stem.kind) === originalBank.get(stem.kind),
                "Unchanged immutable stem should retain its live buffer");
        }
        await first.call("discard", { proposal: sameWorker.proposal });

        const replacement = await bank(second, 0.15);
        let actualCollisions = 0;
        const expected = new Map(replacement.stems.map(stem => {
            assert(stem.samples instanceof Float32Array, "Fresh worker must supply every stem's PCM");
            const previous = originalBank.get(stem.kind);
            if (stem.identity === previous.identity) actualCollisions++;
            // Make the cache contract independent of a future allocator layout:
            // supplied PCM must win even if the incoming hint equals an old hint.
            stem.identity = previous.identity;
            return [stem.kind, stem.samples.slice()];
        }));
        const prepared = playback.prepareAudio(replacement);
        for (const [kind, samples] of expected) {
            const stem = prepared.stems.get(kind);
            assert(stem !== originalBank.get(kind), "New PCM reused a stale playback stem");
            const audible = stem.buffer.getChannelData(0);
            assert(audible.length === samples.length, "Replacement stem length changed");
            let peak = 0;
            for (let frame = 0; frame < samples.length; frame++) {
                assert(audible[frame] === samples[frame], "Prepared playback differs from replacement worker PCM");
                peak = Math.max(peak, Math.abs(samples[frame]));
            }
            assert(stem.peaks[0] === peak, "Live peak cache retained old audio");
        }
        const sum = [...prepared.stems.values()].reduce((value, stem) => value + stem.buffer.getChannelData(0)[0], 0);
        assert(Math.abs(sum - replacement.samples[0]) < 1e-7,
            "Live bank differs from the unchanged-control rendered preview");
        playback.setAudio(prepared, true);
        return {
            checks: ["fresh-worker PCM overrides colliding pointer hints; every playback sample/peak matches the replacement bank while unchanged same-worker buffers are still reused"],
            actual_pointer_collisions: actualCollisions,
            checked_stems: expected.size,
            scope: "Actual workers and prepared live AudioBuffers; no physical listening qualification",
        };
    } finally {
        first.stop(); second.stop(); playback.clear();
        await playback.decoder.ctx?.close();
    }
}
