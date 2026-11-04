// Version consistency check (M5-06).
//
// A release is tagged once, but the version lives in four places. This script
// reads all of them and fails if they disagree, so a tag cannot ship a binary
// whose about box reports a different version.

import { readFileSync } from "node:fs";

function readJson(file) {
  return JSON.parse(readFileSync(file, "utf8"));
}

const cargo = readFileSync("Cargo.toml", "utf8");
const workspaceVersion = /\[workspace\.package\][\s\S]*?version = "([^"]+)"/.exec(cargo)?.[1];
const internal = [
  ...cargo.matchAll(/fluxy-(?:proto|core|net|app) = \{ path = "[^"]+", version = "([^"]+)" \}/g),
].map((match) => match[1]);
const packageJson = readJson("package.json").version;
const tauri = readJson("src-tauri/tauri.conf.json").version;

const seen = {
  "Cargo.toml workspace.package": workspaceVersion,
  "package.json": packageJson,
  "src-tauri/tauri.conf.json": tauri,
};
for (const version of internal) {
  seen["Cargo.toml internal dependency"] = version;
}

const values = [...new Set(Object.values(seen))];
if (values.length !== 1 || !values[0]) {
  console.error("version mismatch:");
  for (const [where, version] of Object.entries(seen)) {
    console.error(`  ${where}: ${version}`);
  }
  process.exit(1);
}

console.log(
  `version ${values[0]} consistent across ${Object.keys(seen).length} declarations (${internal.length} internal dependencies)`,
);
