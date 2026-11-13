#!/usr/bin/env node
// Rewrites the built stylesheet into something an old WebView can read.
//
// A car head unit runs Chrome 95 and reported "every style is gone", then a
// layout with no height, no sidebar and dialogs in the wrong place. Four
// features are newer than that engine: @layer (99), the range media syntax
// (104), oklch() and color-mix() (111), and dvh units (108).
//
// Three of the four have a maintained PostCSS plugin and use it: media
// min/max, oklab and color-mix come from @csstools. Two are handled here as
// PostCSS visitors, because no plugin covers them: dvh fallbacks, and
// color-mix over a var() token (the plugin resolves literal colours only, and
// this palette is entirely variables).
//
// Cascade layers are unwrapped by hand instead of @csstools/postcss-cascade-
// layers. That plugin keeps layer semantics by inflating every selector with
// :not(#\#) hacks; with the whole sheet flattened there is nothing left to
// order against, and the hacks would rewrite thousands of selectors on a
// device that cannot be inspected.
//
// Runs after `vite build`; it edits dist/assets/*.css in place.

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import postcss from "postcss";
import mediaMinmax from "@csstools/postcss-media-minmax";
import oklabFunction from "@csstools/postcss-oklab-function";
import colorMixFunction from "@csstools/postcss-color-mix-function";

const ASSETS = join("dist", "assets");

/// Older engines ignore a dvh value outright, so the height collapses to
/// nothing. Emitting the vh form first gives them something to use; the dvh
/// declaration stays for engines that understand it.
/// `oklch(98.5% 0 none)` is valid CSS and the @csstools plugin does not read
/// the keyword, so the neutral palette survived untouched. `none` means a zero
/// hue; spelling it as 0 lets the plugin do the conversion.
const oklchNoneHue = () => ({
  postcssPlugin: "fx-oklch-none",
  Declaration(decl) {
    if (!decl.value.includes("oklch(") || !decl.value.includes("none")) return;
    decl.value = decl.value.split("none").join("0");
  },
});
oklchNoneHue.postcss = true;

const dvhFallback = () => ({
  postcssPlugin: "fx-dvh-fallback",
  Declaration(decl) {
    if (!/\d(?:dvh|svh|lvh|dvw|svw|lvw)/.test(decl.value)) return;
    const fallback = decl.value.replace(/dvh/g, "vh").replace(/svh/g, "vh").replace(/lvh/g, "vh");
    if (fallback === decl.value) return;
    decl.cloneBefore({ value: fallback });
  },
});
dvhFallback.postcss = true;

/// color-mix(in oklab, var(--x) 40%, transparent) needs Chrome 111. Every
/// colour token is a plain rgb() at this point, so the mix can be written as
/// rgb(var(--x-rgb) / 40%) - but only if the token has an rgb triple beside
/// it, one per theme, which this adds.
const tokenColorMix = () => ({
  postcssPlugin: "fx-token-colour-mix",
  Once(root) {
    const triples = new Map();
    root.walkDecls(/^--[a-z0-9-]+$/, (decl) => {
      if (!/^rgb\(/.test(decl.value)) return;
      const parts = decl.value
        .replace(/^rgb\(|\)$/g, "")
        .replace(/\//g, " ")
        .split(/[\s,]+/)
        .filter(Boolean)
        .slice(0, 3);
      if (parts.length === 3) triples.set(decl.prop, parts.join(" "));
    });

    root.walkDecls((decl) => {
      const pattern =
        /color-mix\(in oklab,\s*var\((--[a-z0-9-]+)\)\s+([0-9.]+)%\s*,\s*transparent\s*\)/g;
      if (!pattern.test(decl.value)) return;
      decl.value = decl.value.replace(pattern, (match, token, percent) => {
        const triple = triples.get(token);
        if (!triple) return match;
        const prop = token + "-rgb";
        decl.parent.insertBefore(decl, decl.clone({ prop, value: triple }));
        return `rgb(var(${prop}) / ${percent}%)`;
      });
    });
  },
});
tokenColorMix.postcss = true;

/**
 * Unwrap @layer blocks and drop the @layer statements.
 *
 * Parsed, not scanned: an @layer inside a string or a URL would defeat a text
 * search, and one brace miscounted would corrupt the sheet.
 */
const unwrapLayers = () => ({
  postcssPlugin: "fx-unwrap-layers",
  Once(root) {
    root.walkAtRules("layer", (at) => {
      if (at.nodes) at.replaceWith(at.nodes);
      else at.remove();
    });
  },
});
unwrapLayers.postcss = true;

export async function legacyCss(css) {
  const result = await postcss([
    unwrapLayers(),
    mediaMinmax(),
    oklchNoneHue(),
    oklabFunction(),
    colorMixFunction(),
    dvhFallback(),
    tokenColorMix(),
  ]).process(css, { from: undefined });
  return result.css;
}

function selfTest() {
  return legacyCss(
    [
      "@layer a, b;",
      "@layer utilities{.x{color:red}}",
      "@media (width>=48rem){.md{color:blue}}",
      ":root{--primary:rgb(10 10 10)}",
      ".y{background:color-mix(in oklab, var(--primary) 40%, transparent)}",
      ".z{height:100dvh}",
    ].join("\n"),
  ).then((out) => {
    const checks = [
      ["the layer statement goes", !out.includes("@layer")],
      ["the rules stay", out.includes(".x{color:red}")],
      [
        "a range query becomes min-width",
        out.includes("(min-width: 48rem)") || out.includes("(min-width:48rem)"),
      ],
      ["a token gains an rgb twin", out.includes("--primary-rgb:10 10 10")],
      ["color-mix becomes rgb()", out.includes("rgb(var(--primary-rgb) / 40%)")],
      ["dvh gains a vh fallback", out.includes("height:100vh")],
    ];
    let failed = 0;
    for (const [name, ok] of checks) {
      if (!ok) {
        console.error("self-test failed: " + name);
        failed += 1;
      }
    }
    return failed === 0;
  });
}

if (process.argv.includes("--self-test")) {
  const ok = await selfTest();
  if (!ok) process.exit(1);
  console.log("css legacy: self-test ok");
  process.exit(0);
}

let changed = 0;
for (const name of readdirSync(ASSETS)) {
  if (!name.endsWith(".css")) continue;
  const path = join(ASSETS, name);
  const before = readFileSync(path, "utf8");
  const after = await legacyCss(before);
  if (after === before) continue;
  writeFileSync(path, after);
  changed += 1;
  const layers = (before.match(/@layer/g) ?? []).length;
  const colours = (before.match(/oklch\(/g) ?? []).length;
  console.log(`${name}: ${layers} @layer, ${colours} oklch -> lowered`);
}
if (changed === 0) console.log("css legacy: nothing to lower");
