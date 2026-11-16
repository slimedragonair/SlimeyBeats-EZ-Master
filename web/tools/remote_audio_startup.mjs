// A running context and resolved play() can precede the device clock on a
// headless PulseAudio sink. Require measured PCM; readiness alone never passes.
export const AUDIO_STARTUP_TIMEOUT_MS = 10000;
export const AUDIO_SAMPLE_INTERVAL_MS = 40;
export const AUDIO_MIN_RMS = 0.0001;
export const AUDIO_SILENCE_RMS = 1e-7;
const MAX_DIAGNOSTIC_SAMPLES = 64;

async function observeAudio({
    measure, ready, snapshot, trace, label,
    error = () => null,
    now = () => performance.now(),
    wait = ms => new Promise(resolve => setTimeout(resolve, ms)),
}, silent) {
    const started = now();
    Object.assign(trace, { timeout_ms: AUDIO_STARTUP_TIMEOUT_MS,
        ...(silent ? { maximum_silence_rms: AUDIO_SILENCE_RMS } : { minimum_rms: AUDIO_MIN_RMS }),
        initial: snapshot(), samples: [], maximum_rms: 0, attempts: 0 });
    for (;;) {
        const rms = measure(), elapsed = now() - started, status = snapshot();
        ++trace.attempts;
        trace.elapsed_ms = elapsed;
        trace.final = status;
        trace.maximum_rms = Math.max(trace.maximum_rms, rms);
        // Retain the first observation and the latest bounded window.
        if (trace.samples.length === MAX_DIAGNOSTIC_SAMPLES) trace.samples.splice(1, 1);
        trace.samples.push({ elapsed_ms: elapsed, rms, ...status });
        const problem = error();
        if (problem || !Number.isFinite(rms))
            throw new Error(`${label}: ${problem || 'Nonfinite captured PCM'}; ${JSON.stringify(trace)}`);
        const accepted = silent ? rms < AUDIO_SILENCE_RMS : rms > AUDIO_MIN_RMS;
        if (elapsed <= AUDIO_STARTUP_TIMEOUT_MS && ready() && accepted)
            return { rms, elapsed_ms: elapsed, attempts: trace.attempts };
        if (elapsed >= AUDIO_STARTUP_TIMEOUT_MS)
            throw new Error(`${label} ${silent ? 'did not reach silence' : 'produced no measurable samples'} before the startup deadline; ${JSON.stringify(trace)}`);
        await wait(Math.min(AUDIO_SAMPLE_INTERVAL_MS, AUDIO_STARTUP_TIMEOUT_MS - elapsed));
    }
}

export const waitForMeasuredAudio = options => observeAudio(options, false);
export const waitForAudioSilence = options => observeAudio(options, true);
