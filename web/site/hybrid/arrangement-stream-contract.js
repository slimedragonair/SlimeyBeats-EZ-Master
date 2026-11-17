const ID = /^[A-Za-z0-9_-]{1,128}$/;
const frame = value => Number.isSafeInteger(value) && value >= 0;

export function checkedArrangementStream(value, snapshot, side) {
    const project = snapshot.arrangement;
    if (!value || value.epoch !== snapshot.epoch || value.prepared_revision !== snapshot.revision ||
        value.identity !== snapshot.preview.identity || value.sample_rate !== project.sample_rate ||
        value.channels !== project.channels || value.duration_frames !== snapshot.preview.frames ||
        !frame(value.side_duration_frames) || !frame(value.start_frame) || value.start_frame > value.side_duration_frames ||
        side === "after" && value.side_duration_frames !== value.duration_frames ||
        !Number.isSafeInteger(value.preview_sample_rate) || value.preview_sample_rate < 8000 || value.preview_sample_rate > 384000 ||
        !frame(value.preview_priming_frames) || !["before", "after"].includes(side) ||
        !(ID.test(value.stream_id || "") || !value.side_duration_frames && value.stream_id == null))
        throw new Error("The server stream does not match the requested arrangement revision or frame layout.");
    return { ...value, side };
}

export function arrangementStreamFrame(descriptor, seconds) {
    if (!Number.isFinite(seconds) || seconds < 0) return descriptor.start_frame;
    const audible = Math.max(0, seconds - descriptor.preview_priming_frames / descriptor.preview_sample_rate);
    return Math.min(descriptor.side_duration_frames, descriptor.start_frame + Math.round(audible * descriptor.sample_rate));
}
