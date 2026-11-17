// Drafts belong to stable entity IDs. A late metadata refresh must not overwrite
// unfinished text or silently rebase a timing edit onto changed source bounds.
export class ArrangementDrafts {
    constructor() { this.entries = new Map(); }
    set(key, record, field, value, watched = null) {
        if (!key) return;
        let entry = this.entries.get(key);
        if (!entry) {
            entry = { base: JSON.stringify(record), values: new Map(), watched: null };
            this.entries.set(key, entry);
        }
        entry.values.set(field, value);
        if (watched) {
            entry.watched ??= new Map();
            for (const property of watched) if (!entry.watched.has(property))
                entry.watched.set(property, JSON.stringify(record[property]));
        }
    }
    value(key, field, committed) { return this.entries.get(key)?.values.get(field) ?? committed; }
    changed(key) { return !!this.entries.get(key)?.values.size; }
    conflicts(key, record) {
        const entry = this.entries.get(key);
        if (!entry) return false;
        if (entry.watched) return [...entry.watched].some(([property, before]) => JSON.stringify(record?.[property]) !== before);
        return entry.base !== JSON.stringify(record);
    }
    requireCurrent(key, record) {
        if (this.conflicts(key, record))
            throw new Error("The selected item changed while you were editing. Your draft is kept; reload its saved values before applying it.");
    }
    clear(key) { this.entries.delete(key); }
    reset() { this.entries.clear(); }
}
