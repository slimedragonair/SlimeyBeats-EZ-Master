// Only interface choices belong here. Audio and processing settings stay in memory.
export const UI_PREFERENCES_KEY = "slimeybeats.interface-preferences";
export const UI_TERMINAL_KEY = `${UI_PREFERENCES_KEY}.tutorial-dismissed`;
const STATUSES = new Set(["unseen", "started", "completed", "declined"]);
const STATUS_ORDER = { unseen: 0, started: 1, declined: 2, completed: 3 };
const strongestStatus = (...values) => values.reduce((saved, next) =>
    STATUS_ORDER[next] > STATUS_ORDER[saved] ? next : saved, "unseen");
const terminal = (value) => value === "completed" || value === "declined";

function invalidData() {
    const error = new Error("Invalid saved interface choices");
    error.name = "InvalidUiPreferences";
    throw error;
}
function storedProfile() {
    const raw = localStorage.getItem(UI_PREFERENCES_KEY);
    if (raw === null) return null;
    try { return readChoices(JSON.parse(raw)); }
    catch { invalidData(); }
}
function storedTerminal() {
    const value = localStorage.getItem(UI_TERMINAL_KEY);
    if (value !== null && !terminal(value)) invalidData();
    return value || "unseen";
}

function readChoices(value) {
    if (!value || typeof value !== "object" || Array.isArray(value))
        throw new Error("Invalid interface preferences");
    if (("advanced" in value && typeof value.advanced !== "boolean") ||
        ("step" in value && (!Number.isInteger(value.step) || value.step < 0)))
        throw new Error("Invalid interface choice");
    // Earlier interface profiles may not have learning fields. Keep any terminal
    // choice across schema versions; a content update must not restart a tour.
    const tutorial = value.tutorial ?? "unseen";
    if (!STATUSES.has(tutorial)) throw new Error("Unknown tutorial status");
    return {
        version: 1,
        advanced: value.advanced === true,
        tutorial,
        step: Number.isInteger(value.step) && value.step >= 0 ? value.step : 0,
    };
}

export function createUiPreferences() {
    let state = readChoices({}), available = true, warning = "";
    const listeners = new Set();
    function unavailable(corrupt = false) {
        available = false;
        warning = corrupt
            ? "Saved interface choices could not be read. Open Help for a tour. Choices last only for this tab; automatic guidance is off."
            : "This browser cannot remember interface choices. Open Help for a tour. Choices last only for this tab; automatic guidance is off.";
    }
    try {
        state = storedProfile() || state;
        const dismissed = storedTerminal();
        state.tutorial = strongestStatus(state.tutorial, dismissed);
        // Migrate old completed/declined profiles. The marker is written only
        // with a terminal choice, never with unseen or started.
        if (terminal(state.tutorial) && STATUS_ORDER[state.tutorial] > STATUS_ORDER[dismissed])
            localStorage.setItem(UI_TERMINAL_KEY, state.tutorial);
        // Probe a separate temporary key: writing a cached profile here could
        // overwrite a terminal choice just saved by another window.
        if (available) {
            const probe = `${UI_PREFERENCES_KEY}.probe`;
            localStorage.setItem(probe, "1");
            localStorage.removeItem(probe);
        }
    } catch (error) { unavailable(error.name === "InvalidUiPreferences"); }

    function update(choice) {
        let latest = state, dismissed = "unseen";
        if (available) {
            try {
                latest = storedProfile() || state;
                dismissed = storedTerminal();
            } catch (error) { unavailable(error.name === "InvalidUiPreferences"); }
        }
        const tutorial = strongestStatus(state.tutorial, latest.tutorial, dismissed, choice.tutorial || "unseen");
        const savedStep = STATUS_ORDER[state.tutorial] > STATUS_ORDER[latest.tutorial] ? state.step : latest.step;
        const record = { ...latest, ...choice, tutorial, step: choice.step ?? savedStep, version: 1 };
        // A tutorial save preserves another tab's Advanced choice. The current
        // window keeps its own visibility until the user changes it or reloads.
        state = { ...state, ...choice, tutorial,
            step: choice.step ?? (STATUS_ORDER[latest.tutorial] > STATUS_ORDER[state.tutorial] ? latest.step : state.step) };
        if (available) {
            // A different key makes suppression survive an interleaving writer
            // between the profile getItem and setItem. Visibility writes never
            // erase this marker. Try each save once; a failed marker can still
            // fall back to the terminal profile, with a visible warning.
            try {
                if (terminal(tutorial) && STATUS_ORDER[tutorial] > STATUS_ORDER[dismissed])
                    localStorage.setItem(UI_TERMINAL_KEY, tutorial);
            } catch { unavailable(); }
            try { localStorage.setItem(UI_PREFERENCES_KEY, JSON.stringify(record)); }
            catch { unavailable(); }
            try { state.tutorial = strongestStatus(state.tutorial, storedTerminal()); }
            catch (error) { unavailable(error.name === "InvalidUiPreferences"); }
        }
        for (const listener of listeners) listener("local");
    }
    window.addEventListener("storage", (event) => {
        if (![UI_PREFERENCES_KEY, UI_TERMINAL_KEY].includes(event.key) || !available) return;
        try {
            const latest = storedProfile() || state, dismissed = storedTerminal();
            const step = STATUS_ORDER[latest.tutorial] > STATUS_ORDER[state.tutorial] ? latest.step : state.step;
            state = { ...state, tutorial: strongestStatus(state.tutorial, latest.tutorial, dismissed), step };
        } catch (error) { unavailable(error.name === "InvalidUiPreferences"); }
        for (const listener of listeners) listener("external");
    });
    return {
        get state() { return { ...state }; },
        get available() { return available; },
        get warning() { return warning; },
        setAdvanced(advanced) { update({ advanced }); },
        tutorialChoice(tutorial, step = state.step) {
            update({ tutorial, step });
        },
        subscribe(listener) { listeners.add(listener); listener(); },
    };
}
