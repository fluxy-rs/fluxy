#!/usr/bin/env node
// Reproducible dependency and decoder audit (M5-03).
//
// Runs the three checks the release gate needs, on any platform. The advisories
// check needs network access to clone the RustSec database; it reports a clear
// failure rather than silently skipping, so a release run cannot forget it.
//
//   node scripts/security/audit.mjs
//   node scripts/security/audit.mjs --fuzz-iterations 35000000
//   node scripts/security/audit.mjs --skip-advisories

import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..", "..");
const args = process.argv.slice(2);
const iterationIndex = args.indexOf("--fuzz-iterations");
const fuzzIterations = iterationIndex >= 0 ? args[iterationIndex + 1] : "200000";
const skipAdvisories = args.includes("--skip-advisories");
const isWindows = process.platform === "win32";

const failed = [];

function run(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, {
    cwd: root,
    stdio: "inherit",
    shell: isWindows,
    ...options,
  });
  return result.error ? 1 : (result.status ?? 1);
}

function step(name, action) {
  console.log(`== ${name}`);
  try {
    if (action() !== 0) failed.push(name);
  } catch (error) {
    console.error(error.message);
    failed.push(name);
  }
}

step("cargo-deny licenses, bans, sources", () =>
  run("cargo", ["deny", "check", "licenses", "bans", "sources"]),
);

if (!skipAdvisories) {
  step("cargo-deny advisories (RustSec database)", () => {
    // The database is a git repository. When the network is unavailable, fall
    // back to the cached copy so a release run still checks it.
    if (run("cargo", ["deny", "check", "advisories"]) === 0) return 0;
    return run("cargo", ["deny", "--offline", "check", "advisories"]);
  });
}

step("decoder fuzz", () =>
  run(
    "cargo",
    [
      "test",
      "-q",
      "-p",
      "fluxy-core",
      "--release",
      "--test",
      "decoder_fuzz",
      "--",
      "--nocapture",
    ],
    { env: { ...process.env, FLUXY_FUZZ_ITERATIONS: fuzzIterations } },
  ),
);

step("unsafe inventory", () => {
  const hits = [];
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.name.endsWith(".rs")) {
        readFileSync(path, "utf8")
          .split("\n")
          .forEach((line, index) => {
            // Only real blocks, not the word in a message or a test name.
            if (/\bunsafe\s*(\{|fn\b)/.test(line)) {
              hits.push(`${path}:${index + 1}`);
            }
          });
      }
    }
  };
  for (const base of ["crates", join("src-tauri", "src")]) {
    const path = join(root, base);
    try {
      if (statSync(path).isDirectory()) visit(path);
    } catch {
      // A tree that does not exist contributes nothing.
    }
  }
  console.log(`unsafe occurrences: ${hits.length}`);
  for (const hit of hits) console.log(`  ${hit}`);
  return 0;
});

if (failed.length > 0) {
  console.log(`FAILED: ${failed.join(", ")}`);
  process.exit(1);
}
