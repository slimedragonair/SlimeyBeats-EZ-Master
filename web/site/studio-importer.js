import { MAX_STEMS, STEM_KINDS, kindLabel } from "./studio-stems.js";
const $ = (id) => document.getElementById(`s-${id}`);

export class StemImporter {
    constructor() {
        this.busy = false;
        this.clear();
    }
    clear() {
        this.plan = null;
        this.files = null;
        this.archiveId = null;
        $("bulk-review").hidden = true;
        $("bulk-parts").replaceChildren();
        $("bulk-action-summary").textContent = "";
        $("bulk-omissions").hidden = true;
        $("bulk-omissions").open = false;
        $("bulk-omitted-parts").replaceChildren();
    }
    open(plan, files, archiveId = null) {
        this.plan = plan;
        this.files = files;
        this.archiveId = archiveId;
        this.selected = new Set(
            plan.parts.slice(0, MAX_STEMS).map((_, i) => i),
        );
        this.kinds = plan.parts.map((part) => part.kind);
        $("bulk-format").value = "wav";
        $("bulk-omissions").open = false;
        $("bulk-review").hidden = false;
        this.render();
    }
    selection() {
        if (!this.plan) throw new Error("Choose stem files or a ZIP first.");
        const preferred = $("bulk-format").value;
        const chosen = [...this.selected]
            .sort((a, b) => a - b)
            .map((index) => {
                const part = this.plan.parts[index];
                const version = this.version(part, preferred);
                return { kind: this.kinds[index], version };
            });
        if (chosen.length < 2 || chosen.length > MAX_STEMS)
            throw new Error("Select 2–9 distinct parts.");
        if (new Set(chosen.map((part) => part.kind)).size !== chosen.length)
            throw new Error(
                "Part types must be unique. Use Part 1–9 for extra instruments.",
            );
        return chosen;
    }
    version(part, preferred) {
        return (
            part.versions.find((file) => file.format === preferred) ||
            part.versions.find((file) => file.format === "wav") ||
            part.versions[0]
        );
    }
    render() {
        const preferred = $("bulk-format").value;
        $("bulk-summary").textContent =
            `${this.plan.parts.length} distinct parts · ${this.selected.size} / ${MAX_STEMS} selected · ${this.plan.skipped_files} non-audio/metadata files skipped`;
        const fallbackCount = [...this.selected].filter(
            (index) =>
                this.version(this.plan.parts[index], preferred).format !==
                preferred,
        ).length;
        const omitted = this.plan.parts.filter(
            (_, index) => !this.selected.has(index),
        );
        $("bulk-action-summary").textContent =
            `${this.plan.parts.length} found · ${this.selected.size} / ${MAX_STEMS} selected` +
            `${omitted.length ? ` · ${omitted.length} not selected` : ""} · Prefer ${preferred.toUpperCase()}. ` +
            (fallbackCount
                ? `${fallbackCount} selected ${fallbackCount === 1 ? "part uses" : "parts use"} the available-format fallback.`
                : `Missing ${preferred.toUpperCase()} uses the available format.`);
        $("bulk-omissions").hidden = !omitted.length;
        const shortNames = omitted.slice(0, 3).map((part) => {
            const chars = [...part.name];
            return chars.length > 28
                ? `${chars.slice(0, 27).join("")}…`
                : part.name;
        });
        $("bulk-omitted-summary").textContent =
            `Left out: ${shortNames.join(", ")}${omitted.length > 3 ? " …" : ""} · Show all ${omitted.length}`;
        $("bulk-omitted-summary").title =
            omitted.length > 3
                ? `Expand to inspect all ${omitted.length} omitted parts.`
                : omitted.map((part) => part.name).join("\n");
        $("bulk-omitted-parts").replaceChildren(
            ...omitted.map((part) => {
                const item = document.createElement("li");
                item.textContent = part.name;
                return item;
            }),
        );
        $("bulk-parts").replaceChildren(
            ...this.plan.parts.map((part, index) => {
                const row = document.createElement("div"),
                    label = document.createElement("label"),
                    check = document.createElement("input"),
                    select = document.createElement("select"),
                    note = document.createElement("small");
                row.className = "edit-bulk-part";
                check.type = "checkbox";
                check.checked = this.selected.has(index);
                check.disabled =
                    !check.checked && this.selected.size >= MAX_STEMS;
                check.setAttribute("aria-label", `Include ${part.name}`);
                check.onchange = () => {
                    if (check.checked) {
                        const used = new Set(
                            [...this.selected].map((i) => this.kinds[i]),
                        );
                        if (used.has(this.kinds[index]))
                            this.kinds[index] =
                                STEM_KINDS.find(
                                    (kind) =>
                                        kind.startsWith("part_") &&
                                        !used.has(kind),
                                ) || this.kinds[index];
                        this.selected.add(index);
                    } else this.selected.delete(index);
                    this.render();
                };
                label.append(check, part.name);
                select.setAttribute("aria-label", `${part.name} part type`);
                select.append(
                    ...STEM_KINDS.map(
                        (kind) => new Option(kindLabel(kind), kind),
                    ),
                );
                select.value = this.kinds[index];
                select.disabled = !check.checked;
                select.onchange = () => {
                    this.kinds[index] = select.value;
                };
                const version = this.version(part, preferred);
                note.textContent = `${version.format.toUpperCase()} · ${version.name}${part.versions.length > 1 ? " · alternate encoding skipped" : ""}`;
                row.append(label, select, note);
                return row;
            }),
        );
        this.sync(this.busy);
    }
    async file(part, client) {
        if (this.files) return this.files[part.version.index];
        const { bytes } = await client.call("archive-read", {
            archive_id: this.archiveId,
            index: part.version.index,
        });
        return new File([bytes], part.version.name.split("/").at(-1), {
            type: part.version.format === "wav" ? "audio/wav" : "audio/mpeg",
        });
    }
    sync(busy) {
        this.busy = busy;
        for (const input of $("bulk-parts").querySelectorAll("input"))
            input.disabled =
                busy || (!input.checked && this.selected.size >= MAX_STEMS);
        for (const [index, input] of [
            ...$("bulk-parts").querySelectorAll("select"),
        ].entries())
            input.disabled = busy || !this.selected.has(index);
        for (const id of [
            "bulk-choose",
            "zip-choose",
            "bulk-discard",
            "import-batch",
        ])
            $(id).disabled = busy;
        $("import-batch").disabled ||= !this.plan || this.selected.size < 2;
    }
}
