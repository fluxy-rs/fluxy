import { describe, expect, it } from "vitest";
import { catalogs } from "./index";

/**
 * Every source file, read at build time. Vite inlines the text, so the guard
 * needs no filesystem access and stays inside the browser test environment.
 */
const modules = import.meta.glob("../**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const sources = Object.entries(modules)
  .map(([key, text]) => ({
    // Glob keys are relative to this file, so resolve them to a path under src/.
    file: new URL(key, import.meta.url).pathname.replace(/.*\/src\//, ""),
    text,
  }))
  .filter(({ file }) => !/\.test\.tsx?$/.test(file));

const bindings = modules["../bindings.ts"] ?? "";

/**
 * Failure reasons mirror the Rust error enum, so the guard reads it from the
 * source instead of keeping a second list in sync by hand.
 */
const rust = import.meta.glob("../../crates/fluxy-core/src/error.rs", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;
const errorSource = rust["../../crates/fluxy-core/src/error.rs"] ?? "";
const zh = catalogs["zh-CN"];
const en = catalogs.en;
const namespaces = new Set(Object.keys(zh).map((key) => key.split(".")[0]));

/** Dotted literals that look like catalog keys, in any string context. */
function literalKeys(text: string): string[] {
  const found = new Set<string>();
  for (const match of text.matchAll(/["'`]([a-z][a-zA-Z0-9]*(?:\.[a-zA-Z0-9]+)+)["'`]/g)) {
    found.add(match[1]);
  }
  for (const match of text.matchAll(/\bt\(\s*["'`]([^"'`$]+)["'`]/g)) {
    found.add(match[1]);
  }
  return [...found];
}

const used = new Map<string, string[]>();
for (const { file, text } of sources) {
  // The catalog itself holds every key; only its consumers prove usage.
  if (file === "i18n/index.tsx") continue;
  for (const key of literalKeys(text)) {
    if (!namespaces.has(key.split(".")[0])) continue;
    used.set(key, [...(used.get(key) ?? []), file]);
  }
}

/** State unions are rendered through template keys, so expand them too. */
function unionMembers(name: string): string[] {
  const match = bindings.match(new RegExp(`export type ${name} =([^;]+);`));
  if (!match) {
    throw new Error(`no union ${name} in bindings.ts`);
  }
  return [...match[1].matchAll(/"([^"]+)"/g)].map((entry) => entry[1]);
}

/** The stable snake_case codes from `ErrorCode::as_str`. */
function errorCodes(): string[] {
  const codes = [...errorSource.matchAll(/ErrorCode::\w+ => "([a-z_]+)"/g)].map(
    (match) => match[1],
  );
  if (codes.length < 10 || !codes.includes("disk_full")) {
    throw new Error(
      "could not read the error codes from crates/fluxy-core/src/error.rs",
    );
  }
  return codes;
}

const dynamicKeys = [
  ...errorCodes().map((code) => `transfers.reason.${code}`),
  // Reject codes the shell sends that have no ErrorCode variant: a transfer
  // message can carry its own reason, and failureReason still looks it up.
  "transfers.reason.rejected",
  "transfers.reason.invalid_manifest",
  "transfers.reason.invalid_batch",
  ...unionMembers("ClipboardShareState").map((state) => `clipboard.state.${state}`),
  ...unionMembers("ClipboardDirection").map(
    (direction) => `clipboard.direction.${direction}`,
  ),
  ...unionMembers("TransferState").map((state) => `transfers.state.${state}`),
];

function placeholders(value: string): string[] {
  return [...value.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
}

describe("i18n catalog", () => {
  it("keeps both locales in step", () => {
    const zhKeys = Object.keys(zh).sort();
    const enKeys = Object.keys(en).sort();
    const missingInEn = zhKeys.filter((key) => !(key in en));
    const missingInZh = enKeys.filter((key) => !(key in zh));
    expect({ missingInEn, missingInZh }).toEqual({
      missingInEn: [],
      missingInZh: [],
    });
    expect(zhKeys.length).toBeGreaterThan(300);
  });

  it("has no empty copy and matching placeholders", () => {
    const problems: string[] = [];
    for (const key of Object.keys(zh)) {
      if (!zh[key]?.trim()) {
        problems.push(`zh-CN ${key} is empty`);
      }
      if (!en[key]?.trim()) {
        problems.push(`en ${key} is empty`);
      }
      const zhVars = placeholders(zh[key] ?? "");
      const enVars = placeholders(en[key] ?? "");
      if (zhVars.join(",") !== enVars.join(",")) {
        problems.push(
          `${key} placeholders differ: zh[${zhVars}] en[${enVars}]`,
        );
      }
    }
    expect(problems).toEqual([]);
  });

  it("resolves every key written in the source", () => {
    const missing = [...used.entries()]
      .filter(([key]) => !(key in zh) || !(key in en))
      .map(
        ([key, files]) =>
          `${key} (used in ${[...new Set(files)].join(", ")})`,
      );
    expect(missing).toEqual([]);
  });

  it("covers the state keys built from the binding unions", () => {
    const missing = dynamicKeys.filter((key) => !(key in zh) || !(key in en));
    expect(missing).toEqual([]);
  });

  it("has a failure reason for every protocol error code", () => {
    const missing = errorCodes().filter(
      (code) => !(`transfers.reason.${code}` in zh) ||
        !(`transfers.reason.${code}` in en),
    );
    expect(missing).toEqual([]);
  });

  it("keeps interface copy out of the components", () => {
    // The catalog is the only place that may hold translated copy; source files
    // outside it are written in English, so any CJK character is a hardcoded
    // string that would ignore the locale.
    const offenders = sources
      .filter(({ file }) => !file.startsWith("i18n/"))
      .filter(({ text }) => /[\u4e00-\u9fff]/.test(text))
      .map(({ file }) => file);
    expect(offenders).toEqual([]);
  });

  it("does not leave unused copy behind for the keys it can see", () => {
    const orphans = Object.keys(zh).filter((key) => !used.has(key));
    // Keys consumed through unions or indirect maps are expected to be here.
    const allowed = new Set(dynamicKeys);
    const unexpected = orphans.filter((key) => !allowed.has(key));
    expect(unexpected).toEqual([]);
  });
});
