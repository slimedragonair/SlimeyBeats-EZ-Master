import { Player } from "./player.js";
import { StreamingPlayer } from "./streaming-player.js";

// Preserve both transports when selecting an engine; changing mode never
// converts a server preview into fake full-length AudioBuffer PCM.
export function createPlayerRouter(onState, onError) {
    const local = new Player(onState, onError), remote = new StreamingPlayer(onState, onError);
    let active = local;
    const api = {
        get isRemote() { return active === remote; },
        get local() { return local; },
        useRemote(value) {
            const next = value ? remote : local;
            if (next !== active) {
                active.pause();
                next.volume = active.volume; next.match = active.match; next.onPlaybackRequest = active.onPlaybackRequest;
                active = next;
            }
            return active;
        },
    };
    return new Proxy(api, {
        get(target, key) {
            if (Reflect.has(target, key)) return Reflect.get(target, key);
            const value = active[key];
            return typeof value === "function" ? value.bind(active) : value;
        },
        set(target, key, value) { if (Reflect.has(target, key)) return Reflect.set(target, key, value); active[key] = value; return true; },
    });
}
