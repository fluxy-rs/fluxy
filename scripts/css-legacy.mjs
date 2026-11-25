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

/// Tailwind positions with the individual transform properties (translate:,
/// rotate:, scale:), which Chrome 104 introduced. An older engine drops them
/// silently - a dialog keeps left/top of 50% and loses the -50% that made it
/// centre, so it lands bottom-right and its buttons fall off the panel. Every
/// rule that sets them is rewritten into one transform in the order the spec
/// composes them: translate, then rotate, then scale, then any transform the
/// rule already had.
const transformProperties = () => ({
  postcssPlugin: "fx-transform-properties",
  Once(root) {
    root.walkRules((rule) => {
      const declarations = (rule.nodes ?? []).filter((node) => node.type === "decl");
      const translate = declarations.find((node) => node.prop === "translate");
      const rotate = declarations.find((node) => node.prop === "rotate");
      const scale = declarations.find((node) => node.prop === "scale");
      if (!translate && !rotate && !scale) return;

      // The other half of a translation lives in an @property initial value,
      // which this engine also ignores; without a fallback the whole function
      // is invalid and the element is left where it was.
      const fallback = (value, rest) => value.replace(/var\((--tw-[a-z-]+)\)/g, `var($1, ${rest})`);
      const parts = [];
      if (translate) parts.push(`translate(${fallback(translate.value, "0")})`);
      if (rotate) parts.push(`rotate(${fallback(rotate.value, "0deg")})`);
      if (scale) parts.push(`scale(${fallback(scale.value, "1")})`);

      const existing = declarations.find((node) => node.prop === "transform");
      if (existing) {
        parts.push(existing.value);
        existing.remove();
      }
      const anchor = translate ?? rotate ?? scale;
      anchor.cloneBefore({ prop: "transform", value: parts.join(" ") });
      for (const node of [translate, rotate, scale]) node?.remove();
    });
  },
});
transformProperties.postcss = true;
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

/// color-mix(in oklab, var(--x) 40%, transparent) needs Chrome 111, and an
/// engine without it falls back to whatever came before the guarded rule.
/// Tailwind writes that fallback as the *solid* token - so every opacity
/// modifier degraded to full strength: `bg-warning/10` painted solid warning,
/// which then hid `text-warning` on top of itself.
///
/// Every colour token is a plain rgb() by now, so the same alpha can be written
/// rgb(var(--x-rgb) / 40%). This adds the triple beside each token, rewrites
/// every mix it can - inside the @supports block as well - and then unwraps
/// those guards, because what is left no longer needs one.
const tokenColorMix = () => ({
  postcssPlugin: "fx-token-colour-mix",
  // OnceExit, not Once: the tokens are still oklch() when the first pass sees
  // them, and only the oklab plugin turns them into rgb().
  OnceExit(root) {
    // The triple is taken from the declaration it sits beside, not from a
    // shared map: a token is defined once per theme, and the themes differ.
    const tripleOf = (value) => {
      if (!/^rgb\(/.test(value)) return null;
      const parts = value
        .replace(/^rgb\(|\)$/g, "")
        .replace(/\//g, " ")
        .split(/[\s,]+/)
        .filter(Boolean)
        .slice(0, 3);
      return parts.length === 3 ? parts.join(" ") : null;
    };
    const known = new Set();
    root.walkDecls(/^--[a-z0-9-]+$/, (decl) => {
      if (decl.prop.endsWith("-rgb")) return;
      const triple = tripleOf(decl.value);
      if (!triple) return;
      known.add(decl.prop);
      decl.after(decl.clone({ prop: `${decl.prop}-rgb`, value: triple }));
    });

    const pattern = () =>
      /color-mix\(in oklab,\s*var\((--[a-z0-9-]+)\)\s+([0-9.]+)%\s*,\s*transparent\s*\)/g;
    root.walkDecls((decl) => {
      if (!decl.value.includes("color-mix(")) return;
      decl.value = decl.value.replace(pattern(), (match, token, percent) => {
        if (!known.has(token)) return match;
        return `rgb(var(${token}-rgb) / ${percent}%)`;
      });
    });

    // A guard that only protected a colour-mix is now protecting rgb(); drop
    // it so the rewritten declaration wins over the solid fallback.
    root.walkAtRules("supports", (at) => {
      if (!at.params.includes("color-mix") || !at.nodes) return;
      let remains = false;
      at.walkDecls((decl) => {
        if (decl.value.includes("color-mix(")) remains = true;
      });
      if (!remains) at.replaceWith(at.nodes);
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
    transformProperties(),
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
      ".tint{background-color:var(--primary)}@supports (color:color-mix(in lab, red, red)){.tint{background-color:color-mix(in oklab, var(--primary) 10%, transparent)}}",
      ".z{height:100dvh}",
      "--tw-translate-x:-50%;.c{--tw-translate-y:-50%;translate:var(--tw-translate-x) var(--tw-translate-y)}",
      ".s{scale:var(--tw-scale-x) var(--tw-scale-y)}",
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
      ["a guarded tint is lowered", out.includes("rgb(var(--primary-rgb) / 10%)")],
      ["its guard is unwrapped", !out.includes("@supports (color:color-mix")],
      ["dvh gains a vh fallback", out.includes("height:100vh")],
      [
        "translate becomes transform",
        out.includes("transform:translate(var(--tw-translate-x, 0) var(--tw-translate-y, 0))"),
      ],
      ["the individual property is gone", !out.includes("translate:var(")],
      ["scale falls back to 1", out.includes("scale(var(--tw-scale-x, 1) var(--tw-scale-y, 1))")],
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
