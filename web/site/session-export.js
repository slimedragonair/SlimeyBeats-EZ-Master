// Portable rendered-audio/settings handoff. This is distinct from local recovery
// and the native path-bound SessionDocument; it does not promise project restore.
import { buildZip, planZip, safeZipName } from "./zip-builder.js";
import { audioEntry, waveInfo } from "./session-export-audio.js";
import { analysisDocument, jsonBytes, plainName, portableTree, select, sourceMetadata } from "./session-export-metadata.js";
import { eqProfileJson, portableSettings, settingsJson } from "./session-export-settings.js";
export { eqProfileJson, settingsJson } from "./session-export-settings.js";
export { buildZip, planZip, ZIP_LIMITS } from "./zip-builder.js";

function jsonEntry(name, value) { return { name, bytes: jsonBytes(value) }; }
function packageBundle(workspace, files, roles, originalIncluded) {
    const manifest = {
        schema: "slimeybeats-session-bundle", version: 1, workspace,
        entries: files.map((file, i) => ({ name: file.name, bytes: file.bytes.byteLength, role: roles[i] })),
        manifest_excludes_itself: true,
        original_audio_included: originalIncluded, reference_audio_included: false,
        integrity: "ZIP entries carry CRC32 checksums; this is not an authenticity signature.",
    };
    return buildZip([...files, jsonEntry("manifest.json", manifest)]);
}
/**
 * Caller supplies an export and settings/report from the same revision.
 * audio: {name, bytes: Uint8Array}; report: current worker report; settings:
 * current Settings; source: {name,duration_seconds,sample_rate,channels};
 * reference?: {name}; includeOriginal?: true, original?: {name,bytes}.
 * Returns application/zip Blob. No source bytes are included by default.
 */
export function buildMasterBundle({ audio, report, settings, source, reference, includeOriginal = false, original }) {
    if (typeof includeOriginal !== "boolean") throw new Error("Original audio inclusion requires an explicit boolean choice");
    const master = audioEntry(audio, "master");
    const files = [master], roles = ["master_export"];
    if (includeOriginal) {
        files.push(audioEntry(original, "original", true));
        roles.push("original_audio_explicitly_selected");
    }
    const session = {
        schema: "slimeybeats-browser-session", version: 1, workspace: "mastering",
        source: sourceMetadata(source), reference: reference ? { name: plainName(reference.name) } : null,
        settings: portableSettings(settings), exported_audio: master.name,
        original_audio_included: includeOriginal, reference_audio_included: false,
        restore_support: "Rendered audio and portable settings only; use local recovery for workspace restoration.",
    };
    files.push(jsonEntry("analysis.json", analysisDocument(report)),
        { name: "eq-profile.json", bytes: eqProfileJson(settings) },
        { name: "settings.json", bytes: settingsJson(settings) }, jsonEntry("session.json", session));
    roles.push("measurements_and_heuristics", "selected_eq_controls", "selected_mastering_settings", "session_metadata");
    return packageBundle("mastering", files, roles, includeOriginal);
}
function stemEntries(files, manifest) {
    if (!Array.isArray(files)) throw new Error("Studio files must be an array of aligned exported stems");
    if (files.length === 0) {
        if (manifest?.stems?.length) throw new Error("Studio manifest has no matching stems");
        return { files: [], manifest: null, layout: null };
    }
    // Preflight before parsing even the bounded WAV headers.
    planZip(files.map((file) => ({ name: file?.name, size: file?.bytes?.byteLength })));
    if (files.length < 2 || manifest?.format !== "wav32f" || !Array.isArray(manifest.stems) || manifest.stems.length !== files.length)
        throw new Error("Studio ZIP requires the current aligned wav32f stem manifest");
    const kinds = new Set(), names = new Set(), fileNames = new Set(files.map((file) => file.name));
    const stems = manifest.stems.map((stem) => {
        const file = safeZipName(stem?.file);
        if (!/^[a-z0-9_]+\.wav$/i.test(file) || typeof stem.kind !== "string" ||
            !/^[a-z][a-z0-9_]{0,39}$/.test(stem.kind) || typeof stem.display_name !== "string" ||
            !stem.display_name.trim() || names.has(file.toLowerCase()) || kinds.has(stem.kind) || !fileNames.has(file))
            throw new Error("Studio stem manifest must map unique files, kinds and display names");
        kinds.add(stem.kind); names.add(file.toLowerCase());
        return { file, kind: stem.kind, display_name: plainName(stem.display_name, stem.kind) };
    });
    let layout;
    for (const file of files) {
        const current = waveInfo(file.bytes);
        if (current.encoding !== 3 || current.bits !== 32 || (layout &&
            ["frames", "sample_rate", "channels"].some((key) => current[key] !== layout[key])))
            throw new Error("Studio stems must be aligned float32 WAVs with identical frames, rate and channels");
        layout = current;
    }
    return { files: files.map((file) => ({ name: file.name, bytes: file.bytes })),
        manifest: { ...portableTree(select(manifest, ["schema", "schema_version", "version"])), format: "wav32f", stems }, layout };
}
/**
 * audio: current committed mix export {name,bytes}; files/manifest: unchanged
 * result of worker stem-export (or [] without a bank); analysis: optional current
 * preview analysis/metrics; info: worker info() including committed controls.
 * Caller must settle pending mix controls before collecting this snapshot.
 */
export function buildStudioBundle({ audio, files = [], manifest, analysis, info }) {
    const mix = audioEntry(audio, "mix"), bank = stemEntries(files, manifest);
    const bankKinds = new Set(bank.manifest?.stems.map((stem) => stem.kind));
    if (info?.stems !== undefined && (!Array.isArray(info.stems) || info.stems.length !== files.length ||
        new Set(info.stems).size !== info.stems.length ||
        info.stems.some((kind) => !bankKinds.has(kind))))
        throw new Error("Studio info and exported stem bank describe different revisions");
    const session = {
        schema: "slimeybeats-browser-session", version: 1, workspace: "studio", exported_audio: mix.name,
        info: portableTree(select(info, ["schema", "schema_version", "version", "duration_seconds", "sample_rate", "channels",
            "controls", "preview_is_mix", "notes", "direct_edit_stems"])),
        stem_layout: bank.layout ? select(bank.layout, ["frames", "sample_rate", "channels"]) : null,
        stems: bank.manifest?.stems || [], original_audio_included: false, reference_audio_included: false,
        controls_scope: "Committed bank mix controls; a directly edited preview may differ from a fresh bank mix.",
        restore_support: "Rendered preview and aligned edited stem bank only; undo history and original source are not included.",
    };
    const contents = [mix, ...bank.files, jsonEntry("analysis.json", analysisDocument(analysis)),
        jsonEntry("settings.json", { schema: "slimeybeats-studio-settings-export", version: 1, eq_status: "not_applied",
            controls: session.info.controls ?? null, preview_is_mix: session.info.preview_is_mix ?? null,
            controls_scope: session.controls_scope }), jsonEntry("session.json", session)];
    const roles = ["current_preview_export", ...bank.files.map(() => "aligned_raw_edited_stem"),
        "measurements_and_heuristics", "committed_bank_mix_controls", "session_metadata"];
    if (bank.manifest) { contents.push(jsonEntry("stems.json", bank.manifest)); roles.push("stem_identity_mapping"); }
    return packageBundle("studio", contents, roles, false);
}
