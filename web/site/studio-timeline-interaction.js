// Pointer gestures select/seek only. Vertical touch scrolling and cancellation
// restore the previous range; jobs are reachable only through explicit buttons.
const DRAG_PIXELS = 8;
export function wireTimelineInteraction(viewport, { model, allowed, choose, selected, seek, repaint }) {
    let gesture;
    function restore() {
        if (!gesture) return;
        const previous = gesture;
        gesture = null;
        if (previous.dragging) {
            model().start = previous.start; model().end = previous.end;
            choose(previous.selected);
            repaint(true);
        }
        if (previous.target.hasPointerCapture(previous.id)) previous.target.releasePointerCapture(previous.id);
    }
    function frame(event, target) {
        const rect = target.getBoundingClientRect(), state = model();
        return Math.max(0, Math.min(state.frames,
            Math.round(state.left + Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width)) * state.span)));
    }
    viewport.addEventListener("pointerdown", (event) => {
        if (gesture) { restore(); return; }
        const target = event.target.closest(".s-time-wave, .s-time-ruler");
        if (!target || !event.isPrimary || event.button !== 0 || !allowed()) return;
        const state = model();
        gesture = { id: event.pointerId, target, x: event.clientX, y: event.clientY,
            first: frame(event, target), start: state.start, end: state.end, selected: selected(),
            kind: target.closest(".s-time-lane")?.dataset.kind || selected(), dragging: false };
        target.setPointerCapture(event.pointerId);
    });
    viewport.addEventListener("pointermove", (event) => {
        if (!gesture || gesture.id !== event.pointerId) return;
        if (!allowed()) { restore(); return; }
        const dx = Math.abs(event.clientX - gesture.x), dy = Math.abs(event.clientY - gesture.y);
        if (event.pointerType === "touch" && dy > DRAG_PIXELS && dy > dx) { restore(); return; }
        if (!gesture.dragging && dx >= DRAG_PIXELS && dx > dy * 1.25) {
            gesture.dragging = true;
            choose(gesture.kind);
        }
        if (gesture.dragging) {
            model().selectFrames(gesture.first, frame(event, gesture.target));
            repaint(true);
        }
    });
    viewport.addEventListener("pointerup", (event) => {
        if (!gesture || gesture.id !== event.pointerId) return;
        if (!allowed()) { restore(); return; }
        const current = gesture;
        gesture = null;
        if (current.target.hasPointerCapture(event.pointerId)) current.target.releasePointerCapture(event.pointerId);
        if (current.dragging) {
            model().selectFrames(current.first, frame(event, current.target));
            repaint(true);
        } else if (Math.hypot(event.clientX - current.x, event.clientY - current.y) < DRAG_PIXELS) {
            choose(current.kind);
            seek(frame(event, current.target) / model().rate);
            repaint(false);
        }
    });
    viewport.addEventListener("pointercancel", restore);
    viewport.addEventListener("lostpointercapture", restore);
    viewport.addEventListener("scroll", () => { restore(); repaint(false); }, { passive: true });
    return { cancel: restore };
}
