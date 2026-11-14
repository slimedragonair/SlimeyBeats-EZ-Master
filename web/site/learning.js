import { wireDialogFocus } from "./dialog-focus.js";
import { TOPICS, action, textElement, renderTopic } from "./learning-content.js";
import { revealLearningElement, revealHelpTopics } from "./learning-navigation.js";

const $ = (id) => document.getElementById(id);
const PLACES = { overview: "Mastering · Overview", eq: "Manual EQ", "studio-tools": "Studio" };

async function fetchGuide() {
    const response = await fetch(new URL("./assets/learning-guide.json", import.meta.url), { cache: "no-cache" });
    if (!response.ok) throw new Error("Guidance could not be loaded. Check your connection and try again.");
    const guide = await response.json();
    if (guide.version !== 1 || !guide.welcome?.title || !guide.steps?.length ||
        !Array.isArray(guide.glossary) || !Array.isArray(guide.mcp?.steps) ||
        !guide.steps.every((step) => step.id && step.title && step.body && step.detail && PLACES[step.web_target]))
        throw new Error("This guidance file is not supported. Refresh the app and try again.");
    return guide;
}

export function wireLearning(preferences) {
    const dialog = $("learning-dialog"), modal = wireDialogFocus(dialog);
    const body = $("learning-body"), actions = $("learning-actions"), nav = $("learning-nav");
    let guide, loading, mode = "help", step = 0, openedManually = false;

    preferences.subscribe((source) => {
        for (const id of ["ui-storage-notice", "learning-storage"]) {
            $(id).textContent = preferences.warning;
            $(id).hidden = !preferences.warning;
        }
        if (source === "external" && mode === "welcome" &&
            ["completed", "declined"].includes(preferences.state.tutorial)) dialog.close();
    });
    function frame(title, description, label, showNav = false) {
        $("learning-title").textContent = title;
        $("learning-description").textContent = description;
        $("learning-description").hidden = !description;
        $("learning-label").textContent = label;
        nav.hidden = !showNav;
        body.replaceChildren();
        actions.replaceChildren();
        modal.open();
        $("learning-title").focus({ preventScroll: true });
        $("learning-scroll").scrollTop = 0;
    }
    function dismiss() {
        if (mode === "welcome" || mode === "tour")
            preferences.tutorialChoice("declined", step);
        dialog.close();
    }
    $("learning-close").onclick = dismiss;
    dialog.addEventListener("keydown", (event) => {
        if (event.key === "Escape") {
            event.preventDefault();
            dismiss();
        }
    });
    dialog.addEventListener("cancel", (event) => { event.preventDefault(); dismiss(); });
    dialog.addEventListener("click", (event) => {
        if (event.target !== dialog) return;
        const r = dialog.getBoundingClientRect();
        if (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom) dismiss();
    });

    function tutorial(index = 0) {
        mode = "tour";
        step = Math.max(0, Math.min(index, guide.steps.length - 1));
        preferences.tutorialChoice("started", step);
        const item = guide.steps[step];
        frame(item.title, item.body, `Guided tutorial · ${step + 1} of ${guide.steps.length}`);
        body.append(textElement("p", `Where to find it: ${PLACES[item.web_target]}`, "learning-callout"),
            textElement("p", item.detail), textElement("p", guide.welcome.privacy_note, "learning-privacy"));
        const previous = action("Back", "tutorial-back", () => tutorial(step - 1));
        previous.disabled = step === 0;
        actions.append(previous, action("End tour", "tutorial-dismiss", dismiss));
        if (step < guide.steps.length - 1)
            actions.append(action("Next", "tutorial-next", () => tutorial(step + 1), true));
        else actions.append(action("Finish tutorial", "tutorial-finish", () => {
            preferences.tutorialChoice("completed", step);
            mode = "help";
            dialog.close();
        }, true));
    }
    function welcome() {
        mode = "welcome";
        frame(guide.welcome.title, guide.welcome.body, "Welcome");
        body.append(textElement("p", guide.welcome.privacy_note, "learning-callout"),
            textElement("p", "Start with Basic controls. Find this tour and plain-language help in the bottom-right corner whenever you need them."));
        actions.append(action("No thanks", "tutorial-decline", dismiss),
            action("Start tour", "tutorial-start", () => tutorial(), true));
        $("tutorial-start").setAttribute("aria-label", "Start guided tutorial");
    }
    function help(topic = "quickstart") {
        mode = "help";
        frame(TOPICS.find(([key]) => key === topic)[1], "", "Help", true);
        nav.replaceChildren(...TOPICS.map(([key, label]) => {
            const button = action(label, `help-${key}`, () => help(key));
            button.classList.toggle("selected", key === topic);
            if (key === topic) button.setAttribute("aria-current", "page");
            return button;
        }));
        renderTopic(body, guide, topic, () => tutorial());
        const close = action("Close", "learning-done", dismiss);
        close.setAttribute("aria-label", "Close Help");
        actions.append(action("Topics", "learning-topics", () => revealHelpTopics(nav)), close);
        if (topic === "quickstart" && preferences.state.tutorial === "started")
            body.prepend(action("Continue tutorial", "continue-tutorial", () => tutorial(preferences.state.step)));
        const search = $("glossary-search");
        revealLearningElement(search || $("learning-title"), {
            focus: true, anchor: search ? search.previousElementSibling : $("learning-title"),
        });
    }
    async function load() {
        loading ??= fetchGuide().then((value) => {
            guide = value;
            $("learning-load-notice").hidden = true;
            return value;
        });
        try { return await loading; }
        catch (error) { loading = null; throw error; }
    }
    async function openHelp() {
        openedManually = true;
        mode = "help";
        if (guide) { help(); return; }
        frame("Loading guidance…", "Your audio stays in this tab.", "Help & learning");
        try { await load(); if (dialog.open) help(); }
        catch (error) {
            if (!dialog.open) return;
            frame("Guidance unavailable", error.message, "Help & learning");
            actions.append(action("Try again", "learning-retry", openHelp, true));
        }
    }
    $("open-learning").onclick = openHelp;
    // A new profile gets one optional offer. Started tours can be continued from
    // Help after a reload, without reopening a modal over a returning session.
    load().then(() => {
        if (preferences.available && preferences.state.tutorial === "unseen" && !openedManually &&
            !document.querySelector("dialog[open]")) welcome();
    }).catch(() => {
        $("learning-load-notice").hidden = false;
        $("learning-load-notice").textContent = "Guidance is unavailable right now. Open Help to try loading it again.";
    });
}
