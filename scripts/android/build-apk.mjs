#!/usr/bin/env node
// Build the Android APK and check the two release-critical properties: the 16 KB
// page alignment of every shipped library (Android 15+) and the signature.
//
//   bun run android:build                 # debug, arm64
//   bun run android:build -- --release    # release (needs a keystore)
//   bun run android:build -- --target x86_64
//
// Requires a JDK and an Android SDK with platform-tools, build-tools and an NDK.
// JAVA_HOME and ANDROID_HOME are honoured; otherwise the usual per-platform
// install locations are tried. Everything here runs on Windows, macOS and Linux.

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..", "..");
const args = process.argv.slice(2);
const release = args.includes("--release");
const targetIndex = args.indexOf("--target");
const target = targetIndex >= 0 ? args[targetIndex + 1] : "aarch64";
if (!target) fail("--target needs an architecture");

const triples = {
  aarch64: "aarch64-linux-android",
  armv7: "armv7-linux-androideabi",
  i686: "i686-linux-android",
  x86_64: "x86_64-linux-android",
};
const triple = triples[target];
if (!triple) fail(`unknown target ${target}; pick one of ${Object.keys(triples).join(", ")}`);

const isWindows = process.platform === "win32";
const exe = (name) => (isWindows ? `${name}.exe` : name);

function fail(message) {
  console.error(`android build: ${message}`);
  process.exit(1);
}

function firstExisting(candidates) {
  return candidates.find((candidate) => candidate && existsSync(candidate));
}

function sdkRoot() {
  const fromEnv = [process.env.ANDROID_HOME, process.env.ANDROID_SDK_ROOT].find(Boolean);
  if (fromEnv && existsSync(fromEnv)) return fromEnv;
  const home = process.env.HOME ?? process.env.USERPROFILE ?? "";
  const local = process.env.LOCALAPPDATA ?? "";
  return firstExisting([
    join(home, "Android", "Sdk"),
    join(local, "Android", "Sdk"),
    join(home, "Library", "Android", "sdk"),
  ]);
}

function jdkRoot() {
  const fromEnv = process.env.JAVA_HOME;
  if (fromEnv && existsSync(fromEnv)) return fromEnv;
  const home = process.env.HOME ?? process.env.USERPROFILE ?? "";
  const local = process.env.LOCALAPPDATA ?? "";
  return firstExisting([
    join("C:", "Program Files", "Android", "Android Studio", "jbr"),
    join(local, "Program Files", "Android", "Studio", "jbr"),
    "/Applications/Android Studio.app/Contents/jbr/Contents/Home",
    join(home, "android-studio", "jbr"),
    "/usr/lib/jvm/default-java",
  ]);
}

/// Newest directory by name, which is how the SDK versions its subdirectories.
function newestParent(...segments) {
  const parent = join(...segments);
  if (!existsSync(parent)) return undefined;
  const entries = readdirSync(parent)
    .filter((name) => statSync(join(parent, name)).isDirectory())
    .sort((a, b) => b.localeCompare(a, undefined, { numeric: true }));
  return entries.length > 0 ? join(parent, entries[0]) : undefined;
}

function run(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, {
    cwd: root,
    stdio: "inherit",
    // bun is a .cmd shim on Windows, so it needs a shell there; the argument
    // list is fixed, never user input.
    shell: isWindows,
    ...options,
  });
  if (result.error) fail(`${command} could not start: ${result.error.message}`);
  return result.status ?? 0;
}

const androidHome = sdkRoot();
if (!androidHome) fail("no Android SDK; set ANDROID_HOME");
const javaHome = jdkRoot();
if (!javaHome) fail("no JDK; set JAVA_HOME");
const ndkHome =
  process.env.NDK_HOME && existsSync(process.env.NDK_HOME)
    ? process.env.NDK_HOME
    : newestParent(androidHome, "ndk");
if (!ndkHome) fail("no NDK under the Android SDK");

const env = {
  ...process.env,
  JAVA_HOME: javaHome,
  ANDROID_HOME: androidHome,
  ANDROID_SDK_ROOT: androidHome,
  NDK_HOME: ndkHome,
  PATH: [join(javaHome, "bin"), join(androidHome, "platform-tools"), process.env.PATH ?? ""].join(
    isWindows ? ";" : ":",
  ),
};

const flavor = release ? "release" : "debug";
if (release) {
  const properties = join(root, "src-tauri", "gen", "android", "keystore.properties");
  if (!existsSync(properties)) {
    // An unsigned release APK installs nowhere; catching it here is far cheaper
    // than a failed install on a phone.
    fail(
      "a release build needs a keystore, and " +
        `${properties} does not exist.\n` +
        "  bun run android:keystore      # create one, then build again\n" +
        "  bun run android:build         # or just build the debug APK, which is signed",
    );
  }
}
const skipBuild = args.includes("--skip-build");
if (!skipBuild) console.log(`building APK (${flavor}, ${target})`);
const buildArgs = ["x", "tauri", "android", "build"];
if (!release) buildArgs.push("--debug");
buildArgs.push("--apk", "--target", target);
if (!skipBuild) {
  const status = run("bun", buildArgs, { env });
  if (status !== 0) fail(`tauri android build exited with ${status}`);
}

const apkRoot = join(root, "src-tauri", "gen", "android", "app", "build", "outputs", "apk");
const apks = [];
(function collect(directory) {
  if (!existsSync(directory)) return;
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) collect(path);
    else if (entry.name.endsWith(".apk") && entry.name.includes(flavor)) apks.push(path);
  }
})(apkRoot);
if (apks.length === 0) fail("the APK was not produced");
if (release && apks.every((path) => path.includes("unsigned"))) {
  fail("the release APK came out unsigned, so a phone will refuse it; check keystore.properties");
}
apks.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
const apk = apks[0];
console.log(`apk: ${apk} (${statSync(apk).size} bytes)`);

// Tauri names every APK "universal" even when it holds a single ABI, so read the
// archive and report what it actually contains.
function apkAbis(path) {
  const buffer = readFileSync(path);
  const END = 0x06054b50;
  let end = buffer.length - 22;
  while (end >= 0 && buffer.readUInt32LE(end) !== END) end -= 1;
  if (end < 0) return [];
  const count = buffer.readUInt16LE(end + 10);
  let offset = buffer.readUInt32LE(end + 16);
  const abis = new Set();
  for (let index = 0; index < count; index += 1) {
    if (buffer.readUInt32LE(offset) !== 0x02014b50) break;
    const nameLength = buffer.readUInt16LE(offset + 28);
    const extraLength = buffer.readUInt16LE(offset + 30);
    const commentLength = buffer.readUInt16LE(offset + 32);
    const name = buffer.toString("utf8", offset + 46, offset + 46 + nameLength);
    const match = /^lib\/([^/]+)\//.exec(name);
    if (match) abis.add(match[1]);
    offset += 46 + nameLength + extraLength + commentLength;
  }
  return [...abis].sort();
}

const abis = apkAbis(apk);
if (abis.length > 0) console.log(`abis: ${abis.join(", ")}`);

// 16 KB page check. The library that ships is the one cargo just linked, so the
// build output is what gets inspected: no zip parsing, no temp directory.
const prebuilt = newestParent(ndkHome, "toolchains", "llvm", "prebuilt");
if (!prebuilt) fail(`no LLVM toolchain under ${ndkHome}`);
const readelf = join(prebuilt, "bin", exe("llvm-readelf"));
if (!existsSync(readelf)) fail(`llvm-readelf not found under ${prebuilt}`);

const buildDir = join(root, "target", triple, flavor);
if (!existsSync(buildDir)) fail(`no build output at ${buildDir}`);
const libraries = readdirSync(buildDir).filter((name) => name.endsWith(".so"));
if (libraries.length === 0) fail(`no .so in ${buildDir}`);

let bad = 0;
for (const library of libraries) {
  const result = spawnSync(readelf, ["-lW", join(buildDir, library)], { encoding: "utf8" });
  const aligns = new Set(
    (result.stdout ?? "")
      .split("\n")
      .filter((line) => line.trimStart().startsWith("LOAD"))
      .map((line) => line.trim().split(/\s+/).at(-1)),
  );
  const ok = aligns.size === 1 && aligns.has("0x4000");
  console.log(
    `${library}: LOAD align ${[...aligns].join(",") || "none"} ${ok ? "OK (16 KB)" : "NOT 16 KB"}`,
  );
  if (!ok) bad += 1;
}
if (bad > 0) fail(`${bad} native libraries are not 16 KB aligned`);

// Not every build-tools version ships the signer (37.0.0 does not), so scan
// from the newest down and take the first that has one.
function findApksigner(sdk) {
  const parent = join(sdk, "build-tools");
  if (!existsSync(parent)) return undefined;
  const versions = readdirSync(parent)
    .filter((name) => statSync(join(parent, name)).isDirectory())
    .sort((a, b) => b.localeCompare(a, undefined, { numeric: true }));
  for (const version of versions) {
    const found = firstExisting([
      join(parent, version, exe("apksigner")),
      join(parent, version, "apksigner.bat"),
    ]);
    if (found) return found;
  }
  return undefined;
}
const apksigner = findApksigner(androidHome);
if (!apksigner) {
  console.warn("android build: apksigner not found; the signature was not checked");
  process.exit(0);
}
// The signer is a wrapper script that needs JAVA_HOME, so it gets the same
// environment the build had.
const verified = spawnSync(apksigner, ["verify", "--print-certs", apk], {
  encoding: "utf8",
  env,
  // apksigner is a .bat on Windows, which needs a shell to run.
  shell: isWindows,
});
for (const line of `${verified.stdout ?? ""}${verified.stderr ?? ""}`.split("\n")) {
  if (line.includes("certificate DN") || line.includes("SHA-256 digest")) {
    console.log(line.trim());
  }
}
if (verified.status !== 0) fail(`apksigner rejected ${apk}`);
