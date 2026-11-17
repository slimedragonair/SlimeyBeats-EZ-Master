// Local-only, bounded diagnostic event trace. No upload endpoint, persistent
// browser storage, audio, request bodies, tokens, URLs or file/project labels.
import { fileTraceMetadata } from "./session-trace-file.js";
import { TRACE_CONTROLS, TRACE_MESSAGES } from "./session-trace-catalog.js";
const OPERATIONS = new Set(['read', 'edit', 'undo', 'redo', 'importFiles', 'importAudio', 'saveProject', 'openProject', 'previewSource', 'exportAudio', 'analyse', 'analyze', 'suggestEq', 'applyEq', 'clear', 'initialise', 'split', 'separate', 'mix', 'load-file', 'import-files', 'doctor-scan', 'spectrogram', 'export', 'master', 'preview', 'analyze', 'render', 'commit', 'discard', 'import-plan', 'archive-open', 'archive-read', 'archive-close', 'suggest-eq', 'apply-eq']);
const CODES = new Set(['arrangement_capacity', 'guest_job_limit', 'guest_job_ip_limit', 'public_job_limit', 'guest_admission_limit', 'guest_session_limit', 'project_capacity', 'AbortError']);
export function safeMessage(text) {
    if (typeof text !== 'string') return { message_code: 'redacted', message: '[Unrecognized message text omitted]' };
    if (TRACE_MESSAGES.has(text)) return { message_code: 'app_message', message: text };
    if (/^(Server|Local) arrangement unavailable:/.test(text)) {
        const status = text.match(/HTTP (\d{3})/);
        return { message_code: 'arrangement_unavailable', message: 'Arrangement unavailable. Current local work is preserved.', ...(status ? { http_status: Number(status[1]) } : {}) };
    }
    const http = /^Server request failed \(HTTP (\d{3})\)/.exec(text);
    if (http) return { message_code: 'server_request_failed', message: 'Server request failed.', http_status: Number(http[1]) };
    return { message_code: 'redacted', message: '[Unrecognized message text omitted]' };
}
export function createSessionTrace({ maxEvents = 1000, now = () => Date.now(), sessionId } = {}) {
    // sessionId is injectable only for deterministic tests. Never use an auth/session token.
    const id = sessionId || globalThis.crypto?.randomUUID?.() || `diagnostic-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const capacity = Math.max(10, Math.min(1000, Number(maxEvents) || 1000));
    let events = [], sequence = 0, dropped = 0, lastAction = null, actionTime = 0;
    const add = (kind, fields = {}) => {
        const event = { session: id, timestamp: new Date(now()).toISOString(), message: kind, extra: { schema_version: 1, sequence: ++sequence, ...fields }, level: fields.severity === 'error' || fields.result === 'failure' ? 'error' : 'info' };
        events.push(event);
        if (events.length > capacity) { events.shift(); dropped++; }
        return event;
    };
    const action = (control, interaction = 'click') => {
        const e = add('ui_action', { control: TRACE_CONTROLS.has(control) ? control : 'unlabelled_control', interaction: interaction === 'change' ? 'change' : 'click' });
        lastAction = e.extra.sequence; actionTime = now(); return e;
    };
    const begin = (operation, mode = 'local') => {
        const safeOperation = OPERATIONS.has(operation) || ['server_request', 'workspace_open'].includes(operation) ? operation : 'other_operation';
        const e = add('process_started', { operation: safeOperation, mode: mode === 'server' ? 'server' : 'local', action_sequence: now() - actionTime < 30000 ? lastAction : null });
        let done = false;
        return (result = 'success', details = {}) => {
            if (done) return; done = true;
            add('process_finished', { process_sequence: e.extra.sequence, result: ['success', 'failure', 'cancelled'].includes(result) ? result : 'failure', duration_ms: Math.max(0, now() - Date.parse(e.timestamp)),
                ...(Number.isInteger(details.status) && details.status >= 100 && details.status <= 599 ? { http_status: details.status } : {}),
                ...(CODES.has(details.code) ? { error_code: details.code } : {}) });
        };
    };
    let pendingFiles = 0, clearGeneration = 0;
    let fileMetadata = new WeakMap();
    return { action, begin,
        file: async (file, uploaded = false) => {
            const event = uploaded === true ? "uploaded file" : "selected file";
            const generation = clearGeneration;
            if (pendingFiles >= 2) return add(event, { size: file.size, format: 'unknown', file_hash: null, hash_status: 'skipped_busy' });
            pendingFiles++;
            try {
                if (!fileMetadata.has(file)) fileMetadata.set(file, fileTraceMetadata(file, { cancelled: () => generation !== clearGeneration }));
                const meta = await fileMetadata.get(file); if (generation === clearGeneration) add(event, meta); }
            catch { if (generation === clearGeneration) add(event, { size: file.size, format: 'unknown', file_hash: null, hash_status: 'read_failed' }); }
            finally { pendingFiles--; }
        },
        message: (text, isError = false) => add('message_displayed', { severity: isError ? 'error' : 'info', ...safeMessage(text) }),
        exportJsonl: () => [JSON.stringify({ session: id, timestamp: new Date(now()).toISOString(), message: 'trace_metadata', extra: { schema_version: 1, retention: 'memory_only', max_events: capacity, dropped_events: dropped, message_policy: 'known_app_text_only; unknown messages omitted' }, level: 'info' }), ...events.map(e => JSON.stringify(e))].join('\n') + '\n',
        clear: () => { events = []; dropped = 0; lastAction = null; clearGeneration++; fileMetadata = new WeakMap(); },
        snapshot: () => structuredClone(events),
    };
}
export const sessionTrace = createSessionTrace();
export function traceAdapter(adapter, mode, trace = sessionTrace) {
    const wrappers = new Map();
    return new Proxy(adapter, { get(target, key) {
        const value = Reflect.get(target, key, target);
        if (typeof value !== 'function') return value;
        if (!wrappers.has(key)) wrappers.set(key, function (...args) {
            const end = trace.begin(key === "call" && typeof args[0] === "string" ? args[0] : key, typeof mode === "function" ? mode() : mode);
            try {
                const result = value.apply(target, args);
                if (result && typeof result.then === 'function') return result.then(value => { end('success'); return value; }, error => { end(error?.name === 'AbortError' ? 'cancelled' : 'failure', { status: error?.status, code: error?.code }); throw error; });
                end('success'); return result;
            } catch (error) { end('failure', { status: error?.status, code: error?.code }); throw error; }
        });
        return wrappers.get(key);
    } });
}
// Wrap only the explicit server client transport, never global fetch. Request
// URL/body/headers and response text are deliberately never inspected.
export function traceFetch(fetchImpl, trace = sessionTrace) {
    return async (...args) => {
        const end = trace.begin('server_request', 'server');
        try { const response = await fetchImpl(...args); end(response.ok ? 'success' : 'failure', { status: response.status }); return response; }
        catch (error) { end(error?.name === 'AbortError' ? 'cancelled' : 'failure'); throw error; }
    };
}
export function wireSessionTrace(doc, trace = sessionTrace) {
    const listener = event => {
        const element = event.target?.closest?.('button, input, select, [role="button"], a');
        if (!element) return;
        if (event.type === 'change' && element.type === 'file') { for (const file of [...(element.files || [])].slice(0,32)) void trace.file(file); }
        trace.action(element.getAttribute('data-arr') || element.getAttribute('data-action') || element.id, event.type);
    };
    doc.addEventListener('click', listener, true); doc.addEventListener('change', listener, true);
    return () => { doc.removeEventListener('click', listener, true); doc.removeEventListener('change', listener, true); };
}
