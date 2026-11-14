import { StemImporter } from "./studio-importer.js";
import { stemInputs, addStemInput } from "./studio-stem-inputs.js";
import { interleave } from "./dsp-client.js";
const $ = (id) => document.getElementById(`s-${id}`);

export function wireStemImport({
    client,
    playback,
    job,
    mutate,
    isCancelled,
    notice,
}) {
    const importer = new StemImporter();
    stemInputs();
    $("mode").onchange = stemInputs;
    $("add-part").onclick = addStemInput;
    async function decode(entries) {
        if (entries.length < 2)
            throw new Error("Choose at least two aligned stem files.");
        if (new Set(entries.map((part) => part.kind)).size !== entries.length)
            throw new Error(
                "Each stem type must be unique. Use separate numbered parts for additional takes of the same instrument.",
            );
        const stems = [];
        let bytes = 0;
        for (const entry of entries) {
            if (isCancelled()) return;
            const file = await entry.read();
            if (isCancelled()) return;
            const buffer = await playback.decode(file);
            if (isCancelled()) return;
            bytes += buffer.length * buffer.numberOfChannels * 4;
            if (bytes > 128 * 1024 * 1024)
                throw new Error(
                    "Combined stems exceed 128 MiB decoded PCM. Try shorter stems or use desktop Studio.",
                );
            stems.push({
                kind: entry.kind,
                display_name:
                    entry.display_name ??
                    file.name
                        .split(/[\\/]/)
                        .at(-1)
                        .replace(/\.[^.]+$/, ""),
                samples: interleave(buffer),
                rate: buffer.sampleRate,
                channels: buffer.numberOfChannels,
            });
        }
        return mutate(
            "stems",
            { stems },
            stems.map((stem) => stem.samples.buffer),
            true,
            "Imported stems",
        );
    }
    const openFiles = (files) =>
        job("Grouping stem files locally…", async () => {
            if (!files.length) return;
            if (files.some((file) => file.name.toLowerCase().endsWith(".zip")))
                throw new Error(
                    "Choose one stem ZIP at a time, or select only loose audio stems.",
                );
            const plan = await client.call("import-plan", {
                files: files.map((file) => ({
                    name: file.name,
                    size: file.size,
                })),
            });
            if (!isCancelled()) {
                await client.call("archive-close");
                if (isCancelled()) return;
                importer.open(plan, files);
                notice(
                    `Review ${plan.parts.length} parts, then import when ready`,
                );
            }
        });
    const openZip = (file) =>
        job("Reading stem ZIP contents locally…", async () => {
            if (!file) return;
            if (file.size > 128 * 1024 * 1024)
                throw new Error(
                    "Browser stem ZIP exceeds 128 MiB. Use desktop Studio for larger archives.",
                );
            const bytes = new Uint8Array(await file.arrayBuffer());
            if (isCancelled()) return;
            const result = await client.call("archive-open", { bytes }, [
                bytes.buffer,
            ]);
            if (!isCancelled()) {
                importer.open(result.plan, null, result.archive_id);
                notice(
                    `Review ${result.plan.parts.length} parts, then import when ready`,
                );
            }
        });
    $("bulk-choose").onclick = () => $("bulk-files").click();
    $("zip-choose").onclick = () => $("zip-file").click();
    $("bulk-files").onchange = async () => {
        await openFiles([...$("bulk-files").files]);
        $("bulk-files").value = "";
    };
    $("zip-file").onchange = async () => {
        await openZip($("zip-file").files[0]);
        $("zip-file").value = "";
    };
    $("bulk-format").onchange = () => importer.render();
    $("bulk-discard").onclick = () =>
        job("Clearing the pending file selection…", async () => {
            importer.clear();
            await client.call("archive-close");
            if (!isCancelled())
                notice("Selection cleared. Studio audio is unchanged.");
        });
    $("import-batch").onclick = () =>
        job("Importing selected stems locally…", async () => {
            const parts = importer.selection();
            const accepted = await decode(
                parts.map((part) => ({
                    kind: part.kind,
                    display_name: part.display_name,
                    read: () => importer.file(part, client),
                })),
            );
            if (accepted) {
                importer.clear();
                await client.call("archive-close");
            }
        });
    $("stem-drop").ondragover = (event) => {
        event.preventDefault();
        event.stopPropagation();
    };
    $("stem-drop").ondrop = async (event) => {
        event.preventDefault();
        event.stopPropagation();
        const files = [...event.dataTransfer.files];
        if (files.length === 1 && files[0].name.toLowerCase().endsWith(".zip"))
            await openZip(files[0]);
        else await openFiles(files);
    };
    $("import").onclick = () =>
        job("Decoding aligned stems locally…", async () => {
            const inputs = [...$("stem-files").querySelectorAll("input")];
            const chosen = inputs.filter((input) => input.files[0]);
            if ($("mode").value !== "multi" && chosen.length !== inputs.length)
                throw new Error(
                    "Choose every file for this preset, or select Custom stem set and leave unused slots empty.",
                );
            await decode(
                chosen.map((input) => ({
                    kind: input.dataset.kind,
                    read: async () => input.files[0],
                })),
            );
        });
    return importer;
}
