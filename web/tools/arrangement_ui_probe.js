import { createArrangementTimeline } from "../arrangement-timeline-controller.js";
import { arrangementTimelineTemplate } from "../arrangement-timeline-template.js";
import { ArrangementTimelineState } from "../arrangement-timeline-state.js";
import { createArrangementTimelineView } from "../arrangement-timeline-view.js";
import { wireArrangementTimelineInteraction } from "../arrangement-timeline-interaction.js";
import { wireArrangementInspector } from "../arrangement-timeline-inspector.js";
import { wireArrangementImport } from "../arrangement-timeline-import.js";

const host = document.getElementById("arrangement-probe");
const defaults = () => [50, 120, 300, 800, 2000, 5000, 10000].map((freq, i) => ({
    type: i === 0 ? "low_shelf" : i === 6 ? "high_shelf" : "bell", freq, gain: 0, q: 0.707, on: true,
}));

let model, view, inspector, interaction, baseline, commands = [], seeks = [], nextReply, sceneAssets;
window.arrangementUiProbe = {
    async gate(mode = "unavailable") {
        let previewCalls = 0, installs = 0, edits = 0;
        const adapter = { read: async () => ({ revision: 0, arrangement: { version: 1, sample_rate: 48000,
            channels: 2, lanes: [], eq: [] }, assets: [], history: {}, capabilities: { playback: mode !== "unavailable" },
            preview: { identity: "probe", prepared_revision: 0, pending: false } }),
            edit: async () => { edits++; }, previewSource: async () => { previewCalls++;
                return { kind: "arrangement", identity: mode === "stale" ? "old" : "probe", sample_rate: 48000, channels: 2, duration_frames: 0 }; } };
        const controller = createArrangementTimeline(host, { adapter, defaults, position: () => 0,
            seek: () => {}, installPreview: async () => { installs++; } });
        const ready = await controller.initialise();
        const result = { ready, exposed: !host.querySelector('[data-arr="workspace"]').hidden, previewCalls, installs, edits };
        controller.dispose();
        return result;
    },
    mount() {
        host.innerHTML = arrangementTimelineTemplate;
        host.querySelector('[data-arr="workspace"]').hidden = false;
        host.querySelector('[data-arr="eq"]').hidden = true;
        for (const name of ["add-files", "add-zip", "undo", "redo"]) host.querySelector(`[data-arr="${name}"]`).disabled = true;
        model = new ArrangementTimelineState();
        const lane = index => ({ id: `lane-${index}`, name: index < 2 ? "Drums" : `Track ${index + 1}`,
            gain_db: 0, pan: 0, mute: false, solo: false, clips: [] });
        const lanes = Array.from({ length: 1000 }, (_, i) => lane(i));
        lanes[0].clips = [{ id: "clip-a", source_id: "asset-a", start_frame: 48000, source_start_frame: 24000,
            end_frame: 144000, gain_db: 0, fade_in_frames: 0, fade_out_frames: 0 }];
        lanes[1].clips = [{ id: "clip-b", source_id: "asset-b", start_frame: 0, source_start_frame: 0,
            end_frame: 192000, gain_db: 0, fade_in_frames: 0, fade_out_frames: 0 }];
        const assets = sceneAssets = [{ id: "asset-a", name: "Part A", frames: 288000 }, { id: "asset-b", name: "Part B", frames: 192000 }];
        model.configure({ version: 1, sample_rate: 48000, channels: 2, lanes, eq: [] }, 1);
        model.selectClip("clip-a"); baseline = JSON.stringify(model.arrangement); commands = []; seeks = [];
        const repaint = () => { view.sync(model, { assets }); inspector?.sync(); };
        const command = async value => {
            commands.push(structuredClone(value));
            if (!nextReply) return false;
            model.configure(nextReply, model.revision + 1, assets); nextReply = null; repaint(); return true;
        };
        view = createArrangementTimelineView(host, {
            getEnvelope: () => ({ peaks: Array.from({ length: 64 }, () => [-0.1, 0.1]) }),
            selectClip: id => { model.selectClip(id); repaint(); }, selectLane: id => { model.selectLane(id); repaint(); }, toggleLane: () => {},
        });
        inspector = wireArrangementInspector(host, { model: () => model, getState: () => ({ busy: false, assets }),
            command, position: () => 96000, repaint: () => { repaint(); view.reveal(model.selectedLane); }, notice: () => {} });
        interaction = wireArrangementTimelineInteraction(view, { model: () => model, allowed: () => true,
            sourceFrames: id => assets.find(asset => asset.id === id).frames, repaint,
            selectClip: id => model.selectClip(id), selectLane: id => model.selectLane(id), seek: frame => seeks.push(frame),
            commit: command, nudge: inspector.nudge, shortcut: inspector.action });
        for (const mode of ["move", "select"]) host.querySelector(`[data-arr="mode-${mode}"]`).onclick = () => { model.mode = mode; repaint(); };
        host.querySelector('[data-arr="fit-selection"]').onclick = () => { model.fitSelection(); repaint(); };
        repaint();
        return this.state();
    },
    state() { return { commands, seeks, unchanged: JSON.stringify(model.arrangement) === baseline,
        selectedClip: model.selectedClip, selectedLane: model.selectedLane, start: model.start, end: model.end,
        gesture: model.gesture, left: model.left, right: model.right, metrics: view.metrics() }; },
    clear() { commands = []; seeks = []; },
    singleFrameSelection() { model.setRange(48000, 48001); },
    gainReply(value) { nextReply = structuredClone(model.arrangement); nextReply.lanes[0].clips[0].gain_db = value; },
    receiveGain(value) {
        const next = structuredClone(model.arrangement); next.lanes[0].clips[0].gain_db = value;
        model.configure(next, model.revision + 1, sceneAssets); inspector.sync(); view.render();
    },
    async importReview() {
        interaction?.dispose(); inspector?.dispose(); view?.dispose();
        host.innerHTML = arrangementTimelineTemplate; host.querySelector('[data-arr="workspace"]').hidden = false;
        const plan = { id: "review", parts: [{ id: "part-a", name: "Drums", versions: [{ id: "wav-a", format: "wav" }, { id: "mp3-a", format: "mp3" }] },
            { id: "part-b", name: "Drums", versions: [{ id: "wav-b", format: "wav" }] }] };
        const importer = wireArrangementImport(host, { review: async () => plan, add: async () => false,
            discard: async () => true, getState: () => ({ busy: false, playable: true }), notice: () => {} });
        await importer.choose([new File([new Uint8Array([0])], "fixture.wav")]);
    },
    resetView() { view.viewport.scrollTop = 0; model.selectClip("clip-a"); model.fit(); view.render(); inspector.sync(); },
    dispose() { interaction?.dispose(); inspector?.dispose(); view?.dispose(); host.replaceChildren(); },
};
