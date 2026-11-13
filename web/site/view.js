import { openHelp } from "./help.js";
export const $ = (id) => document.getElementById(id);
export const time = (seconds) =>
    `${Math.floor(seconds / 60)}:${Math.floor(seconds % 60)
        .toString()
        .padStart(2, "0")}`;
export function status(text, error = false) {
    $("status").textContent = text;
    $("status").classList.toggle("status-error", error);
    const notice = $("import-error");
    notice.textContent = error ? text : "";
    notice.hidden = !error;
    if (error) {
        notice.scrollIntoView({ block: "center" });
        notice.focus({ preventScroll: true });
    }
}
export function page(name) {
    for (const key of ["overview", "eq", "report", "studio-tools"]) {
        $(key).hidden = key !== name;
        $(`tab-${key}`).classList.toggle("selected", key === name);
        if (key === name) $(`tab-${key}`).setAttribute("aria-current", "page");
        else $(`tab-${key}`).removeAttribute("aria-current");
    }
    document.dispatchEvent(new CustomEvent("studio-page", { detail: name }));
}
export function metrics(target, analysis) {
    const m = analysis.metrics;
    const values = [
        ["Loudness", m.integrated_lufs, "LUFS · integrated"],
        ["True peak", m.true_peak_dbtp, "dBTP · reconstructed"],
        ["Dynamics", m.crest_db, "dB · crest factor"],
    ];
    $(target).replaceChildren(
        ...values.map(([label, value, unit]) => {
            const box = document.createElement("div");
            box.className = "metric";
            const a = document.createElement("span"),
                b = document.createElement("strong"),
                c = document.createElement("small");
            a.textContent = label;
            b.textContent = Number.isFinite(value) ? value.toFixed(1) : "—";
            c.textContent = unit;
            box.append(a, b, c);
            return box;
        }),
    );
}
export function reportView(report) {
    $("report-empty").hidden = Boolean(report);
    $("save-report").disabled = !report;
    const content = $("report-content");
    content.replaceChildren();
    if (!report) return;
    const intro = document.createElement("p");
    intro.className = "muted";
    intro.textContent = `${report.profile} · Target ${report.target_lufs.toFixed(1)} LUFS · ${report.processing_seconds.toFixed(2)} seconds processing`;
    content.append(intro);
    const ul = document.createElement("ul");
    ul.className = "report-notes";
    for (const note of report.notes) {
        const li = document.createElement("li");
        li.textContent = note;
        ul.append(li);
    }
    content.append(ul);
    for (const stage of report.log) {
        const row = document.createElement("div");
        row.className = "report-stage";
        const h = document.createElement("h3"),
            p = document.createElement("p");
        h.textContent = stage.module.replaceAll("_", " ");
        p.textContent = `${stage.seconds.toFixed(3)} s · ${JSON.stringify(stage.detail)}`;
        row.append(h, p);
        content.append(row);
    }
    if (report.exports?.length) {
        const p = document.createElement("p");
        p.className = "teal";
        const e = report.exports[0];
        p.textContent = `Last downloaded export: ${e.format} · ${e.sample_rate} Hz · verified ${e.decoded_metrics.true_peak_dbtp.toFixed(2)} dBTP`;
        content.prepend(p);
    }
}
const names = ["Sub", "Bass", "Warmth", "Body", "Presence", "Clarity", "Air"];
const helps = ["bass", "bass", "lowmid", "mid", "presence", "air", "air"];
export function buildBands(settings, onChange, rate = 48000) {
    const short = ["Sub", "Bass", "Warm", "Body", "Pres.", "Clear", "Air"];
    $("band-picker").replaceChildren(
        ...short.map((label, index) => {
            const button = document.createElement("button");
            button.textContent = `${index + 1} ${label}`;
            button.setAttribute(
                "aria-label",
                `Edit band ${index + 1}, ${names[index]}`,
            );
            button.setAttribute("aria-pressed", String(index === 0));
            button.classList.toggle("selected", index === 0);
            button.onclick = () => {
                for (const [i, b] of [...$("band-picker").children].entries()) {
                    b.classList.toggle("selected", i === index);
                    b.setAttribute("aria-pressed", String(i === index));
                }
                for (const [i, b] of [...$("bands").children].entries())
                    b.classList.toggle("active", i === index);
            };
            return button;
        }),
    );
    $("bands").replaceChildren(
        ...settings.manual_eq.map((band, index) => {
            const box = document.createElement("div");
            box.className = `band${index === 0 ? " active" : ""}`;
            const name = document.createElement("button");
            name.className = "band-name";
            name.textContent = `${index + 1} · ${names[index]} ⓘ`;
            name.title = "Tap for a plain-language explanation";
            name.onclick = () => openHelp(helps[index]);
            box.append(name);
            function input(label, type, property, min, max, step, className) {
                const l = document.createElement("label");
                l.className = className;
                l.append(document.createTextNode(label));
                const el = document.createElement("input");
                el.type = type;
                el.min = min;
                el.max = max;
                el.step = step;
                el.value = band[property];
                el.setAttribute("aria-label", `${names[index]} ${label}`);
                el.dataset.band = index;
                el.dataset.parameter = property;
                el.oninput = () => {
                    const value = Number(el.value);
                    if (!Number.isFinite(value) || value < min || value > max)
                        return;
                    band[property] = value;
                    if (property === "gain")
                        gainValue.textContent = `${value > 0 ? "+" : ""}${value.toFixed(1)}`;
                    if (property === "freq")
                        shapeButton.textContent = `${value} Hz ▾`;
                    onChange();
                };
                el.onchange = () => {
                    el.value = band[property];
                };
                l.append(el);
                box.append(l);
                return el;
            }
            const on = document.createElement("label");
            on.className = "on-label";
            const checkbox = document.createElement("input");
            checkbox.type = "checkbox";
            checkbox.checked = band.on;
            checkbox.setAttribute("aria-label", `Enable ${names[index]} band`);
            checkbox.onchange = () => {
                band.on = checkbox.checked;
                onChange();
            };
            on.append(checkbox, "On");
            box.append(on);
            input("Gain dB", "range", "gain", -12, 12, 0.25, "gain-label");
            const gainValue = document.createElement("output");
            gainValue.textContent = `${band.gain > 0 ? "+" : ""}${band.gain.toFixed(1)}`;
            box.append(gainValue);
            input(
                "Hz",
                "number",
                "freq",
                20,
                Math.min(20000, Math.floor(rate * 0.49)),
                1,
                "frequency-label",
            );
            input("Q", "number", "q", 0.1, 18, 0.05, "q-label");
            const type = document.createElement("label");
            type.className = "type-label";
            type.append("Type");
            const select = document.createElement("select");
            select.setAttribute("aria-label", `${names[index]} filter type`);
            for (const [value, text] of [
                ["bell", "Bell"],
                ["low_shelf", "Low shelf"],
                ["high_shelf", "High shelf"],
                ["low_cut", "Low cut"],
                ["high_cut", "High cut"],
            ]) {
                const option = document.createElement("option");
                option.value = value;
                option.textContent = text;
                select.append(option);
            }
            select.value = band.type;
            select.onchange = () => {
                band.type = select.value;
                onChange();
            };
            type.append(select);
            box.append(type);
            const shapeButton = document.createElement("button");
            shapeButton.className = "band-shape";
            shapeButton.textContent = `${band.freq} Hz ▾`;
            shapeButton.setAttribute(
                "aria-label",
                `Frequency, Q and filter shape for ${names[index]}`,
            );
            shapeButton.setAttribute("aria-expanded", "false");
            shapeButton.onclick = () => {
                const expanded = box.classList.toggle("advanced");
                shapeButton.setAttribute("aria-expanded", String(expanded));
            };
            box.append(shapeButton);
            return box;
        }),
    );
}
export function download(bytes, name, type) {
    const url = URL.createObjectURL(new Blob([bytes], { type }));
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 60000);
}
