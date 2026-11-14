export const MIX_PENDING_NOTICE = "Mix edits pending. Playback uses the previous preview. Update mix preview to hear your changes.";
const $ = (id) => document.getElementById(`s-${id}`);

export function wireStudioPreviewStatus() {
    $("quick-mix").onclick = () => $("mix").click();
    return function refresh(pendingMix) {
        $("quick-mix-row").hidden = !pendingMix;
        $("quick-mix").disabled = $("mix").disabled;
        $("compare-state").textContent = pendingMix ? "A Before · B Old mix" : "A Before · B Preview";
        $("after").querySelector(".compare-long").textContent = pendingMix ? "B · Previous preview" : "B · Preview";
        $("after").setAttribute("aria-label", pendingMix ? "B · Previous preview, mix edits pending" : "B · Preview");
        $("preview-heading").textContent = pendingMix ? "Previous preview · edits pending" : "Current preview";
    };
}
