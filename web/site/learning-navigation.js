// Scroll only the guidance pane; topic changes never move the audio workspace.
const FOCUS_INSET = 8;
export function revealLearningElement(element, { focus = false, anchor = element } = {}) {
    if (focus) element.focus({ preventScroll: true });
    const content = element.closest(".learning-content");
    content.scrollTop += anchor.getBoundingClientRect().top - content.getBoundingClientRect().top - FOCUS_INSET;
}

export function revealHelpTopics(nav) {
    revealLearningElement(nav.querySelector("button"), { focus: true });
}
