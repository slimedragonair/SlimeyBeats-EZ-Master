import {
    MULTITRACK_KINDS,
    kindOptions,
    nextPartKind,
    kindLabel,
} from "./studio-stems.js";
const element = (id) => document.getElementById(`s-${id}`);

function refreshKinds(host) {
    const options = kindOptions(
        [...host.querySelectorAll("input")].map((input) => input.dataset.kind),
    );
    for (const select of host.querySelectorAll("select")) {
        const current = select.closest(".edit-file-slot").querySelector("input")
            .dataset.kind;
        select.replaceChildren(
            ...options.map((kind) => new Option(kindLabel(kind), kind)),
        );
        select.value = current;
    }
}

function stemInput(kind, index, editable) {
    const slot = document.createElement("div"),
        label = document.createElement("label"),
        input = document.createElement("input");
    slot.className = "edit-file-slot";
    label.append(kindLabel(kind));
    input.type = "file";
    input.accept = "audio/*,.wav,.mp3,.flac,.m4a";
    input.dataset.kind = kind;
    label.append(input);
    if (editable) {
        const kindField = document.createElement("label"),
            select = document.createElement("select");
        kindField.textContent = `Part ${index + 1} type`;
        select.setAttribute("aria-label", `Part ${index + 1} type`);
        select.title =
            "Choose a unique type. Numbered parts can hold any instrument or additional song layer.";
        select.onchange = () => {
            input.dataset.kind = select.value;
            label.firstChild.textContent = kindLabel(select.value);
            refreshKinds(element("stem-files"));
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
}

export function stemInputs() {
    const mode = element("mode").value,
        host = element("stem-files");
    const kinds =
        mode === "two"
            ? ["vocals", "instrumental"]
            : mode === "four"
              ? ["vocals", "drums", "bass", "other"]
              : MULTITRACK_KINDS;
    host.replaceChildren(
        ...kinds.map((kind, index) => stemInput(kind, index, mode === "multi")),
    );
    element("add-part").hidden = mode !== "multi";
    refreshKinds(host);
}

export function addStemInput() {
    if (element("mode").value !== "multi") return;
    const host = element("stem-files"),
        inputs = [...host.querySelectorAll("input")];
    const row = stemInput(
        nextPartKind(inputs.map((input) => input.dataset.kind)),
        inputs.length,
        true,
    );
    host.append(row);
    refreshKinds(host);
    row.querySelector("input").focus({ preventScroll: true });
    row.scrollIntoView({ block: "center" });
}
