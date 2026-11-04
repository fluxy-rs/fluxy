import { readFile, readdir } from "node:fs/promises";
import { join, extname } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const failures = [];

function fail(message) {
  failures.push(message);
}

async function readJson(path) {
  return JSON.parse(await readFile(join(root, path), "utf8"));
}

const config = await readJson("src-tauri/tauri.conf.json");
const security = config.app?.security ?? {};
const csp = security.csp ?? "";
const devCsp = security.devCsp ?? "";

function directive(policy, name) {
  return policy
    .split(";")
    .map((part) => part.trim())
    .find((part) => part === name || part.startsWith(`${name} `)) ?? "";
}

const expected = [
  ["default-src", "default-src 'self'"],
  ["script-src", "script-src 'self'"],
  ["object-src", "object-src 'none'"],
  ["connect-src", "connect-src 'self' ipc: http://ipc.localhost"],
];
for (const [name, value] of expected) {
  if (directive(csp, name) !== value) {
    fail(`production CSP ${name} must be exactly "${value}", got "${directive(csp, name) || "(missing)"}"`);
  }
}
if (csp.includes("unsafe-eval")) fail("production CSP allows unsafe-eval");
if (directive(csp, "script-src").includes("unsafe-inline")) {
  fail("production script-src must not allow inline scripts");
}
if (!devCsp.includes("ws://localhost:1420")) fail("dev CSP is missing the Vite HMR websocket");
if (security.freezePrototype !== true) fail("freezePrototype must be enabled");

const capabilities = await readJson("src-tauri/capabilities/default.json");
const permissions = capabilities.permissions ?? [];
// Only capabilities that are deliberately part of the product belong here.
// "fluxy-mobile:default" is the app's own bridge. "barcode-scanner:default" is
// the official Tauri scanner, which opens a native camera view and returns a
// short string - it cannot read files, spawn processes or reach the network.
// Anything else - shell, opener, fs - would hand the WebView more than it needs.
const allowed = new Set([
  "core:default",
  "fluxy-mobile:default",
  "barcode-scanner:default",
  // The UI asks whether notification permission is granted before offering the
  // switch, so this capability is genuinely used; the notifications themselves
  // are still raised from Rust.
  "notification:default",
]);
const unexpected = permissions.filter((entry) => !allowed.has(entry));
if (unexpected.length > 0 || !permissions.includes("core:default")) {
  fail(`capability permissions must be exactly core:default, got ${JSON.stringify(permissions)}`);
}
if (permissions.some((permission) => String(permission).startsWith("opener:"))) {
  fail("opener permission must not be present");
}

const cargo = await readFile(join(root, "src-tauri/Cargo.toml"), "utf8");
const packageJson = await readFile(join(root, "package.json"), "utf8");
if (cargo.includes("tauri-plugin-opener")) fail("Cargo.toml still depends on tauri-plugin-opener");
if (packageJson.includes("@tauri-apps/plugin-opener")) fail("package.json still depends on the opener plugin");

const sinkPatterns = [
  [/\.innerHTML\s*=/, "innerHTML assignment"],
  [/\.outerHTML\s*=/, "outerHTML assignment"],
  [/dangerouslySetInnerHTML/, "dangerouslySetInnerHTML"],
  [/\beval\s*\(/, "eval()"],
  [/\bnew Function\s*\(/, "new Function()"],
  [/document\.write\s*\(/, "document.write()"],
  [/window\.open\s*\(/, "window.open()"],
  [/location\.(?:href|assign|replace)\s*=/, "direct location assignment"],
];

async function filesUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await filesUnder(path)));
    else if ([".ts", ".tsx"].includes(extname(entry.name))) files.push(path);
  }
  return files;
}

for (const file of await filesUnder(join(root, "src"))) {
  const source = await readFile(file, "utf8");
  for (const [pattern, label] of sinkPatterns) {
    if (pattern.test(source)) fail(`${file.replace(root, "")}: uses ${label}`);
  }
  if (/<a\b[^>]*target\s*=\s*["']_blank["']/.test(source)) {
    fail(`${file.replace(root, "")}: opens a new browsing context`);
  }
}

if (failures.length > 0) {
  console.error("UI security check failed:");
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exit(1);
}

console.log("UI security check passed: strict CSP, no opener, minimal capabilities, no HTML/eval sinks.");
