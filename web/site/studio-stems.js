// Stable IDs match the shared Rust StemKind schema and exported WAV filenames.
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
];
export function nextPartKind(kinds) {
    const used = new Set(kinds);
    let index = 1;
    while (used.has(`part_${index}`)) index++;
    return `part_${index}`;
}
export function kindOptions(kinds) {
    const current = [...kinds];
    return [
        ...new Set([
            ...STEM_KINDS,
            ...Array.from(
                { length: current.length + 1 },
                (_, i) => `part_${i + 1}`,
            ),
            ...current,
        ]),
    ];
}
export const kindLabel = (kind) =>
    kind[0].toUpperCase() + kind.slice(1).replace("_", " ");
