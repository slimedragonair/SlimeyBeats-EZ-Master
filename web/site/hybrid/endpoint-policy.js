export const LEGACY_API_ORIGIN = "https://api.slimedragon.net";
// Deployment pin: replace this assignment with the exact authenticated HTTPS
// origin, and add that same origin to index.html's connect-src policy.
export const PUBLIC_API_ORIGIN = "https://slimeybeats-preview.tailf8de2d.ts.net";

const TRUSTED_API_ORIGINS = Object.freeze([...new Set([PUBLIC_API_ORIGIN, LEGACY_API_ORIGIN])]);

export function serverEndpointHelp() {
    return `Use this site's HTTPS origin or ${TRUSTED_API_ORIGINS.join(" or ")}. ` +
        "Only these exact origins are trusted. HTTP loopback is available only from a localhost development page.";
}

function loopback(host) {
    return host === "localhost" || host === "127.0.0.1" || host === "[::1]";
}

export function serverEndpoint(value, pageUrl) {
    if (!value.trim()) throw new Error("Enter a server address before connecting.");
    let endpoint;
    try { endpoint = new URL(value.trim()); }
    catch { throw new Error("Enter a complete HTTPS server address."); }
    const page = new URL(pageUrl);
    if (endpoint.username || endpoint.password || endpoint.search || endpoint.hash || endpoint.pathname !== "/")
        throw new Error("Use only the server origin, without credentials, a path, query or fragment.");
    const dev = page.protocol === "http:" && loopback(page.hostname) &&
        endpoint.protocol === "http:" && loopback(endpoint.hostname);
    const allowed = endpoint.protocol === "https:" &&
        (endpoint.origin === page.origin || TRUSTED_API_ORIGINS.includes(endpoint.origin));
    if (!allowed && !dev)
        throw new Error(serverEndpointHelp());
    return endpoint.origin;
}
