const colors = [
    [14, 11, 21],
    [64, 29, 101],
    [160, 83, 206],
    [112, 220, 207],
    [242, 245, 220],
];
const envelopes = new WeakMap();
function peaks(buffer, width) {
    const cached = envelopes.get(buffer);
    if (cached?.width === width) return cached.values;
    const channels = Array.from(
        { length: buffer.numberOfChannels },
        (_, channel) => buffer.getChannelData(channel),
    );
    const values = new Float32Array(width);
    for (let x = 0; x < width; x++) {
        const start = Math.floor((x / width) * buffer.length),
            end = Math.min(
                buffer.length,
                Math.max(
                    start + 1,
                    Math.floor(((x + 1) / width) * buffer.length),
                ),
            );
        for (const samples of channels)
            for (let i = start; i < end; i++)
                values[x] = Math.max(values[x], Math.abs(samples[i]));
    }
    envelopes.set(buffer, { width, values });
    return values;
}
function color(db) {
    const position = Math.max(0, Math.min(4, ((db + 100) / 85) * 4)),
        i = Math.min(3, Math.floor(position)),
        f = position - i;
    return colors[i].map((v, c) =>
        Math.round(v * (1 - f) + colors[i + 1][c] * f),
    );
}
function backing(canvas) {
    const ratio = devicePixelRatio || 1,
        width = Math.max(1, canvas.clientWidth),
        height = Math.max(1, canvas.clientHeight);
    if (
        canvas.width !== Math.round(width * ratio) ||
        canvas.height !== Math.round(height * ratio)
    ) {
        canvas.width = Math.round(width * ratio);
        canvas.height = Math.round(height * ratio);
    }
    const ctx = canvas.getContext("2d");
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, width, height);
    return { ctx, width, height };
}
export function waveform(canvas, playback) {
    const { ctx, width, height } = backing(canvas);
    ctx.fillStyle = "#100c18";
    ctx.fillRect(0, 0, width, height);
    ctx.font = "11px system-ui";
    for (const [index, buffer] of [playback.before, playback.after].entries()) {
        if (!buffer) continue;
        const envelope = peaks(buffer, width),
            lane = height / 2,
            center = lane * (index + 0.5),
            traceHeight = lane / 2 - 8;
        ctx.strokeStyle = index ? "#78dbd0" : "#bcb0c8";
        ctx.beginPath();
        for (let x = 0; x < width; x++) {
            const size = Math.min(envelope[x], 1) * traceHeight;
            ctx.moveTo(x, center - size);
            ctx.lineTo(x, center + size);
        }
        ctx.stroke();
        ctx.fillStyle = index ? "#78dbd0" : "#bcb0c8";
        ctx.fillText(index ? "SAVED PREVIEW" : "BEFORE", 6, index * lane + 12);
    }
    if (playback.after) {
        const x = (playback.position() / playback.after.duration) * width;
        ctx.strokeStyle = "#ecd4b8";
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, height);
        ctx.stroke();
    }
}
export function spectrum(canvas, image, selection) {
    const { ctx, width, height } = backing(canvas),
        chart = {
            x: 48,
            y: 10,
            width: Math.max(1, width - 58),
            height: height - 42,
        };
    if (!image) {
        ctx.fillStyle = "#bcb0c8";
        ctx.font = "13px system-ui";
        ctx.fillText("Build the selected source's spectrogram", 12, 30);
        return chart;
    }
    const bitmap = document.createElement("canvas");
    bitmap.width = image.columns;
    bitmap.height = image.rows;
    const target = bitmap.getContext("2d"),
        pixels = target.createImageData(image.columns, image.rows);
    for (let x = 0; x < image.columns; x++)
        for (let y = 0; y < image.rows; y++) {
            const c = color(image.levels_dbfs[x * image.rows + y]),
                p = ((image.rows - 1 - y) * image.columns + x) * 4;
            pixels.data.set([...c, 255], p);
        }
    target.putImageData(pixels, 0, 0);
    ctx.drawImage(bitmap, chart.x, chart.y, chart.width, chart.height);
    const high = image.frequencies_hz.at(-1),
        timeX = (seconds) =>
            chart.x + (seconds / image.duration_seconds) * chart.width,
        frequencyY = (frequency) =>
            chart.y +
            chart.height *
                (1 -
                    Math.log(Math.max(20, frequency) / 20) /
                        Math.log(high / 20));
    ctx.fillStyle = "#e4d7ee";
    ctx.font = "13px system-ui";
    ctx.textAlign = "right";
    for (const hz of [20, 100, 1000, 10000])
        if (hz <= high)
            ctx.fillText(
                hz >= 1000 ? `${hz / 1000}k` : `${hz}`,
                chart.x - 6,
                frequencyY(hz) + 4,
            );
    for (let i = 0; i <= 4; i++) {
        ctx.textAlign = i === 0 ? "left" : i === 4 ? "right" : "center";
        ctx.fillText(
            `${((image.duration_seconds * i) / 4).toFixed(1)}s`,
            chart.x + (i / 4) * chart.width,
            height - 12,
        );
    }
    const x = timeX(selection.start_seconds),
        y = frequencyY(selection.high_hz),
        w = timeX(selection.end_seconds) - x,
        h = frequencyY(selection.low_hz) - y;
    ctx.fillStyle = "#dfa9ff22";
    ctx.fillRect(x, y, w, h);
    ctx.lineWidth = 4;
    ctx.strokeStyle = "#100c18";
    ctx.strokeRect(x, y, w, h);
    ctx.lineWidth = 1.5;
    ctx.strokeStyle = "#fff0ff";
    ctx.strokeRect(x, y, w, h);
    for (const [cornerX, cornerY, dx, dy] of [
        [x, y, 1, 1],
        [x + w, y, -1, 1],
        [x, y + h, 1, -1],
        [x + w, y + h, -1, -1],
    ]) {
        const size = Math.min(7, Math.abs(w) / 3, Math.abs(h) / 3);
        ctx.beginPath();
        ctx.moveTo(cornerX + dx * size, cornerY);
        ctx.lineTo(cornerX, cornerY);
        ctx.lineTo(cornerX, cornerY + dy * size);
        ctx.lineWidth = 3;
        ctx.stroke();
    }
    ctx.lineWidth = 1;
    return chart;
}
