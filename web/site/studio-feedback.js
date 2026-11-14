// Keep processing unchanged: this module only locates errors and manages focus.
export function wireStudioFeedback(isActive) {
    const host = document.getElementById("studio-tools");
    const alert = document.createElement("p"), fieldMessage = document.createElement("small");
    alert.id = "s-action-error";
    alert.className = "edit-local-error";
    alert.role = "alert";
    alert.tabIndex = -1;
    alert.hidden = true;
    fieldMessage.id = "s-field-error";
    fieldMessage.className = "edit-field-error";
    let clicked, origin, invalid;
    host.append(alert);
    document.addEventListener("click", (event) => {
        const button = event.target.closest("button");
        if (button && host.contains(button)) clicked = button;
    }, true);
    function clear() {
        alert.hidden = true;
        invalid?.removeAttribute("aria-invalid");
        if (invalid) {
            const ids = (invalid.getAttribute("aria-describedby") || "").split(" ").filter((id) => id !== fieldMessage.id);
            if (ids.length) invalid.setAttribute("aria-describedby", ids.join(" "));
            else invalid.removeAttribute("aria-describedby");
        }
        fieldMessage.remove();
        invalid = null;
    }
    function begin() {
        origin = clicked || (host.contains(document.activeElement) ? document.activeElement.closest("button") : null);
        clicked = null;
        clear();
    }
    function error(message) {
        clear();
        origin = clicked || origin;
        clicked = null;
        const fallback = host.querySelector(".edit-visual > div:not([hidden]) button.primary") || document.getElementById("s-status");
        const anchor = origin?.isConnected ? origin : fallback;
        anchor.after(alert);
        alert.textContent = message;
        alert.hidden = false;
        const fieldId = message.includes("high-pass") ? "s-highpass"
            : message.includes("gate threshold") ? "s-gate" : null;
        if (fieldId) {
            invalid = document.getElementById(fieldId);
            invalid.setAttribute("aria-invalid", "true");
            invalid.setAttribute("aria-describedby", `${invalid.getAttribute("aria-describedby") || ""} ${fieldMessage.id}`.trim());
            fieldMessage.textContent = message;
            invalid.closest("label").append(fieldMessage);
            if (isActive() && document.body.classList.contains("basic-controls"))
                document.dispatchEvent(new CustomEvent("show-advanced-controls"));
        }
        // Jobs unlock their inputs in finally before the next frame. Focusing
        // sooner would silently fail on the disabled correction field.
        requestAnimationFrame(() => {
            if (!isActive() || alert.hidden) return;
            const target = invalid || alert;
            target.focus({ preventScroll: true });
            target.scrollIntoView({ block: "center" });
        });
    }
    host.addEventListener("input", (event) => {
        if (event.target === invalid) clear();
    });
    return { begin, error };
}
