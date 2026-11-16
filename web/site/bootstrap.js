const STARTUP_TIMEOUT_MS = 30000;

function startupFailed() {
    if (document.documentElement.dataset.engineState === "ready") return;
    document.documentElement.dataset.engineState = "failed";
    document.getElementById("engine-status").textContent = "Engine unavailable";
    const status = document.getElementById("status");
    // Module errors may contain URLs, paths or stack traces. Reporting uses its
    // existing category allowlist; the startup message never copies the error.
    status.textContent = "The local audio engine could not start. Reload this page to retry, or choose Report a problem. The optional Server studio has separate connection and upload controls with its own status.";
    status.classList.add("status-error");
    document.getElementById("engine-report").hidden = false;
    document.dispatchEvent(new Event("engine-unavailable"));
}

const timeout = setTimeout(startupFailed, STARTUP_TIMEOUT_MS);
try {
    // Static dependencies can fail before app.js reaches its own startup catch.
    // Keep support.js independent so reporting/recovery still work in that case.
    await import("./app.js");
} catch {
    startupFailed();
} finally {
    clearTimeout(timeout);
}
