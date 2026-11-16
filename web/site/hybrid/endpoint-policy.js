export const PUBLIC_API_ORIGIN = "https://api.slimedragon.net";

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
        (endpoint.origin === page.origin || endpoint.origin === PUBLIC_API_ORIGIN);
    if (!allowed && !dev)
        throw new Error(`Use this site's HTTPS origin or ${PUBLIC_API_ORIGIN}. HTTP loopback is available only from a localhost development page.`);
    return endpoint.origin;
}
