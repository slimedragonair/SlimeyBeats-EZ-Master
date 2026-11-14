export function wireWorkspaceNavigation() {
    const pages = document.getElementById("page-navigation");
    const controls = document.getElementById("control-detail-panel");
    const names = { overview: "Overview", eq: "Manual EQ", report: "Report", "studio-tools": "Studio" };
    let compact = false;
    document.addEventListener("interface-layout", (event) => {
        compact = event.detail.compactWorkspace;
        pages.open = controls.open = !compact;
        document.querySelector(".skip-link").setAttribute("href", compact ? "#page-navigation-summary" : "#workspace-controls");
    });
    document.addEventListener("studio-page", (event) => {
        document.getElementById("current-page-name").textContent = names[event.detail];
        if (compact) {
            const focusedTab = document.activeElement?.closest(".tabs");
            pages.open = false;
            if (focusedTab) document.getElementById("page-navigation-summary").focus({ preventScroll: true });
        }
    });
}
