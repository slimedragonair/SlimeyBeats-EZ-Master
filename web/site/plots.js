import { $, time } from "./view.js";
function surface(canvas) {
    const r = canvas.getBoundingClientRect(),
        ratio = Math.min(2, devicePixelRatio || 1);
    if (r.width < 1 || r.height < 1) return;
    const w = Math.round(r.width * ratio),
        h = Math.round(r.height * ratio);
    if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
    }
    const c = canvas.getContext("2d");
    c.setTransform(ratio, 0, 0, ratio, 0, 0);
    c.clearRect(0, 0, r.width, r.height);
    return { c, w: r.width, h: r.height };
}
export class Plots {
    constructor(player) {
        this.player = player;
        this.curve = new Float32Array(256);
        this.bins = new Float32Array(2048);
        this.peaks = [];
        this.running = true;
        this.draw = this.draw.bind(this);
        this.resize = new ResizeObserver(() => this.draw(false));
        this.resize.observe($("eq-plot"));
        this.resize.observe($("waveform"));
        requestAnimationFrame(this.draw);
    }
    setAudio(buffer) {
        this.peaks = [];
        if (!buffer) return;
        const count = 850,
            stride = Math.max(1, Math.floor(buffer.length / count));
        for (let start = 0; start < buffer.length; start += stride) {
            let min = 0,
                max = 0;
            for (let ch = 0; ch < buffer.numberOfChannels; ch++) {
                const data = buffer.getChannelData(ch);
                for (
                    let i = start;
                    i < Math.min(start + stride, data.length);
                    i++
                ) {
                    min = Math.min(min, data[i]);
                    max = Math.max(max, data[i]);
                }
            }
            this.peaks.push([min, max]);
        }
    }
    draw(schedule = true) {
        if (!this.running) return;
        const position = this.player.position(),
            duration = this.player.original?.duration || 0;
        $("clock").textContent =
            `${time(position)} / ${time(Math.round(duration))}`;
        if (!this.seeking)
            $("seek").value = duration ? (position / duration) * 1000 : 0;
        const wave = surface($("waveform"));
        if (wave) {
            const { c, w, h } = wave;
            c.strokeStyle = "#58416c";
            c.beginPath();
            c.moveTo(0, h / 2);
            c.lineTo(w, h / 2);
            c.stroke();
            if (this.peaks.length) {
                const x = duration ? (position / duration) * w : 0;
                for (let i = 0; i < this.peaks.length; i++) {
                    const px = (i / this.peaks.length) * w;
                    c.strokeStyle = px < x ? "#bb84df" : "#537e7e";
                    c.beginPath();
                    c.moveTo(px, h / 2 + this.peaks[i][0] * h * 0.43);
                    c.lineTo(px, h / 2 + this.peaks[i][1] * h * 0.43);
                    c.stroke();
                }
                c.strokeStyle = "#ecd4ff";
                c.beginPath();
                c.moveTo(x, 0);
                c.lineTo(x, h);
                c.stroke();
            } else {
                c.fillStyle = "#9a8ca9";
                c.font = "11px system-ui";
                c.textAlign = "center";
                c.fillText(
                    "Your track’s waveform will appear here",
                    w / 2,
                    h / 2 - 9,
                );
            }
        }
        const plot = surface($("eq-plot"));
        if (plot) {
            const { c, w, h } = plot,
                left = 35,
                right = w - 37,
                top = 18,
                bottom = h - 28,
                width = right - left,
                height = bottom - top,
                rate = this.player.ctx?.sampleRate || 48000,
                maxHz = Math.min(20000, rate * 0.49);
            const x = (f) =>
                left + (Math.log(f / 20) / Math.log(maxHz / 20)) * width;
            const y = (db) => top + ((18 - db) / 36) * height;
            c.font = "11px system-ui";
            c.textAlign = "right";
            for (const db of [-18, -12, -6, 0, 6, 12, 18]) {
                c.strokeStyle = db === 0 ? "#61466f" : "#30263c";
                c.beginPath();
                c.moveTo(left, y(db));
                c.lineTo(right, y(db));
                c.stroke();
                c.fillStyle = "#bb9ed0";
                c.fillText(`${db > 0 ? "+" : ""}${db}`, left - 7, y(db) + 3);
            }
            c.textAlign = "center";
            for (const hz of [
                20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000,
            ]) {
                if (hz > maxHz) continue;
                const px = x(hz);
                c.strokeStyle = "#292331";
                c.beginPath();
                c.moveTo(px, top);
                c.lineTo(px, bottom);
                c.stroke();
                c.fillStyle = "#ad9eb9";
                c.fillText(hz >= 1000 ? `${hz / 1000}k` : hz, px, bottom + 16);
            }
            c.textAlign = "left";
            for (const db of [-90, -60, -30, 0]) {
                const py = top + (-db / 90) * height;
                c.fillStyle = "#7cbbb5";
                c.fillText(`${db}`, right + 7, py + 3);
            }
            c.save();
            c.beginPath();
            c.rect(left, top, width, height);
            c.clip();
            if (this.player.playing && this.player.analyser) {
                this.player.analyser.getFloatFrequencyData(this.bins);
                c.beginPath();
                c.moveTo(left, bottom);
                for (let i = 1; i < this.bins.length; i++) {
                    const hz = (i * rate) / 4096;
                    if (hz < 20 || hz > maxHz) continue;
                    const py =
                        top + (-Math.max(-90, this.bins[i]) / 90) * height;
                    c.lineTo(x(hz), py);
                }
                c.lineTo(right, bottom);
                c.closePath();
                c.fillStyle = "#4bbaad26";
                c.fill();
                c.strokeStyle = "#77cec3aa";
                c.lineWidth = 1;
                c.stroke();
            }
            c.beginPath();
            for (let i = 0; i < this.curve.length; i++) {
                const f = 20 * ((rate * 0.49) / 20) ** (i / 255);
                const px = x(f),
                    py = y(this.curve[i]);
                if (i === 0) c.moveTo(px, py);
                else c.lineTo(px, py);
            }
            c.strokeStyle = "#d7a0ff";
            c.lineWidth = 2;
            c.stroke();
            c.restore();
        }
        if (schedule !== false)
            setTimeout(
                () => requestAnimationFrame(this.draw),
                this.player.playing ? 25 : 130,
            );
    }
}
