#!/usr/bin/env node
// Create the release keystore Android needs, and the properties file the Tauri
// Gradle project reads when it signs a release build.
//
//   bun run android:keystore
//   FLUXY_KEYSTORE_PASSWORD=... bun run android:keystore -- --force
//
// The keystore and its password live outside version control (see .gitignore):
// losing them means an update can no longer be installed over the old app, so
// back both up before shipping anything.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { randomBytes } from "node:crypto";

const root = resolve(import.meta.dirname, "..", "..");
const args = process.argv.slice(2);
const force = args.includes("--force");
const isWindows = process.platform === "win32";

function fail(message) {
  console.error(`keystore: ${message}`);
  process.exit(1);
}

const javaHome =
  process.env.JAVA_HOME ??
  [
    join("C:", "Program Files", "Android", "Android Studio", "jbr"),
    "/Applications/Android Studio.app/Contents/jbr/Contents/Home",
  ].find((path) => existsSync(path));
if (!javaHome) fail("no JDK; set JAVA_HOME");

const keytool = join(javaHome, "bin", isWindows ? "keytool.exe" : "keytool");
if (!existsSync(keytool)) fail(`keytool not found at ${keytool}`);

const secrets = join(root, ".android-secrets");
const store = join(secrets, "fluxy-release.jks");
const properties = join(root, "src-tauri", "gen", "android", "keystore.properties");

if (existsSync(store)) {
  if (!force) {
    fail(
      `${store} already exists; pass --force to replace it (the old keystore cannot sign updates afterwards)`,
    );
  }
  // keytool refuses to overwrite an existing key, so the file goes first.
  rmSync(store, { force: true });
}
mkdirSync(secrets, { recursive: true });

const password = process.env.FLUXY_KEYSTORE_PASSWORD ?? randomBytes(18).toString("base64url");
const alias = "fluxy";

const generated = spawnSync(
  keytool,
  [
    "-genkeypair",
    "-v",
    "-keystore",
    store,
    "-alias",
    alias,
    "-keyalg",
    "RSA",
    "-keysize",
    "2048",
    "-validity",
    "10000",
    "-storepass",
    password,
    "-keypass",
    password,
    "-dname",
    "CN=Fluxy, OU=Release, O=Fluxy, C=US",
  ],
  { stdio: "inherit" },
);
if (generated.status !== 0) fail("keytool refused to create the keystore");

writeFileSync(
  properties,
  [
    `storeFile=${store.replace(/\\/g, "/")}`,
    `password=${password}`,
    `keyAlias=${alias}`,
    "",
  ].join("\n"),
);

console.log(`keystore: ${store}`);
console.log(`properties: ${properties}`);
console.log("password: stored in the properties file; back up both before releasing");
