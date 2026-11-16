import { StudioPlayback } from "../studio-player.js";
import { RemoteStudioPlayback } from "./remote-studio-playback.js";

export function createStudioPlaybackRouter(onChange, beforePlay) {
    const local = new StudioPlayback(onChange, beforePlay), remote = new RemoteStudioPlayback(onChange, beforePlay);
    let active = local;
    const api = { local, remote, get isRemote() { return active === remote; },
        useRemote(value) { const next = value ? remote : local; if (next !== active) { active.pause(); next.volume = active.volume; active = next; } } };
    return new Proxy(api, {
        get(target, key) { if (Reflect.has(target, key)) return Reflect.get(target, key); const value = active[key]; return typeof value === "function" ? value.bind(active) : value; },
        set(target, key, value) {
            if (key === "onChange") { local.onChange = remote.onChange = value; return true; }
            active[key] = value; return true;
        },
    });
}
