export function wireDialogFocus(dialog) {
    let opener;
    dialog.addEventListener("keydown", (event) => {
        if (event.key !== "Tab") return;
        const controls = [...dialog.querySelectorAll(
            "button, a[href], input, select, summary, [tabindex='0']",
        )].filter((element) => !element.disabled && element.getClientRects().length);
        if (!controls.length) { event.preventDefault(); return; }
        const first = controls[0], last = controls.at(-1), focused = document.activeElement;
        // Firefox also tabs to scroll panes. Let native focus advance within
        // the dialog instead of cycling an unlisted pane back to an endpoint.
        const outside = !dialog.contains(focused);
        if (event.shiftKey && (focused === first || outside)) {
            event.preventDefault(); last.focus();
        } else if (!event.shiftKey && (focused === last || outside)) {
            event.preventDefault(); first.focus();
        }
    });
    dialog.addEventListener("close", () => {
        const target = opener?.isConnected && opener !== document.body && opener.getClientRects().length
            ? opener : document.getElementById("open-learning");
        target?.focus({ preventScroll: true });
    });
    return {
        open() {
            if (dialog.open) return;
            opener = document.activeElement;
            dialog.showModal();
        },
    };
}
