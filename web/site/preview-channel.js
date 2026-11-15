// Use this module's deployed directory: query strings and page flags must never
// select a different store or expose the production recovery database.
export const IS_PREVIEW = new URL("./", import.meta.url).pathname.endsWith("/current-work/");

export function storageKey(name) {
    return IS_PREVIEW ? `current-work:${name}` : name;
}
