import { buildMasterBundle, buildStudioBundle, eqProfileJson, settingsJson, buildZip, planZip, ZIP_LIMITS } from "../site/session-export.js";
import { analysisDocument, jsonBytes, portableTree, MAX_JSON_BYTES } from "../site/session-export-metadata.js";
import { waveInfo } from "../site/session-export-audio.js";
import { stemZip } from "../site/studio-zip.js";

const text = (bytes) => new TextDecoder().decode(bytes);
function assert(value, message) { if (!value) throw new Error(message); }
function rejects(call, pattern) {
    let error;
    try { call(); } catch (caught) { error = caught; }
    assert(error && pattern.test(String(error)), `Expected ${pattern}, received ${error}`);
}
function wav(frames = 32, channels = 2, rate = 48000, encoding = 3, bits = 32, extensible = false) {
    const size = frames * channels * bits / 8, fmtSize = extensible ? 40 : 16;
    const bytes = new Uint8Array(28 + fmtSize + size + (size & 1)), view = new DataView(bytes.buffer);
    const put = (at, value) => bytes.set(new TextEncoder().encode(value), at);
    put(0, "RIFF"); view.setUint32(4, bytes.length - 8, true); put(8, "WAVEfmt ");
    view.setUint32(16, fmtSize, true); view.setUint16(20, extensible ? 0xfffe : encoding, true);
    view.setUint16(22, channels, true); view.setUint32(24, rate, true);
    view.setUint32(28, rate * channels * bits / 8, true); view.setUint16(32, channels * bits / 8, true);
    view.setUint16(34, bits, true);
    if (extensible) {
        view.setUint16(36, 22, true); view.setUint16(38, bits, true);
        bytes.set([encoding, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113], 44);
    }
    const data = 20 + fmtSize;
    put(data, "data"); view.setUint32(data + 4, size, true);
    if (encoding === 3) for (let i = 0; i < frames * channels; i++) view.setFloat32(data + 8 + i * 4, (i % 7) / 100, true);
    return bytes;
}
function base64(bytes) {
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return btoa(binary);
}
async function archive(blob) {
    assert(blob instanceof Blob && blob.type === "application/zip", "ZIP must return application/zip Blob");
    return base64(new Uint8Array(await blob.arrayBuffer()));
}
export async function verifySessionExports(defaults, validateSettings, realExports = [], realMaster) {
    const checks = [], artifacts = {};
    const settings = structuredClone(defaults);
    settings.manual_eq[2].gain = -3;
    settings.eq_output_db = 1.25;
    settings.out_dir = "C:\\Users\\private-user\\Music\\output";
    settings.reference = "/home/private-user/reference.wav";
    settings.unrecognized_private_field = "private-user";
    settings.modules = { reference_match: { enabled: true }, custom: {
        enabled: false, path: "C:\\Users\\private-user\\song.wav", message: "/tmp/private-user/notes", apiToken: "private-user" } };
    const eq = JSON.parse(text(eqProfileJson(settings))), portable = JSON.parse(text(settingsJson(settings)));
    assert(eq.version === 2 && eq.bands.length === 7 && eq.bands[2].gain === -3 && eq.out_gain_db === 1.25 && eq.bypass === false, "Desktop EQ preset fields changed");
    assert(portable.manual_eq[2].gain === -3 && !('out_dir' in portable) && !('reference' in portable), "Portable settings lost controls or leaked paths");
    assert(portable.modules.reference_match.enabled, "Legitimate reference matching module was removed");
    assert(!text(settingsJson(settings)).includes("private-user"), "Portable settings leak");
    validateSettings?.(portable);
    validateSettings?.(JSON.parse(text(settingsJson(defaults))));
    const eqRestored = { ...defaults, manual_eq: eq.bands, eq_hold_loudness: eq.hold_loudness, eq_output_db: eq.out_gain_db };
    validateSettings?.(eqRestored);
    for (const edit of [{ ceiling_dbtp: -12, intensity: 0 }, { ceiling_dbtp: 0, intensity: 1.5 },
        { target_lufs: -40, eq_output_db: -12 }, { target_lufs: -5, eq_output_db: 12 }])
        validateSettings?.(JSON.parse(text(settingsJson({ ...defaults, ...edit }))));
    artifacts.eq = base64(eqProfileJson(settings)); artifacts.settings = base64(settingsJson(settings));
    checks.push("actual WASM defaults and edited portable settings accepted by Rust Settings; desktop version-2 EQ field round trip");

    const measurement = { duration_s: 32 / 48000, sample_rate: 48000, channels: 2,
        metrics: { integrated_lufs: -14.2, true_peak_dbtp: -1.1, clipped_samples: 0 },
        spectrum: { freqs: [100, 1000], db: [-20, -25], cutoff_hz: null, noise_candidate: true }, measured_db: [0, -2], valid: [true, false],
        deviation_db: [1, 4], issues: [{ key: "mud", severity: 30, detail: "A heuristic warning" }], suggested_profile: "neutral" };
    const report = { version: "0.3.0", schema_version: 7, file: settings.out_dir, input: measurement, output: measurement,
        configuration: settings, target_lufs: -14, ceiling_dbtp: -1,
        log: [{ module: "static_eq", seconds: 0.1, detail: { path: settings.out_dir, message: settings.reference, moves: [1, -2] } }],
        exports: [{ file: settings.out_dir, metrics: measurement.metrics }] };
    const options = { audio: { name: "Chosen master.wav", bytes: wav() }, report, settings,
        source: { name: "C:\\Users\\private-user\\Song.wav", duration_seconds: 32 / 48000, sample_rate: 48000, channels: 2,
            path: settings.out_dir, private_field: "private-user" }, reference: { name: "/home/private-user/Reference.wav" } };
    const before = JSON.stringify(options);
    artifacts.master = await archive(buildMasterBundle(options));
    assert(before === JSON.stringify(options), "Master packager mutated caller data");
    assert(artifacts.master === await archive(buildMasterBundle(options)), "Master ZIP is not deterministic");
    artifacts.original = await archive(buildMasterBundle({ ...options, includeOriginal: true, original: { name: "Source.wav", bytes: wav(16) } }));
    artifacts.original_ignored = await archive(buildMasterBundle({ ...options, original: { name: "Source.wav", bytes: new Uint8Array([99]) } }));
    rejects(() => buildMasterBundle({ ...options, includeOriginal: true }), /audio requires/);
    rejects(() => buildMasterBundle({ ...options, includeOriginal: "yes" }), /explicit boolean/);
    const opaqueMp3 = new Uint8Array([73, 68, 51, 4, 0, 0, 0, 0, 0, 0]);
    artifacts.mp3 = await archive(buildMasterBundle({ ...options, audio: { name: "Export.mp3", bytes: opaqueMp3 } }));
    checks.push("deterministic immutable mastering bundle; explicit-only original; WAV and opaque MP3 byte preservation");
    for (let i = 0; i < realExports.length; i++)
        artifacts[`wasm_wav_${i}`] = await archive(buildMasterBundle({ audio: { name: "Real export.wav", bytes: realExports[i] }, settings: defaults }));
    if (realExports.length) checks.push("actual Rust WAV16/WAV24/WAV32F exports with odd mono frame count package successfully");
    if (realMaster) artifacts.wasm_report = await archive(buildMasterBundle({ audio: { name: "Real.wav", bytes: realMaster.audio },
        settings: defaults, report: realMaster.report }));

    const files = [{ name: "vocals.wav", bytes: wav() }, { name: "part_64.wav", bytes: wav(32, 2, 48000, 3, 32, true) }];
    const manifest = { format: "wav32f", version: 9, stems: [
        { file: "vocals.wav", kind: "vocals", display_name: "Lead vocal" },
        { file: "part_64.wav", kind: "part_64", display_name: "/home/private-user/Backing take" },
    ] };
    const info = { stems: ["vocals", "part_64"], controls: { tracks: [
        { kind: "vocals", gain_db: -6, balance: 0, mute: false, solo: false },
        { kind: "part_64", gain_db: 0, balance: 0.2, mute: false, solo: false }], output_gain_db: 1, protect_peaks: true, ceiling_dbtp: -1 },
        duration_seconds: 32 / 48000, sample_rate: 48000, channels: 2, preview_is_mix: false,
        notes: ["Direct vocal edit"], direct_edit_stems: ["vocals"], filename: settings.out_dir };
    const studio = { audio: { name: "Studio.wav", bytes: wav(32, 2, 48000, 1, 24) }, files, manifest, analysis: measurement.metrics, info };
    const studioBefore = JSON.stringify(studio);
    artifacts.studio = await archive(buildStudioBundle(studio));
    assert(studioBefore === JSON.stringify(studio), "Studio packager mutated caller data");
    artifacts.studio_empty = await archive(buildStudioBundle({ audio: studio.audio, info: { stems: [], controls: {}, preview_is_mix: false } }));
    artifacts.studio_partial = await archive(buildStudioBundle({ audio: studio.audio, info: {} }));
    for (const bytes of [wav(33), wav(32, 1), wav(32, 2, 44100), wav(32, 2, 48000, 1, 16)])
        rejects(() => buildStudioBundle({ ...studio, files: [files[0], { ...files[1], bytes }] }), /aligned float32/);
    rejects(() => buildStudioBundle({ ...studio, info: { stems: ["drums", "bass"] } }), /different revisions/);
    rejects(() => buildStudioBundle({ ...studio, info: { stems: "vocals" } }), /different revisions/);
    rejects(() => buildStudioBundle({ ...studio, info: { stems: ["vocals", "vocals"] } }), /different revisions/);
    rejects(() => buildStudioBundle({ ...studio, manifest: { ...manifest, stems: [manifest.stems[0], manifest.stems[0]] } }), /unique/);
    rejects(() => buildStudioBundle({ ...studio, manifest: { ...manifest, format: "wav24" } }), /wav32f/);
    checks.push("Studio current preview plus aligned float32 edited bank; extensible WAV; committed controls and partial/no-bank metadata; alignment and identity rejection");

    const backing = new Uint8Array([88, 1, 2, 99]);
    const general = buildZip([{ name: "vector.bin", bytes: new TextEncoder().encode("123456789") },
        { name: "empty.json", bytes: new Uint8Array() }, { name: "slice.bin", bytes: backing.subarray(1, 3) }]);
    backing.fill(77);
    artifacts.general = await archive(general);
    for (const name of ["../x.wav", "folder/x.wav", "C:\\x.wav", "/x.wav", "x..wav", "CON.wav", "LPT9.json", "ends.", "file:audio", "é.wav", "x".repeat(121), "x\u0000.wav"])
        rejects(() => planZip([{ name, size: 1 }]), /safe flat/);
    rejects(() => planZip([{ name: "A.wav", size: 0 }, { name: "a.wav", size: 0 }]), /unique/);
    const fixture = [{ name: "a.bin", size: 10 }], exact = planZip(fixture).size;
    assert(planZip(fixture, { maxArchiveBytes: exact, maxEntryBytes: 10, maxEntries: 1 }).size === exact, "Exact ZIP budgets rejected");
    rejects(() => planZip(fixture, { maxArchiveBytes: exact - 1 }), /budget/);
    rejects(() => planZip(fixture, { maxEntryBytes: 9 }), /entry exceeds/);
    rejects(() => planZip(fixture, { maxEntries: 0 }), /entry count/);
    for (const size of [-1, 0.5, NaN, Infinity, 0xffffffff, Number.MAX_SAFE_INTEGER])
        rejects(() => planZip([{ name: "a.bin", size }]), /ZIP32/);
    rejects(() => planZip([{ name: "a.bin", size: ZIP_LIMITS.maxEntryBytes + 1 }]), /entry exceeds/);
    rejects(() => planZip(Array.from({ length: 12 }, (_, i) => ({ name: `${i}.bin`, size: ZIP_LIMITS.maxEntryBytes }))), /ZIP64/);
    rejects(() => planZip([{ name: "a.bin", size: 300 * 1048576 }, { name: "b.bin", size: 300 * 1048576 }]), /budget/);
    rejects(() => planZip(new Array(0xffff).fill(fixture[0])), /ZIP64/);
    rejects(() => planZip(fixture, { maxArchiveBytes: ZIP_LIMITS.maxArchiveBytes + 1 }), /Invalid ZIP export limit/);
    rejects(() => buildZip([{ name: "a.bin", bytes: [1, 2] }]), /Uint8Array/);
    let blobs = 0;
    const OriginalBlob = globalThis.Blob;
    try {
        globalThis.Blob = class extends OriginalBlob { constructor(...args) { blobs++; super(...args); } };
        rejects(() => buildZip([{ name: "a.bin", bytes: new Uint8Array([1]) }], { maxEntryBytes: 0 }), /entry exceeds/);
    } finally { globalThis.Blob = OriginalBlob; }
    assert(blobs === 0, "Invalid ZIP constructed a Blob");
    checks.push("CRC reference vector; flat unique portable names; exact budgets; ZIP32/count guards; no oversized allocations or invalid Blob construction");

    for (const bytes of [wav().subarray(0, 40), new Uint8Array(44)]) rejects(() => waveInfo(bytes), /WAV/);
    const badWave = wav(); new DataView(badWave.buffer).setUint16(32, 3, true);
    rejects(() => waveInfo(badWave), /WAV layout/);
    const cycle = {}; cycle.self = cycle;
    for (const value of [cycle, { a: NaN }, new Date(), 1n, { a: new Array(8193).fill(0) }])
        rejects(() => portableTree(value), /metadata/);
    let deep = 0; for (let i = 0; i < 14; i++) deep = { deep };
    rejects(() => portableTree(deep), /complex/);
    rejects(() => jsonBytes({ body: "a".repeat(MAX_JSON_BYTES) }), /1 MiB/);
    for (const edit of [{ eq_output_db: 13 }, { manual_eq: [] }, { ceiling_dbtp: -13 }, { intensity: 2 },
        { output_sr: 48000.5 }, { suffix: "../bad" }, { formats: ["exe"] }, { disabled: ["invented"] }, { profile: settings.out_dir }])
        rejects(() => settingsJson({ ...settings, ...edit }), /Invalid|requires/);
    checks.push("JSON traversal, depth, byte and finite-number bounds; paths removed; invalid EQ/settings and WAV headers rejected");
    assert(analysisDocument({ version: "unmeasured" }).status === "not_recorded", "Partial report invented measurements");
    rejects(() => analysisDocument("not an object"), /analysis must/);

    const rawFiles = [{ name: "vocals.wav", bytes: new TextEncoder().encode("123456789") },
        { name: "instrumental.wav", bytes: new Uint8Array([0, 1, 255]) }];
    const rawManifest = { format: "wav32f", stems: [{ file: "vocals.wav", kind: "vocals", display_name: "Lead vocal" },
        { file: "instrumental.wav", kind: "instrumental", display_name: "Backing" }] };
    artifacts.raw_stems = base64(stemZip(rawFiles, rawManifest));
    checks.push("existing stemZip API deterministic baseline bytes preserved");
    return { checks, artifacts };
}
