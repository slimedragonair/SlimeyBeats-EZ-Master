export function setText(element, text) {
    if (element.textContent !== text) element.textContent = text;
}

export function setOptions(select, entries, selected) {
    const key = JSON.stringify(entries);
    if (select.dataset.optionsKey !== key) {
        select.replaceChildren(...entries.map(([value, label]) => new Option(label, value)));
        select.dataset.optionsKey = key;
    }
    if (select.value !== (selected || "")) select.value = selected || "";
}

export function setInputValue(element, value) {
    if (element.type === "checkbox") element.checked = !!value;
    else if (element.value !== String(value ?? "") && document.activeElement !== element) element.value = value ?? "";
}
