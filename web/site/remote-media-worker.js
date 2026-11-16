import { parseMediaUrl, validByteRange } from "./hybrid/remote-media-policy.js";
import { serverEndpoint } from "./hybrid/endpoint-policy.js";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", event => event.waitUntil(self.clients.claim()));

// Native download fetches may omit clientId. Keep only a short-lived route to
// the initiating editing tab; credentials still come from that tab per request.
const downloads = new Map(), DOWNLOAD_ROUTE_MS = 120000, MAX_DOWNLOAD_ROUTES = 256;
function pruneDownloads() {
    for (const [path, route] of downloads) if (route.expires <= Date.now()) downloads.delete(path);
}
self.addEventListener("message", event => {
    if (event.data?.type !== "slimeybeats-download-route" || !event.ports?.[0]) return;
    event.waitUntil((async () => {
        const port = event.ports[0], source = event.source?.id && await self.clients.get(event.source.id);
        const media = parseMediaUrl(event.data.url, self.registration.scope);
        const scope = new URL(self.registration.scope);
        const page = source && new URL(source.url);
        pruneDownloads();
        const allowed = source?.type === "window" && page.origin === scope.origin &&
            page.pathname.startsWith(scope.pathname) && media?.kind === "artifact" &&
            (downloads.has(media.path) || downloads.size < MAX_DOWNLOAD_ROUTES);
        if (allowed) downloads.set(media.path, { clientId: source.id, expires: Date.now() + DOWNLOAD_ROUTE_MS });
        port.postMessage({ ready: !!allowed }); port.close();
    })());
});

async function credentials(clientId, path) {
    const client = clientId && await self.clients.get(clientId);
    if (!client) throw new Error("No active editing tab.");
    const channel = new MessageChannel();
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => { channel.port1.close(); reject(new Error("Editing tab unavailable.")); }, 5000);
        channel.port1.onmessage = ({ data }) => {
            clearTimeout(timer); channel.port1.close();
            if (data?.path !== path || typeof data.token !== "string" ||
                !data.token || data.token.length > 4096 || /[\s\x00-\x1f\x7f]/.test(data.token))
                reject(new Error("Server access unavailable."));
            else resolve(data);
        };
        client.postMessage({ type: "slimeybeats-media-access", version: 2, path }, [channel.port2]);
    });
}

async function stream(event, media) {
    try {
        if (event.request.method !== "GET" || !validByteRange(event.request.headers.get("Range")))
            return new Response("Unsupported media request.", { status: 400 });
        pruneDownloads();
        const clientId = event.clientId || (media.kind === "artifact" && downloads.get(media.path)?.clientId);
        const access = await credentials(clientId, media.path);
        const endpoint = serverEndpoint(access.endpoint, self.registration.scope);
        const headers = { Authorization: `Bearer ${access.token}` };
        const range = event.request.headers.get("Range");
        if (range) headers.Range = range;
        const response = await fetch(endpoint + media.path, { headers, signal: event.request.signal,
            cache: "no-store", credentials: "omit", redirect: "error", referrerPolicy: "no-referrer" });
        if (![200, 206, 416].includes(response.status)) {
            response.body?.cancel().catch(() => {});
            return new Response("Server media unavailable. Return to the editor to reconnect.", { status: response.status });
        }
        const safe = new Headers({ "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff" });
        for (const name of ["Content-Type", "Content-Length", "Content-Range", "Accept-Ranges", "Content-Disposition"])
            if (response.headers.has(name)) safe.set(name, response.headers.get(name));
        // Pass through the stream; never arrayBuffer(), Blob(), or cache audio.
        return new Response(response.body, { status: response.status, headers: safe });
    } catch {
        return new Response("Server preview unavailable. Check the connection in the editor.", { status: 503 });
    }
}

self.addEventListener("fetch", event => {
    const media = parseMediaUrl(event.request.url, self.registration.scope);
    if (media) event.respondWith(stream(event, media));
});
