const DRAG_PIXELS = 8;

export function wireArrangementTimelineInteraction(view, {
    model, allowed, sourceFrames, repaint, selectClip, selectLane, seek, commit, nudge, shortcut,
}) {
    const viewport = view.viewport;
    let gesture = null;
    function finishCapture(id) {
        if (viewport.hasPointerCapture(id)) viewport.releasePointerCapture(id);
    }
    function cancel() {
        if (!gesture) return;
        const previous = gesture;
        gesture = null;
        const state = model();
        state.selectedClip = previous.selectedClip; state.selectedLane = previous.selectedLane;
        state.start = previous.start; state.end = previous.end; state.gesture = null;
        finishCapture(previous.id); repaint();
    }
    function down(event) {
        if (gesture || !allowed() || !event.isPrimary || event.button !== 0) return;
        const laneHandle = event.target.closest("[data-lane-handle]");
        const wave = event.target.closest(".arr-lane-wave");
        if (!laneHandle && !wave) return;
        const state = model(), laneId = event.target.closest("[data-lane-id]")?.dataset.laneId;
        const clipId = event.target.closest("[data-clip-id]")?.dataset.clipId;
        const entry = clipId ? state.clip(clipId) : null;
        const rect = wave?.getBoundingClientRect();
        gesture = { id: event.pointerId, x: event.clientX, y: event.clientY, laneId, clipId,
            clip: entry?.clip, edge: event.target.closest("[data-trim]")?.dataset.trim,
            reorder: !!laneHandle, first: rect ? state.frameAt(event.clientX, rect) : 0,
            rect, dragging: false, selectedClip: state.selectedClip, selectedLane: state.selectedLane,
            start: state.start, end: state.end, revision: state.revision };
        viewport.setPointerCapture(event.pointerId);
        if (laneHandle) event.preventDefault();
    }
    function move(event) {
        if (!gesture || event.pointerId !== gesture.id) return;
        const state = model();
        if (!allowed() || state.revision !== gesture.revision) { cancel(); return; }
        const dx = Math.abs(event.clientX - gesture.x), dy = Math.abs(event.clientY - gesture.y);
        if (!gesture.dragging && !gesture.reorder && event.pointerType === "touch" && dy > DRAG_PIXELS && dy > dx) { cancel(); return; }
        const movingClip = !!gesture.clip && !gesture.edge && state.mode === "move";
        const moveThreshold = movingClip && event.pointerType !== "touch" ? Math.hypot(dx, dy) >= DRAG_PIXELS
            : dx >= DRAG_PIXELS && dx > dy * 1.25;
        if (!gesture.dragging && (gesture.reorder ? dy >= DRAG_PIXELS : moveThreshold))
            gesture.dragging = true;
        if (!gesture.dragging) return;
        if (gesture.reorder) {
            gesture.laneIndex = state.lanes.findIndex(lane => lane.id === view.laneAt(event.clientY)?.id);
            return;
        }
        const frame = state.frameAt(event.clientX, gesture.rect);
        if (state.mode === "select" || !gesture.clip) {
            state.selectLane(gesture.laneId); state.selectRange(gesture.first, frame);
        } else {
            state.selectClip(gesture.clipId);
            const delta = frame - gesture.first;
            if (gesture.edge) {
                const edge = gesture.edge === "start" ? gesture.clip.start_frame : gesture.clip.end_frame;
                state.gesture = state.trimPreview(gesture.clipId, gesture.edge, edge + delta, sourceFrames(gesture.clip.source_id));
            } else {
                const lane = view.laneAt(event.clientY);
                state.gesture = state.movePreview(gesture.clipId, gesture.clip.start_frame + delta, lane?.id || gesture.laneId);
            }
        }
        repaint();
    }
    function up(event) {
        if (!gesture || event.pointerId !== gesture.id) return;
        if (!allowed() || model().revision !== gesture.revision) { cancel(); return; }
        move(event);
        if (!gesture) return;
        const current = gesture, state = model(), preview = state.gesture;
        gesture = null; state.gesture = null; finishCapture(current.id);
        if (current.dragging) {
            if (current.reorder && current.laneIndex >= 0 && state.lanes[current.laneIndex]?.id !== current.laneId)
                commit({ kind: "reorder_lane", lane_id: current.laneId, index: current.laneIndex });
            else if (preview && current.edge) {
                if (preview.clip.start_frame !== current.clip.start_frame || preview.clip.end_frame !== current.clip.end_frame)
                    commit({ kind: "trim_clip", clip_id: current.clipId, start_frame: preview.clip.start_frame,
                        source_start_frame: preview.clip.source_start_frame, end_frame: preview.clip.end_frame });
            } else if (preview && (preview.clip.start_frame !== current.clip.start_frame || preview.lane_id !== current.laneId))
                commit({ kind: "move_clip", clip_id: current.clipId, lane_id: preview.lane_id, start_frame: preview.clip.start_frame });
        } else if (Math.hypot(event.clientX - current.x, event.clientY - current.y) < DRAG_PIXELS) {
            if (current.clipId) selectClip(current.clipId);
            else if (current.laneId) selectLane(current.laneId);
            if (!current.reorder) seek(current.first);
        }
        repaint();
    }
    function key(event) {
        if (event.key === "Escape") { cancel(); return; }
        if (!allowed() || event.target.closest("input,select,textarea,[contenteditable=true]")) return;
        const clip = event.target.closest("[data-clip-id]")?.dataset.clipId;
        if (clip) model().selectClip(clip);
        if (["ArrowLeft", "ArrowRight"].includes(event.key) && model().selectedClip) {
            event.preventDefault(); nudge(event.key === "ArrowLeft" ? -1 : 1, event.shiftKey ? 10 : 1); return;
        }
        const command = (event.ctrlKey || event.metaKey) && !event.altKey;
        if (command && event.key.toLowerCase() === "z") { event.preventDefault(); shortcut(event.shiftKey ? "redo" : "undo"); }
        else if (command && event.key.toLowerCase() === "d") { event.preventDefault(); shortcut("duplicate"); }
        else if (event.key === "Delete" && !event.altKey && !event.ctrlKey && !event.metaKey) { event.preventDefault(); shortcut("delete-clip"); }
    }
    viewport.addEventListener("pointerdown", down);
    viewport.addEventListener("pointermove", move);
    viewport.addEventListener("pointerup", up);
    viewport.addEventListener("pointercancel", cancel);
    viewport.addEventListener("lostpointercapture", cancel);
    viewport.addEventListener("keydown", key);
    viewport.addEventListener("scroll", cancel, { passive: true });
    return { cancel, dispose() {
        cancel();
        for (const [type, callback] of [["pointerdown", down], ["pointermove", move], ["pointerup", up],
            ["pointercancel", cancel], ["lostpointercapture", cancel], ["keydown", key], ["scroll", cancel]])
            viewport.removeEventListener(type, callback);
    } };
}
