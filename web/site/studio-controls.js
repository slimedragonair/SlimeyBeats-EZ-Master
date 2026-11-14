import { MULTITRACK_KINDS, STEM_KINDS, kindLabel } from "./studio-stems.js";
const element = (id) => document.getElementById(`s-${id}`);
const number = (id) => Number(element(id).value);
export function selection() {
    return {
        start_seconds: number("start"),
        end_seconds: number("end"),
        low_hz: number("low"),
        high_hz: number("high"),
        reduction_db: number("reduction"),
        mode: element("repair-mode").value,
    };
}
export function voiceSettings(defaults) {
    return {
        ...defaults,
        highpass_hz: number("highpass"),
        noise_reduction: number("noise"),
        deess: number("deess"),
        noise_region: element("learn-noise").checked
            ? {
                  start_seconds: number("noise-start"),
                  end_seconds: number("noise-end"),
              }
            : null,
        gate_threshold_db: element("gate-on").checked ? number("gate") : null,
        hold_loudness: element("hold").checked,
    };
}
export function stemInputs() {
    const kinds =
        element("mode").value === "two"
            ? ["vocals", "instrumental"]
            : element("mode").value === "four"
              ? ["vocals", "drums", "bass", "other"]
              : MULTITRACK_KINDS;
    element("stem-files").replaceChildren(
        ...kinds.map((kind, index) => {
            const slot = document.createElement("div");
            slot.className = "edit-file-slot";
            const label = document.createElement("label"),
                input = document.createElement("input");
            label.append(kindLabel(kind));
            input.type = "file";
            input.accept = "audio/*,.wav,.mp3,.flac,.m4a";
            input.dataset.kind = kind;
            label.append(input);
            if (element("mode").value === "multi") {
                const kindField = document.createElement("label"),
                    select = document.createElement("select");
                kindField.textContent = `Part ${index + 1} type`;
                select.setAttribute("aria-label", `Part ${index + 1} type`);
                select.title =
                    "Choose a unique type. Part 1–9 can hold any instrument or additional takes of the same instrument.";
                select.append(
                    ...STEM_KINDS.map(
                        (value) => new Option(kindLabel(value), value),
                    ),
                );
                select.value = kind;
                select.onchange = () => {
                    input.dataset.kind = select.value;
                    label.firstChild.textContent = kindLabel(select.value);
                };
                kindField.append(select);
                slot.append(kindField);
            }
            const clear = document.createElement("button");
            clear.textContent = "Clear file";
            clear.className = "s-clear-file";
            clear.setAttribute("aria-label", `Clear part ${index + 1} file`);
            clear.onclick = () => {
                input.value = "";
            };
            slot.append(label, clear);
            return slot;
        }),
    );
}
export function strengthReadouts() {
    for (const id of ["noise", "deess"]) {
        const input = element(id),
            output = document.createElement("output");
        output.htmlFor = input.id;
        const update = () => {
            output.textContent = `${Math.round(Number(input.value) * 100)}%`;
        };
        input.addEventListener("input", update);
        update();
        input.after(output);
    }
}
export function mixFaders(controls, changed, channels = 2, stemNames = {}) {
    const host = element("faders");
    host.replaceChildren();
    if (!controls.tracks.length) {
        const p = document.createElement("p");
        p.className = "muted";
        p.textContent =
            "Import 2–9 aligned stems to enable rebalancing. Voice cleanup and repair also work on a single recording.";
        host.append(p);
        return;
    }
    for (const track of controls.tracks) {
        const row = document.createElement("div"),
            title = document.createElement("strong"),
            fields = document.createElement("div");
        row.className = "edit-fader";
        fields.className = "edit-fader-controls";
        title.textContent = stemNames?.[track.kind] ?? kindLabel(track.kind);
        row.append(title, fields);
        for (const [key, text, min, max, step, help] of [
            [
                "gain_db",
                "Gain / dB",
                -60,
                12,
                0.5,
                "Changes this stem's level. 0 dB keeps its imported level.",
            ],
            [
                "balance",
                "Balance",
                -1,
                1,
                0.05,
                "Stereo balance: -1 left, 0 centered, +1 right. This is not stereo widening.",
            ],
        ]) {
            if (key === "balance" && channels === 1) continue;
            const label = document.createElement("label"),
                input = document.createElement("input"),
                output = document.createElement("output");
            label.textContent = text;
            label.title = help;
            input.type = "range";
            input.min = min;
            input.max = max;
            input.step = step;
            input.value = track[key];
            input.setAttribute("aria-label", `${track.kind} ${text}`);
            input.dataset.control = key;
            output.textContent = Number(track[key]).toFixed(
                key === "gain_db" ? 1 : 2,
            );
            input.oninput = () => {
                track[key] = Number(input.value);
                output.textContent = Number(input.value).toFixed(
                    key === "gain_db" ? 1 : 2,
                );
                changed();
            };
            label.append(input, output);
            fields.append(label);
        }
        for (const key of ["mute", "solo"]) {
            const label = document.createElement("label"),
                input = document.createElement("input");
            input.type = "checkbox";
            input.checked = track[key];
            input.setAttribute("aria-label", `${track.kind} ${key}`);
            input.onchange = () => {
                track[key] = input.checked;
                changed();
            };
            label.append(input, key);
            fields.append(label);
        }
        host.append(row);
    }
}
