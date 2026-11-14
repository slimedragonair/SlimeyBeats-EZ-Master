export const MIX_PENDING_NOTICE = "Stem controls are live on B. Update mix preview to commit before further edits or saving.";
const $ = (id) => document.getElementById(`s-${id}`);

export function wireStudioPreviewStatus() {
    $("quick-mix").onclick = () => $("mix").click();
    return function refresh(pendingMix, monitorMix) {
        $("quick-mix-row").hidden = !pendingMix;
        $("quick-mix").disabled = $("mix").disabled;
        $("compare-state").textContent = monitorMix
            ? "A Before · B Live stems · sample headroom; final true-peak guard on render/export"
            : "A Before · B Preview";
        const label = monitorMix ? "B · Live stem mix" : "B · Preview";
        $("after").querySelector(".compare-long").textContent = label;
        $("after").setAttribute("aria-label", label);
        $("preview-heading").textContent = pendingMix ? "Live mix · saved preview pending" : "Current preview";
    };
}
