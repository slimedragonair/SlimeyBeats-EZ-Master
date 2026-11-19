// Move the existing selects, not copies: local, server and recovery share their values and handlers.
export function wireMasteringQuickControls() {
    const query = window.matchMedia('(max-width: 680px)');
    const mobile = document.getElementById('mobile-master-settings');
    const fields = ['profile', 'format'].map(name => ({
        field: document.getElementById(`quick-${name}`),
        slot: document.getElementById(`${name}-slot`),
    }));
    let loaded = false;
    let moved = false;

    function layout() {
        const move = loaded && query.matches;
        if (move === moved) return;
        moved = move;
        const focused = document.activeElement;
        const restoreFocus = fields.some(({ field }) => field.contains(focused));
        mobile.hidden = !move;
        for (const { field, slot } of fields) (move ? mobile : slot).append(field);
        if (restoreFocus) {
            focused.focus({ preventScroll: true });
            focused.scrollIntoView({ block: 'nearest', behavior: 'instant' });
        }
    }

    query.addEventListener('change', layout);
    return (hasTrack) => {
        if (loaded === hasTrack) return;
        loaded = hasTrack;
        layout();
    };
}
