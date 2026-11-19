// Targets are real controls. Never click one, change workspace, or start audio on a learner's behalf.
const OVERVIEW = ['#tab-overview', 'Overview', 'Return to Overview to find this control.'];
const whenDisabled = (message) => (element) => element.disabled ? message : '';
const MASTER = ['#mobile-master, #master', 'Master track', whenDisabled('Add a track to enable Master.')];
const TARGETS = {
    add: [['#add', 'Add audio file'],
        ['#track-card', 'Current track', 'On this layout, Add audio file reappears after you choose Remove track.'], OVERVIEW],
    analyze: [['#analyze', 'Analyze', whenDisabled('Add a track to enable Analyze.')], OVERVIEW],
    master: [MASTER, OVERVIEW],
    compare: [['.primary-player .ab', 'A/B comparison'],
        [MASTER[0], 'Master track', 'Add a track and prepare a master to compare A and B.'], OVERVIEW],
    eq: [['#tab-eq', 'Manual EQ']],
    export: [['#mobile-export, #export', 'Export', whenDisabled('Prepare a master to enable Export.')], OVERVIEW],
    studio: [['#tab-studio-tools', 'Studio']],
    help: [['#open-learning', 'Help']],
};

const EDGE = 6;
const CARD_GAP = 14;
const MIN_CARD_ROOM = 138;
const visible = (element) => element.getClientRects().length && getComputedStyle(element).visibility === 'visible';

function candidatesFor(id) {
    for (const [selector, label, hint] of TARGETS[id] ?? []) {
        for (const element of document.querySelectorAll(selector)) {
            if (visible(element)) return { element, label, hint: typeof hint === 'function' ? hint(element) : hint };
        }
    }
    return null;
}

function fixedOnScreen(element) {
    for (let node = element; node && node !== document.body; node = node.parentElement) {
        if (getComputedStyle(node).position === 'fixed') return true;
    }
    return false;
}

export function wireTutorialSpotlight(dialog) {
    const ring = document.createElement('div');
    ring.id = 'tutorial-spotlight';
    ring.setAttribute('aria-hidden', 'true');
    ring.hidden = true;
    document.body.append(ring);
    const pages = document.getElementById('page-navigation');
    let step, note, fallback, pendingFrame = 0, target, openedPages = false, pagePosition;
    const panePositions = new Map();
    const resize = new ResizeObserver(() => queue());

    function hide() {
        ring.hidden = true;
        dialog.classList.remove('tutorial-card', 'tutorial-card-top');
        dialog.style.removeProperty('--tutorial-card-room');
    }
    function pick() {
        // Compact navigation hides the two workspace tabs in Pages.
        let next = candidatesFor(step);
        if ((!next || next.element.closest('#page-navigation')) && !pages.open) {
            const shouldOpen = step === 'eq' || step === 'studio' || !next;
            if (shouldOpen) {
                pages.open = true;
                openedPages = true;
                next = candidatesFor(step);
            }
        }
        return next;
    }
    function render(next, reveal) {
        if (next?.element !== target) {
            if (target) resize.unobserve(target);
            target = next?.element;
            if (target) resize.observe(target);
        }
        const cue = next ? `Look for: ${next.label}.${next.hint ? ` ${next.hint}` : ''}` : fallback;
        if (note.textContent !== cue) note.textContent = cue;
        if (!target) { hide(); return; }
        if (reveal && !fixedOnScreen(target)) {
            // scrollIntoView can move nested desktop panes as well as the document.
            for (let pane = target.parentElement; pane && pane !== document.body; pane = pane.parentElement) {
                if (!panePositions.has(pane) && (pane.scrollHeight > pane.clientHeight || pane.scrollWidth > pane.clientWidth)) {
                    panePositions.set(pane, [pane.scrollLeft, pane.scrollTop]);
                }
            }
            target.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'instant' });
        }
        const r = target.getBoundingClientRect();
        const height = document.documentElement.clientHeight, width = document.documentElement.clientWidth;
        const above = r.top - CARD_GAP, below = height - r.bottom - CARD_GAP;
        const room = Math.max(above, below);
        if (r.top < -1 || r.bottom > height + 1 || r.left < -1 || r.right > width + 1 || room < MIN_CARD_ROOM) {
            hide();
            return;
        }
        const x = Math.max(0, r.left - EDGE), y = Math.max(0, r.top - EDGE);
        const right = Math.min(width, r.right + EDGE), bottom = Math.min(height, r.bottom + EDGE);
        ring.style.left = `${x}px`;
        ring.style.top = `${y}px`;
        ring.style.width = `${right - x}px`;
        ring.style.height = `${bottom - y}px`;
        dialog.style.setProperty('--tutorial-card-room', `${room}px`);
        dialog.classList.toggle('tutorial-card-top', above > below);
        dialog.classList.add('tutorial-card');
        ring.hidden = false;
    }
    function queue(reveal = false) {
        if (!step || pendingFrame) return;
        pendingFrame = requestAnimationFrame(() => {
            pendingFrame = 0;
            if (step && dialog.open) render(reveal ? pick() : candidatesFor(step), reveal);
        });
    }
    const onResize = () => {
        if (!step) return;
        cancelAnimationFrame(pendingFrame);
        pendingFrame = 0;
        queue(true);
    };
    const onScroll = () => queue();

    return {
        show(id, message) {
            pagePosition ??= [scrollX, scrollY];
            step = id;
            note = message;
            fallback = note.textContent;
            hide();
            cancelAnimationFrame(pendingFrame);
            pendingFrame = 0;
            window.addEventListener('resize', onResize);
            window.addEventListener('scroll', onScroll, true);
            // Start after the dialog/title focus has settled, including on touch layouts.
            queue(true);
        },
        clear() {
            if (!step) return;
            step = undefined;
            cancelAnimationFrame(pendingFrame);
            pendingFrame = 0;
            resize.disconnect();
            target = undefined;
            window.removeEventListener('resize', onResize);
            window.removeEventListener('scroll', onScroll, true);
            hide();
            // Wide navigation hides the disclosure summary and relies on Pages staying open.
            if (openedPages && document.body.classList.contains('compact-workspace')) pages.open = false;
            openedPages = false;
            for (const [pane, [left, top]] of panePositions) {
                pane.scrollTo({ left, top, behavior: 'instant' });
            }
            panePositions.clear();
            window.scrollTo({ left: pagePosition[0], top: pagePosition[1], behavior: 'instant' });
            pagePosition = undefined;
        },
    };
}
