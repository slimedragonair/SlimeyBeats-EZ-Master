// The ABI follows wasm-media-encoders 0.7.0; original MIT notice is in encoders/.
// Only the pinned MP3 binary is used; no external scripts or codec are loaded.
export async function createMp3Encoder(sampleRate, channels, bitrateKbps) {
    const url = new URL("./encoders/lame-3.100-wme-0.7.0.wasm", import.meta.url);
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Local MP3 encoder could not load (${response.status})`);
    const binary = await response.arrayBuffer();
    if (binary.byteLength !== 132999) throw new Error("Local MP3 encoder asset is damaged");
    const { instance } = await WebAssembly.instantiate(binary, {
        env: { emscripten_notify_memory_growth() {} },
        wasi_snapshot_preview1: { proc_exit(code) { throw new Error(`MP3 encoder exited (${code})`); } },
    });
    const wasm = instance.exports;
    wasm._initialize();
    const string = (ptr) => {
        const bytes = new Uint8Array(wasm.memory.buffer, ptr, 64);
        return String.fromCharCode(...bytes.subarray(0, bytes.indexOf(0)));
    };
    if (string(wasm.version()) !== "wasm-media-encoders-0.7.0" || string(wasm.mime_type()) !== "audio/mpeg")
        throw new Error("MP3 encoder binary version or MIME type mismatch");
    const params = wasm.malloc(20);
    if (!params) throw new Error("MP3 encoder parameter allocation failed");
    let state;
    try {
        const data = new DataView(wasm.memory.buffer, params, 20);
        data.setUint32(0, channels, true);
        data.setUint32(4, sampleRate, true);
        data.setUint32(8, bitrateKbps, true);
        data.setFloat32(12, -1, true); // Explicit CBR; never let LAME select a different rate.
        data.setUint32(16, sampleRate, true);
        state = wasm.enc_init(params);
        if (!state) throw new Error("MP3 encoder initialization failed");
    } finally {
        wasm.free(params);
    }
    const output = (size) => {
        if (size < 0) throw new Error(`MP3 encoding failed (${size})`);
        // Output storage is borrowed and changes on the next encoder call.
        return new Uint8Array(wasm.memory.buffer, wasm.enc_get_out_buf(state), size).slice();
    };
    return {
        encode(planar) {
            const frames = planar[0].length, pointers = wasm.enc_get_pcm(state, frames);
            if (!pointers) throw new Error("MP3 encoder PCM allocation failed");
            const addresses = new Uint32Array(wasm.memory.buffer, pointers, channels);
            for (let ch = 0; ch < channels; ch++)
                new Float32Array(wasm.memory.buffer, addresses[ch], frames).set(planar[ch]);
            return output(wasm.enc_encode(state, frames));
        },
        finish() { return output(wasm.enc_flush(state)); },
        close() { if (state) wasm.enc_free(state); state = 0; },
    };
}
