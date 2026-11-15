import { IS_PREVIEW } from "./preview-channel.js";

function readIdentity(value) {
    const sha = /^[a-f0-9]{40}$/i;
    const runtime = /^[a-f0-9]{16}$/;
    if (!value || value.branch !== "current-work" || !sha.test(value.commit) ||
        !sha.test(value.main_commit) || typeof value.built_at !== "string" ||
        value.built_at.length > 64 || !Number.isFinite(Date.parse(value.built_at)) ||
        typeof value.runtime_version !== "string" || !runtime.test(value.runtime_version) ||
        typeof value.run_url !== "string" ||
        !/^https:\/\/github\.com\/slimedragonair\/SlimeyBeats-EZ-Master\/actions\/runs\/\d+$/i.test(value.run_url))
        throw new Error("Preview build identity is unavailable");
    const ownVersion = new URL(import.meta.url).searchParams.get("v");
    if (ownVersion !== null && (!runtime.test(ownVersion) || ownVersion !== value.runtime_version))
        throw new Error("Preview runtime does not match");
    return value;
}

async function showPreviewIdentity() {
    const banner = document.querySelector("#preview-banner");
    if (!IS_PREVIEW || !banner) return;
    banner.hidden = false;
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 8000);
    try {
        const response = await fetch(new URL("./preview-build.json", import.meta.url), {
            method: "GET", cache: "no-store", credentials: "omit",
            redirect: "error", signal: controller.signal,
        });
        if (!response.ok) throw new Error("Preview build identity is unavailable");
        const identity = readIdentity(await response.json());
        document.querySelector("#preview-identity").textContent = `Build ${identity.commit.slice(0, 12)}`;
        document.querySelector("#preview-commit").textContent = identity.commit;
        const built = document.querySelector("#preview-built");
        built.dateTime = identity.built_at;
        built.textContent = new Date(identity.built_at).toLocaleString();
        document.querySelector("#preview-main-commit").textContent = identity.main_commit;
        const run = document.querySelector("#preview-run");
        run.href = identity.run_url;
        run.hidden = false;
    } catch (error) {
        document.querySelector("#preview-identity").textContent = error.message === "Preview runtime does not match"
            ? "Build changed; reload to verify identity" : "Build identity unavailable";
    } finally {
        clearTimeout(timeout);
    }
}

showPreviewIdentity();
