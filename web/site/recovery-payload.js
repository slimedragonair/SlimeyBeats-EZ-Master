// WebKit can reject IndexedDB Blobs even while accepting ArrayBuffers. Keep
// binary bodies separate from the metadata index so listing points loads no PCM.
import { checkBudget } from "./recovery-format.js";
export async function encodeRecoveryPayload(value) {
    if (value instanceof Blob) return { recovery_stored_bytes: await value.arrayBuffer(), type: value.type };
    if (!value || typeof value !== "object") return value;
    if (Array.isArray(value)) {
        const result = [];
        for (const entry of value) result.push(await encodeRecoveryPayload(entry));
        return result;
    }
    const result = {};
    for (const [key, entry] of Object.entries(value))
        Object.defineProperty(result, key, { value: await encodeRecoveryPayload(entry), enumerable: true, writable: true, configurable: true });
    return result;
}
export function decodeRecoveryPayload(value) {
    if (value instanceof Blob) return value; // Preserve earlier local schema-1 points.
    if (!value || typeof value !== "object") return value;
    if (value.recovery_stored_bytes instanceof ArrayBuffer && Object.keys(value).length === 2 && typeof value.type === "string") {
        checkBudget(value.recovery_stored_bytes.byteLength);
        return new Blob([value.recovery_stored_bytes], { type: value.type.slice(0, 100) });
    }
    if (Array.isArray(value)) return value.map(decodeRecoveryPayload);
    return Object.fromEntries(Object.entries(value).map(([key, entry]) => [key, decodeRecoveryPayload(entry)]));
}
