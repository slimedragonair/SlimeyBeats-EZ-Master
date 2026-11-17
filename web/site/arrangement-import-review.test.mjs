import test from "node:test";
import assert from "node:assert/strict";
import { groupFileVersions } from "./arrangement-import-review.js";

const file = (name, webkitRelativePath = "", size = 10) => ({ name, webkitRelativePath, size });
test("one source, repeated labels and alternate encodings retain owned independent IDs", () => {
    const first = file("Lead.wav"), repeated = file("Lead.wav"), alternate = file("Lead.mp3");
    let next = 0;
    const value = groupFileVersions([first, repeated, alternate], { newId: () => String(++next) });
    assert.equal(value.parts.length, 2);
    assert.deepEqual(value.parts.map(part => part.name), ["Lead", "Lead"]);
    assert.equal(value.parts[0].versions.length, 2);
    assert.equal(value.entries.get(value.parts[1].versions[0].id), repeated);
    assert.equal(groupFileVersions([first]).parts.length, 1);
});
test("WAV/MP3 encoding folders pair within their parent source only", () => {
    const value = groupFileVersions([file("A.wav", "one/WAV/A.wav"), file("A.mp3", "one/MP3/A.mp3"), file("A.wav", "two/WAV/A.wav")]);
    assert.equal(value.parts.length, 2);
    assert.deepEqual(value.parts.map(part => part.versions.length), [2, 1]);
});
test("engine file budgets and metadata-only large reviews are explicit", () => {
    assert.throws(() => groupFileVersions([file("A.wav", "", 20)], { max_file_bytes: 10 }));
    assert.equal(groupFileVersions([file("A.wav", "", 20)], { max_file_bytes: 30 }).parts.length, 1);
    assert.equal(groupFileVersions(Array.from({ length: 1000 }, (_, i) => file(`${i}.wav`))).parts.length, 1000);
    const reviewed = groupFileVersions([file("A.wav"), file("notes.txt")]);
    assert.equal(reviewed.skipped_files, 1);
});
