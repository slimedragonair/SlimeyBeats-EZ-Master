import test from "node:test";
import assert from "node:assert/strict";
import { serverStoragePolicy } from "./server-limits.js";

const limits = { max_upload_bytes: 1024 ** 3, max_jobs: 2 };
const policy = (additional = {}) => serverStoragePolicy({
    limits: { ...limits, ...additional }, privacy: { retention_seconds: 3600 },
});

test("512 MiB bounds normalized MP3 to about 23m18s even when the advertised duration is 30 minutes", () => {
    const result = policy({ max_audio_seconds: 1800, max_decoded_bytes: 512 * 1024 ** 2 });
    assert.match(result, /configured audio duration limit 1,800 seconds/);
    assert.match(result, /decoded PCM budget 512 MiB/);
    assert.match(result, /normalizes MP3 input to 48 kHz stereo/);
    assert.match(result, /MP3 duration bound from the reported duration and decoded PCM limits together is about 23 min 18 sec/);
    assert.match(result, /not a universal track maximum/);
    assert.match(result, /WAV capacity depends on its sample rate and channels/);
    assert.match(result, /Local browser mastering has a separate 15-minute limit/);
});

test("a lower configured duration or memory budget lowers the displayed MP3 bound", () => {
    assert.match(policy({ max_audio_seconds: 600, max_decoded_bytes: 512 * 1024 ** 2 }), /about 10 min 0 sec/);
    assert.match(policy({ max_audio_seconds: 1800, max_decoded_bytes: 256 * 1024 ** 2 }), /about 11 min 39 sec/);
});

test("omitted optional limits stay compatible and do not invent an audio duration or decoded budget", () => {
    const result = policy();
    assert.doesNotMatch(result, /configured audio duration limit|decoded PCM budget|about \d+ min/);
    assert.match(result, /maximum upload/);
    assert.match(result, /48 kHz stereo/);
    assert.match(result, /separate 15-minute limit/);
});

test("each optional limit is displayed independently without assuming a missing counterpart", () => {
    const durationOnly = policy({ max_audio_seconds: 1800 });
    assert.match(durationOnly, /configured audio duration limit 1,800 seconds/);
    assert.doesNotMatch(durationOnly, /decoded PCM budget|about \d+ min/);
    const decodedOnly = policy({ max_decoded_bytes: 512 * 1024 ** 2 });
    assert.match(decodedOnly, /decoded PCM budget 512 MiB/);
    assert.match(decodedOnly, /budget alone is about 23 min 18 sec/);
    assert.doesNotMatch(decodedOnly, /configured audio duration limit/);
});
