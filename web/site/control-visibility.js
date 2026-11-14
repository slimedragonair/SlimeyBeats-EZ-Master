const $ = (id) => document.getElementById(id);

function customSettings(settings, defaults) {
    if (!settings || !defaults) return [];
    const active = [];
    if (settings.profile.includes(":")) active.push("profile blend");
    if (settings.target_lufs !== null) active.push(`target ${settings.target_lufs} LUFS`);
    if (settings.ceiling_dbtp !== defaults.ceiling_dbtp)
        active.push(`peak ceiling ${settings.ceiling_dbtp} dBTP`);
    if (settings.intensity !== defaults.intensity) active.push(`intensity ${settings.intensity}`);
    if (settings.safety !== defaults.safety) active.push("punch protection off");
    if (settings.reference) active.push("reference track");
    if (settings.dither !== defaults.dither) active.push("dither off");
    const shaped = settings.manual_eq.filter((band, i) => band.on &&
        ["freq", "q", "type"].some((key) => band[key] !== defaults.manual_eq[i][key]));
    if (shaped.length) active.push(`${shaped.length} custom EQ band shape${shaped.length === 1 ? "" : "s"}`);
    return active;
}

function studioSettings() {
    const active = [];
    if (Number($("s-highpass").value) !== 75) active.push(`rumble filter ${$("s-highpass").value} Hz`);
    if ($("s-learn-noise").checked) active.push("noise-only region");
    if ($("s-gate-on").checked) active.push(`quiet-gap gate ${$("s-gate").value} dBFS`);
    if (!$("s-hold").checked) active.push("cleanup loudness hold off");
    return active;
}

export function wireControlVisibility(preferences, snapshot) {
    function refresh() {
        const advanced = preferences.state.advanced;
        document.body.classList.toggle("basic-controls", !advanced);
        for (const mode of ["basic", "advanced"]) {
            const selected = (mode === "advanced") === advanced;
            $(`controls-${mode}`).classList.toggle("selected", selected);
            $(`controls-${mode}`).setAttribute("aria-pressed", String(selected));
        }
        $("controls-description").textContent = "Settings stay intact in either view.";
        $("current-control-view").textContent = advanced ? "Advanced controls" : "Basic controls";
        const { settings, defaults } = snapshot();
        const active = document.body.classList.contains("studio-mode")
            ? studioSettings() : customSettings(settings, defaults);
        $("advanced-notice").hidden = advanced || !active.length;
        $("advanced-notice").textContent = active.length
            ? `Advanced settings active: ${active.join(" · ")}. They still affect processing. Choose Advanced to inspect them.` : "";
        for (const button of document.querySelectorAll(".band-shape"))
            button.setAttribute("aria-expanded", String(advanced));
    }
    function choose(advanced) {
        preferences.setAdvanced(advanced);
        if (document.body.classList.contains("compact-workspace")) {
            $("control-detail-panel").open = false;
            $("control-detail-summary").focus({ preventScroll: true });
        }
    }
    $("controls-basic").onclick = () => choose(false);
    $("controls-advanced").onclick = () => choose(true);
    document.addEventListener("show-advanced-controls", () => {
        preferences.setAdvanced(true);
        $("controls-advanced").focus({ preventScroll: true });
    });
    document.addEventListener("input", refresh);
    document.addEventListener("change", refresh);
    document.addEventListener("studio-page", refresh);
    preferences.subscribe(refresh);
    return { refresh };
}
