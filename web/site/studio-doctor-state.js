// Reports describe one saved target/revision; invalidation also retires late replies.
export class DoctorState {
    constructor() { this.revision = 0; this.sequence = 0; this.clear(); }
    clear() { this.report = null; this.selected = null; this.snapshot = null; this.scanning = false; }
    invalidate() { ++this.revision; ++this.sequence; this.clear(); }
    begin(target, label) {
        this.clear(); this.scanning = true;
        return { target, label, revision: this.revision, sequence: ++this.sequence };
    }
    current(token, target) {
        return token.sequence === this.sequence && token.revision === this.revision && token.target === target;
    }
    accept(token, target, report) {
        if (!this.current(token, target)) return false;
        if (report?.schema_version !== 1 || !Array.isArray(report.issues) ||
            !Number.isFinite(report.duration_seconds) || report.duration_seconds <= 0)
            throw new Error("This issue report is invalid. Scan again with the current engine.");
        this.report = report; this.snapshot = token; this.scanning = false;
        return true;
    }
    get issue() { return this.report?.issues.find(issue => issue.id === this.selected); }
    canRepair({ busy, pendingMix, target }) {
        return !busy && !pendingMix && !this.scanning && !!this.issue?.repair &&
            !!this.snapshot && this.current(this.snapshot, target);
    }
    download() {
        if (!this.report || !this.snapshot) return null;
        return { schema: "slimeybeats-studio-issue-report", version: 1,
            target: this.snapshot.target, target_label: this.snapshot.label,
            saved_revision: this.snapshot.revision, report: this.report,
            scope: "Read-only snapshot of this saved target. No audio, undo history, recovery recipe or live pending mix is included. Heuristics are listening prompts, not a diagnosis." };
    }
}
