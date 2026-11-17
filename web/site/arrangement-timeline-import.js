import { setOptions, setText } from "./arrangement-timeline-fields.js";

export function wireArrangementImport(host, { review, add, discard, getState, notice }) {
    const $ = name => host.querySelector(`[data-arr="${name}"]`);
    const rows = new Map();
    let plan = null, selected = new Set(), versions = new Map(), generation = 0;
    let disposed = false;
    function version(part) {
        return part.versions.find(value => value.id === versions.get(part.id)) ||
            part.versions.find(value => value.format === $("import-format").value) || part.versions[0];
    }
    function render() {
        if (disposed) return;
        $("import-review").hidden = !plan;
        if (!plan) { rows.clear(); $("import-parts").replaceChildren(); return; }
        const locked = getState().busy;
        $("import-summary").textContent = `${plan.parts.length} parts found · ${selected.size} selected. Each selected part adds a track; existing tracks stay in place.`;
        const wanted = new Set(plan.parts.map(part => part.id));
        for (const [id, entry] of rows) if (!wanted.has(id)) { entry.row.remove(); rows.delete(id); }
        plan.parts.forEach((part, index) => {
            let entry = rows.get(part.id);
            if (!entry) {
                const row = document.createElement("div"); row.className = "arr-import-part";
                const label = document.createElement("label"), checkbox = document.createElement("input"), name = document.createElement("span");
                checkbox.type = "checkbox";
                checkbox.onchange = () => { checkbox.checked ? selected.add(part.id) : selected.delete(part.id); render(); };
                label.append(checkbox, name);
                const select = document.createElement("select");
                select.onchange = () => { versions.set(part.id, select.value); };
                row.append(label, select); entry = { row, checkbox, name, select }; rows.set(part.id, entry);
            }
            entry.checkbox.checked = selected.has(part.id); entry.checkbox.disabled = locked;
            entry.checkbox.setAttribute("aria-label", `Include ${part.name}`); setText(entry.name, ` ${part.name}`);
            entry.select.setAttribute("aria-label", `Version for ${part.name}`); entry.select.disabled = locked;
            setOptions(entry.select, part.versions.map(value => [value.id, value.label || value.format.toUpperCase()]), version(part).id);
            if ($("import-parts").children[index] !== entry.row)
                $("import-parts").insertBefore(entry.row, $("import-parts").children[index] || null);
        });
        const omitted = plan.parts.filter(part => !selected.has(part.id));
        const omissions = omitted.length ? `Left out: ${omitted.map(part => part.name).join(", ")}.` : "All parts selected. One encoding per part is used.";
        const skipped = plan.skipped_files ? ` ${plan.skipped_files} unsupported or non-audio files were skipped.` : "";
        $("import-omissions").textContent = omissions + skipped;
        $("import-commit").disabled = locked || !selected.size;
        for (const name of ["import-all", "import-none", "import-format", "import-cancel"]) $(name).disabled = locked;
    }
    async function choose(files) {
        if (disposed || !files.length || getState().busy || !getState().playable) return;
        const token = ++generation;
        try {
            if (plan) {
                if (!await discard(plan)) return;
                plan = null; selected.clear(); versions.clear(); render();
            }
            const next = await review(files, { preferred_format: $("import-format").value });
            if (token !== generation || !next) return;
            if (!next.id || !Array.isArray(next.parts) || next.parts.some(part => !part.id || !part.name || !part.versions?.length))
                throw new Error("The arrangement import review is incomplete. Existing audio was kept.");
            plan = next; selected = new Set(next.parts.map(part => part.id)); versions.clear(); render();
            $("import-review").scrollIntoView({ block: "nearest" });
        } catch (error) { notice(error.message || String(error), true); }
    }
    $("add-files").onclick = () => { if (!getState().busy) $("files").click(); };
    $("add-zip").onclick = () => { if (!getState().busy) $("zip").click(); };
    for (const name of ["files", "zip"]) $(name).onchange = async () => {
        const files = [...$(name).files]; $(name).value = ""; await choose(files);
    };
    $("import-format").onchange = () => { versions.clear(); render(); };
    $("import-all").onclick = () => { if (plan && !getState().busy) { selected = new Set(plan.parts.map(part => part.id)); render(); } };
    $("import-none").onclick = () => { if (!getState().busy) { selected.clear(); render(); } };
    $("import-commit").onclick = async () => {
        if (!plan || getState().busy || !getState().playable || !selected.size) return;
        const current = plan;
        const selection = current.parts.filter(part => selected.has(part.id)).map(part => ({ part_id: part.id, version_id: version(part).id }));
        if (await add(current, selection)) { plan = null; selected.clear(); versions.clear(); render(); }
    };
    $("import-cancel").onclick = async () => {
        if (getState().busy) return;
        if (plan && !await discard(plan)) return;
        plan = null; ++generation; selected.clear(); versions.clear(); render();
        notice("File selection discarded. Existing tracks and source audio are unchanged.");
    };
    return { sync: render, choose, reset() { ++generation; plan = null; selected.clear(); versions.clear(); render(); },
        dispose() { disposed = true; ++generation; plan = null; selected.clear(); versions.clear(); rows.clear(); } };
}
