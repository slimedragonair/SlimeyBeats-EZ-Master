export const TOPICS = [
    ["quickstart", "Quick start"], ["glossary", "Glossary"],
    ["troubleshooting", "Fix a problem"], ["mcp", "AI / MCP setup"],
    ["recovery", "Local recovery"], ["support", "Report a problem"],
];
// The shared guide also serves the native app. Qualify browser-only privacy at
// rendering time instead of changing the desktop's local-workflow promises.
export const BROWSER_PRIVACY_NOTE = "Local mastering keeps audio on your device by default. Use server processing checks availability without audio; while enabled and visible, checks repeat every 30 seconds. Where guest access is enabled, opting in or choosing Check & connect requests a guest credential kept only in this tab. Operator address and token fields stay in advanced settings. Your current main file can be reused, but audio and its filename are sent only after destination and temporary-storage review, informed consent, and Upload & master on server. Switching off keeps access to existing jobs; Disconnect or leaving clears credentials. Public server availability is not guaranteed. There is no automatic local or server fallback. The tour never connects, uploads, starts processing or saves audio for you.";
export function textElement(tag, text, className) {
    const node = document.createElement(tag);
    node.textContent = text;
    if (className) node.className = className;
    return node;
}
export function action(label, id, run, primary = false) {
    const button = textElement("button", label, primary ? "primary" : "");
    button.id = id;
    button.onclick = run;
    return button;
}

export function renderTopic(host, guide, topic, replay) {
    host.replaceChildren();
    if (topic === "recovery" || topic === "support") {
        const recovery = topic === "recovery";
        host.append(textElement("p", recovery
            ? "Recovery audio storage is off by default. Enable it only if you want audio and settings saved locally on this device, or download a recovery file yourself. Local recovery does not upload audio or save server tokens, file selections, consent or jobs. Optional server processing is a separate workflow."
            : "Preview a small report before deciding whether to share it. Audio and raw logs are never attached automatically."));
        host.append(action(recovery ? "Open local recovery" : "Create a private report", `help-open-${topic}`, () => {
            document.getElementById("learning-dialog").close();
            document.dispatchEvent(new Event(recovery ? "open-recovery" : "open-support-report"));
        }, true));
    } else if (topic === "quickstart") {
        host.append(textElement("p", BROWSER_PRIVACY_NOTE, "learning-privacy"));
        host.append(action("Replay guided tutorial", "replay-tutorial", replay, true));
        const list = document.createElement("ol");
        list.className = "learning-quickstart";
        for (const step of guide.steps.slice(0, -1)) {
            const item = document.createElement("li");
            item.append(textElement("h3", step.title.replace(/^\d+\.\s*/, "")), textElement("p", step.body));
            list.append(item);
        }
        host.append(list);
    } else if (topic === "glossary") {
        const label = textElement("label", "Find a term"), search = document.createElement("input");
        search.type = "search";
        search.id = "glossary-search";
        label.htmlFor = search.id;
        const list = document.createElement("dl"), empty = textElement("p", "No matching terms. Try a different word.");
        list.className = "learning-glossary";
        function filter() {
            const query = search.value.trim().toLocaleLowerCase();
            const terms = guide.glossary.filter((entry) => `${entry.term} ${entry.description}`.toLocaleLowerCase().includes(query));
            list.replaceChildren(...terms.flatMap((entry) => [textElement("dt", entry.term), textElement("dd", entry.description)]));
            empty.hidden = terms.length !== 0;
        }
        search.oninput = () => {
            filter();
            revealLearningElement(search, { anchor: label });
        };
        host.append(label, search, list, empty);
        filter();
    } else if (topic === "mcp") {
        host.append(textElement("p", "Desktop companion feature", "learning-callout"),
            textElement("h3", guide.mcp.title), textElement("p", guide.mcp.body));
        const steps = document.createElement("ol");
        for (const step of guide.mcp.steps) steps.append(textElement("li", step));
        host.append(steps, textElement("p", guide.mcp.privacy_note),
            textElement("p", "The desktop MCP companion uses stdio, launched by your AI client with an executable and arguments. Use the desktop app to generate its connection example and choose permitted folders. This browser app does not connect to that companion. Use server processing selects a separate HTTP mastering workflow. Its availability checks contain no audio; guest access, when enabled, uses a tab-only credential. Operator credentials belong in advanced settings. Only explicit Upload & master on server with informed consent sends audio."));
    } else {
        const issues = [
            ["I cannot hear playback", "Choose Play, check listening Volume and your device output, then compare A / B. B needs a prepared preview. Browsers may require a tap or click before audio can start."],
            ["Export is unavailable", "Master first. If you change the mastering direction, Master again to prepare a current preview. In Studio, render pending fader changes with Update mix preview before further edits or exporting."],
            ["How do I edit one part of a stem?", "In Studio → Stem mix → Time edits, choose Edit this stem and type Selection start / end in seconds, or drag its waveform. Choose Silence, Fade in, Fade out, Reverse or Apply gain explicitly. Update pending mix faders first. Mixer returns to live gain, mute and solo. The range stays the same length; other stems stay aligned. Undo restores the previous revision, and Export is a separate action."],
            ["My file will not load", "Check the format and the displayed import limits. Browser decoder support varies. Try a WAV or the example track. Large files also need sufficient device memory; use the desktop app for larger sessions."],
            ["Server processing is unavailable", "[Server disabled] means the server responded but public processing is disabled; use local processing or a private operator connection in advanced settings. [Server down] means a network failure, timeout or server error. [Setup needed] means the server responded with an incompatible API or connection setup. [Access needed] means access is unavailable or denied, and [Server busy] means retry later. An online health check does not by itself authorize processing: choose Check & connect when prompted. No check uploads audio or resumes a job. Public server availability is not guaranteed."],
            ["The page was reloaded", "By default, browser audio lives only in tab memory and reloading discards it. Help → Local recovery offers optional device storage or a manual recovery download. If you opted in and a completed point remains, choose it explicitly; recovery never starts playback. Keep exported audio as your lasting copy."],
            ["Guidance choices are not remembered", "Private browsing or browser settings may block storage. If choices cannot be saved, automatic guidance stays off and Help still works for this tab. Enable site storage to remember later choices."],
        ];
        for (const [title, body] of issues) {
            const item = document.createElement("details");
            item.append(textElement("summary", title), textElement("p", body));
            host.append(item);
        }
    }
}
import { revealLearningElement } from "./learning-navigation.js";
