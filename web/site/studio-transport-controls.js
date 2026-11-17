// Both Studio workspaces route these existing controls to their own real
// transport. There is no audio conversion or source replacement in this layer.
export function wireStudioTransportControls({ transport, beforePlay, notice }) {
    const $ = name => document.getElementById(`s-${name}`);
    async function invoke(action) {
        try { const source = transport(); if (source) await action(source); }
        catch (error) { notice(error.message || String(error), true); }
    }
    $("play").onclick = () => invoke(async source => {
        beforePlay();
        if (source.playing) source.pause(); else await source.play();
    });
    $("stop").onclick = () => invoke(source => source.stop());
    for (const side of ["before", "after"]) $(side).onclick = () => invoke(source => source.choose(side));
    $("volume").oninput = () => invoke(source => source.setVolume(Number($("volume").value)));
    $("seek").oninput = () => invoke(source => source.seek(Number($("seek").value)));
}
