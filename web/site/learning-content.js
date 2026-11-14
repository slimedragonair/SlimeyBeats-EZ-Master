export const TOPICS = [
    ["quickstart", "Quick start"], ["glossary", "Glossary"],
    ["troubleshooting", "Fix a problem"], ["mcp", "AI / MCP setup"],
];
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
    if (topic === "quickstart") {
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
            textElement("p", "Use the desktop app to generate its connection example and choose permitted folders. This browser app has no upload or localhost connection to the desktop companion."));
    } else {
        const issues = [
            ["I cannot hear playback", "Choose Play, check listening Volume and your device output, then compare A / B. B needs a prepared preview. Browsers may require a tap or click before audio can start."],
            ["Export is unavailable", "Master first. If you change the mastering direction, Master again to prepare a current preview. In Studio, render pending fader changes with Update mix preview before further edits or exporting."],
            ["My file will not load", "Check the format and the displayed import limits. Browser decoder support varies. Try a WAV or the example track. Large files also need sufficient device memory; use the desktop app for larger sessions."],
            ["The page was reloaded", "Browser audio sessions live in this tab's memory. Reloading discards the session; add the original again. Export a version you want to keep before leaving. Only small interface choices are remembered."],
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
