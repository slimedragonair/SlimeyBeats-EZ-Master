// Stable IDs match the shared Rust StemKind schema and exported WAV filenames.
export const MAX_STEMS = 9;
export const MULTITRACK_KINDS = [
    "vocals",
    "drums",
    "bass",
    "guitar",
    "piano",
    "synth",
    "strings",
    "percussion",
    "other",
];
export const STEM_KINDS = [
    "vocals",
    "instrumental",
    "drums",
    "bass",
    "other",
    "guitar",
    "piano",
    "synth",
    "strings",
    "percussion",
    ...Array.from({ length: MAX_STEMS }, (_, i) => `part_${i + 1}`),
];
export const kindLabel = (kind) =>
    kind[0].toUpperCase() + kind.slice(1).replace("_", " ");
