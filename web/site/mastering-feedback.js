import { $, page } from "./view.js";

export function wireMasteringFeedback() {
    const analysis = $("mobile-analysis-feedback");
    const master = $("mobile-master-feedback");
    const mobileExport = $("mobile-export");
    const outputState = $("output-results-state");
    const analysisButton = $("view-mobile-analysis");
    const masterButton = $("view-mobile-master");
    const mobileLayout = matchMedia("(max-width: 680px)");
    let focusedView;
    let prior;

    function viewResults(sectionId, headingId) {
        const section = $(sectionId);
        if ($("measurements").hidden || section.hidden) return;
        if ($("overview").hidden) page("overview");
        const heading = $(headingId);
        heading.focus({ preventScroll: true });
        heading.scrollIntoView({ block: "start", behavior: "instant" });
    }
    analysisButton.onclick = () => viewResults("measurements", "input-results-heading");
    masterButton.onclick = () => viewResults("output-section", "output-results-heading");
    document.addEventListener("focusin", ({ target }) => {
        focusedView = target === analysisButton || target === masterButton ? target : undefined;
    });
    document.addEventListener("pointerdown", ({ target }) => {
        if (!focusedView?.contains(target)) focusedView = undefined;
    });
    mobileLayout.addEventListener("change", ({ matches }) => {
        if (matches || !focusedView || !document.hasFocus()) return;
        // A hidden control can lose focus before the media-query event arrives.
        if (document.activeElement !== focusedView && document.activeElement !== document.body) return;
        const action = $(focusedView === analysisButton ? "analyze" : "master");
        const next = action.disabled ? $("track-title") : action;
        next.focus({ preventScroll: true });
        next.scrollIntoView({ block: "nearest", behavior: "instant" });
    });

    // Use the same preview/signature state that gates export, not status text or a second job state.
    return ({ analyzed, mastered, stale, refining }) => {
        if (prior?.analyzed === analyzed && prior.mastered === mastered && prior.stale === stale && prior.refining === refining) return;
        prior = { analyzed, mastered, stale, refining };
        analysis.hidden = !analyzed;
        master.hidden = !mastered;
        master.toggleAttribute("data-stale", stale && !refining);
        if (stale || (mastered && refining)) mobileExport.setAttribute("aria-describedby", "mobile-master-message");
        else mobileExport.removeAttribute("aria-describedby");
        outputState.textContent = stale ? "Previous master" : "Ready to audition";
        $("mobile-master-state").textContent = refining ? "Re-mastering…" : stale ? "Re-master to export these settings" : "Master ready";
        $("mobile-master-hint").textContent = refining ? "Export will be available when the new preview is ready." : stale ? "B still plays the previous master." : "Play B to hear the master.";
    };
}
