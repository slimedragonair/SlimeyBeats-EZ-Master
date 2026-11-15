import { interleave } from "../dsp-client.js";

export async function analyzeSavedAudio(bytes, { context, client, profile = "neutral", signal }) {
    const check = () => {
        if (signal?.aborted) throw new DOMException("Export cancelled", "AbortError");
    };
    check();
    let decoded;
    try { decoded = await context.decodeAudioData(bytes.slice().buffer); }
    catch { throw new Error("This browser could not verify the saved audio. Try WAV export instead."); }
    check();
    const rate = decoded.sampleRate, channels = decoded.numberOfChannels;
    const samples = interleave(decoded);
    decoded = null;
    const result = await client.call("measure-encoded", { samples, rate, channels, profile }, [samples.buffer]);
    check();
    return result;
}
