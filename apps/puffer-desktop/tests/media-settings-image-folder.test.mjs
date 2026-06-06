import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(
  resolve(root, "../src/lib/screens/agent/MediaSettingsModal.svelte"),
  "utf8"
);

function assertIncludes(fragment, description) {
  if (!source.includes(fragment)) {
    throw new Error(`MediaSettingsModal is missing ${description}`);
  }
}

assertIncludes('import { invoke } from "@tauri-apps/api/core";', "Tauri invoke import");
assertIncludes("const imageDir = $derived(", "derived image directory path");
assertIncludes('sessionCwd.replace(/\\/+$/, "")', "session cwd trailing slash trim");
assertIncludes('"/.puffer/workflows/images"', "image output directory suffix");
assertIncludes("let openError = $state<string | null>(null);", "open error state");
assertIncludes("async function openImageDir()", "open folder handler");
assertIncludes('invoke("open_image_dir", { cwd: sessionCwd })', "open_image_dir invoke");
assertIncludes('{#if kind === "image"}', "image settings branch");
assertIncludes("{#if imageDir}", "non-empty image directory guard");
assertIncludes('readonly value={imageDir}', "read-only image folder input");
assertIncludes('onclick={openImageDir}', "open folder button handler");
assertIncludes("Open folder", "open folder button label");
assertIncludes('{#if openError}', "open error rendering");
assertIncludes(".pf-media-path-row", "path row CSS");

console.log("Verified image media settings folder path UI contract.");
