import { selection } from "./studio-controls.js";
export function wireSelection(canvas, getImage, getChart, changed) {
    let anchor;
    function point(event) {
        const image = getImage(),
            chart = getChart();
        if (!image || !chart) return;
        const rect = canvas.getBoundingClientRect(),
            x = Math.max(
                0,
                Math.min(
                    1,
                    (event.clientX - rect.left - chart.x) / chart.width,
                ),
            ),
            y = Math.max(
                0,
                Math.min(
                    1,
                    (event.clientY - rect.top - chart.y) / chart.height,
                ),
            );
        return {
            time: x * image.duration_seconds,
            hz: 20 * (image.frequencies_hz.at(-1) / 20) ** (1 - y),
        };
    }
    canvas.onpointerdown = (event) => {
        anchor = point(event);
        if (anchor) {
            canvas.setPointerCapture(event.pointerId);
            event.preventDefault();
        }
    };
    canvas.onpointermove = (event) => {
        if (!anchor) return;
        const next = point(event);
        if (!next) return;
        for (const [id, value] of Object.entries({
            start: Math.min(anchor.time, next.time),
            end: Math.max(anchor.time, next.time),
            low: Math.min(anchor.hz, next.hz),
            high: Math.max(anchor.hz, next.hz),
        }))
            document.getElementById(`s-${id}`).value = value.toFixed(
                id === "low" || id === "high" ? 0 : 3,
            );
        changed(selection());
    };
    canvas.onpointerup = canvas.onpointercancel = () => {
        anchor = null;
    };
}
