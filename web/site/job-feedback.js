const $ = (id) => document.getElementById(id);
const text = (element, value) => { if (element.textContent !== value) element.textContent = value; };

// These buttons invoke the existing cancellation handlers and their guards.
export function wireJobFeedback(studioBusy) {
    $("job-cancel-master").onclick = () => $("cancel").click();
    $("job-cancel-studio").onclick = () => $("s-cancel").click();
    let queued = 0;
    function refresh() {
        // Workspace promises can settle between resize notifications. Commit
        // overlay geometry after that delivery pass, using the newest state.
        if (!queued) queued = requestAnimationFrame(() => {
            queued = requestAnimationFrame(() => { queued = 0; render(); });
        });
    }
    function render() {
        const master = !$("job").hidden, studio = studioBusy();
        $("job-feedback").hidden = !master && !studio;
        $("job-feedback-master").hidden = !master;
        $("job-feedback-studio").hidden = !studio;
        $("job-cancel-master").disabled = $("cancel").disabled;
        $("job-cancel-studio").disabled = $("s-cancel").disabled;
        $("job-master-progress").value = $("progress").value;
        text($("job-studio-status"), $("s-status").textContent);
    }
    new MutationObserver(refresh).observe($("s-status"), { childList: true });
    render();
    return { refresh };
}
