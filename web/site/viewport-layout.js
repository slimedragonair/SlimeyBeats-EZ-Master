// Measure fixed controls; waveform/listening details remain in document flow.
export function wireViewportLayout() {
    const root = document.documentElement, body = document.body;
    const help = document.querySelector(".help-dock"), jobs = document.getElementById("job-feedback");
    const players = [document.querySelector(".playback"), document.querySelector(".edit-player")];
    const details = [document.getElementById("playback-details"), document.getElementById("s-playback-details")];
    const toggle = document.getElementById("playback-details-toggle");
    let queued = 0, previousWorkspace;
    function variable(name, value) {
        if (root.style.getPropertyValue(name) !== value) root.style.setProperty(name, value);
    }
    function flag(name, value) {
        if (body.classList.contains(name) !== value) body.classList.toggle(name, value);
    }
    function activeDetails() { return details[body.classList.contains("studio-mode") ? 1 : 0]; }
    function measure() {
        const large = parseFloat(getComputedStyle(root).fontSize) >= 24;
        const compact = root.clientWidth <= 940 || innerHeight <= 780 || large;
        const workspace = large || (root.clientWidth <= 680 && innerHeight <= 650);
        flag("large-text", large);
        flag("compact-player", compact);
        flag("compact-workspace", workspace);
        if (workspace !== previousWorkspace) {
            previousWorkspace = workspace;
            document.dispatchEvent(new CustomEvent("interface-layout", { detail: { compactWorkspace: workspace } }));
        }
        const studio = body.classList.contains("studio-mode");
        const loaded = studio ? document.getElementById("studio-tools").dataset.loaded === "true" : body.classList.contains("master-loaded");
        toggle.hidden = !loaded;
        help.querySelector(".help-dock-note").hidden = loaded;
        toggle.setAttribute("aria-controls", activeDetails().id);
        toggle.setAttribute("aria-expanded", String(activeDetails().open));
        toggle.setAttribute("aria-label", `${activeDetails().open ? "Hide" : "Show"} ${studio ? "Studio" : "Mastering"} playback details`);
        const panel = document.querySelector(".studio"), box = panel.getBoundingClientRect(), style = getComputedStyle(panel);
        variable("--studio-left", `${Math.ceil(box.left + parseFloat(style.paddingLeft) + parseFloat(style.borderLeftWidth))}px`);
        variable("--studio-right", `${Math.max(12, Math.ceil(innerWidth - box.right + parseFloat(style.paddingRight) + parseFloat(style.borderRightWidth)))}px`);
        variable("--studio-player-bottom", `${Math.max(0, Math.ceil(innerHeight - box.bottom + parseFloat(style.paddingBottom) + parseFloat(style.borderBottomWidth)))}px`);
        variable("--help-dock-height", `${Math.ceil(help.getBoundingClientRect().height)}px`);
        variable("--playback-height", `${Math.ceil(Math.max(...players.map((p) => p.getBoundingClientRect().height)))}px`);
        variable("--job-overlay-height", `${Math.ceil(jobs.getBoundingClientRect().height)}px`);
    }
    function schedule() {
        if (!queued) queued = requestAnimationFrame(() => { queued = 0; measure(); });
    }
    toggle.onclick = () => {
        const section = activeDetails();
        section.open = !section.open;
        if (section.open) requestAnimationFrame(() => section.scrollIntoView({ block: "start" }));
        schedule();
    };
    const observer = new ResizeObserver(schedule);
    for (const element of [root, help, jobs, document.querySelector(".studio"), ...players]) observer.observe(element);
    new MutationObserver(schedule).observe(root, { attributes: true, attributeFilter: ["style"] });
    new MutationObserver(schedule).observe(body, { attributes: true, attributeFilter: ["class"] });
    for (const section of details) section.addEventListener("toggle", schedule);
    document.addEventListener("studio-page", schedule);
    window.addEventListener("resize", schedule);
    measure();
}
