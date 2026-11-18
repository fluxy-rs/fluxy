#!/usr/bin/env node
// Rewrites the built stylesheet into something an old WebView can read.
//
// A car head unit reported "every style is gone". The bundle is Tailwind v4,
// and its output is wrapped in @layer - which an engine older than Chrome 99
// discards whole, so not one rule applies. The palette is written in oklch(),
// which needs Chrome 111; there the layout would survive and the colours would
// not. Both are lowered here, in the built file, so the source stays modern and
// the shipped CSS stays readable by engines that never heard of either.
//
// Run after `vite build`; it edits dist/assets/*.css in place.

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const ASSETS = join("dist", "assets");

/** Oklch -> sRGB, the standard conversion through Oklab and linear sRGB. */
function oklchToRgb(lightness, chroma, hue) {
  const radians = (hue * Math.PI) / 180;
  const a = chroma * Math.cos(radians);
  const b = chroma * Math.sin(radians);

  const l = lightness + 0.3963377774 * a + 0.2158037573 * b;
  const m = lightness - 0.1055613458 * a - 0.0638541728 * b;
  const s = lightness - 0.0894841775 * a - 1.291485548 * b;

  const l3 = l * l * l;
  const m3 = m * m * m;
  const s3 = s * s * s;

  const linear = [
    4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3,
    -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3,
    -0.0041960863 * l3 - 0.7034186147 * m3 + 1.707614701 * s3,
  ];

  const encode = (value) => {
    const clamped = Math.min(1, Math.max(0, value));
    return clamped <= 0.0031308
      ? Math.round(clamped * 12.92 * 255)
      : Math.round((1.055 * Math.pow(clamped, 1 / 2.4) - 0.055) * 255);
  };

  return linear.map(encode);
}

/** Replace every oklch() literal with the rgb()/rgba() an old engine knows. */
export function lowerOklch(css) {
  return css.replace(/oklch\(([^()]*)\)/g, (match, body) => {
    const [colorPart, alphaPart] = body.split("/");
    const parts = colorPart.trim().split(/\s+/);
    if (parts.length < 3) return match;
    // The shipped form is "70.4% .191 22.216": lightness in percent, chroma
    // as a fraction (percent means of 0.4), and a hue that may be "none".
    const lightness = parts[0].endsWith("%") ? Number.parseFloat(parts[0]) / 100 : Number(parts[0]);
    const chroma = parts[1].endsWith("%") ? Number.parseFloat(parts[1]) * 0.004 : Number(parts[1]);
    const hueToken = parts[2].replace(/deg$/, "");
    const hue = hueToken === "none" ? 0 : Number(hueToken);
    if ([lightness, chroma, hue].some((value) => Number.isNaN(value))) return match;
    const [r, g, b] = oklchToRgb(lightness, chroma, hue);
    const alpha = alphaPart === undefined ? 1 : Number(alphaPart.trim());
    if (Number.isNaN(alpha)) return match;
    return alpha >= 1 ? `rgb(${r} ${g} ${b})` : `rgba(${r} ${g} ${b} / ${alpha})`;
  });
}

/**
 * Unwrap @layer blocks and drop the @layer statements.
 *
 * The body is kept, the wrapper goes. Braces are counted rather than matched
 * with a regex, so a layer that contains nested blocks survives intact; strings
 * and comments are skipped because both may legally contain a brace.
 */
export function flattenLayers(css) {
  let out = "";
  let index = 0;

  while (index < css.length) {
    const start = css.indexOf("@layer", index);
    if (start === -1) {
      out += css.slice(index);
      break;
    }
    out += css.slice(index, start);
    let cursor = start + "@layer".length;
    const open = css.indexOf("{", cursor);
    const semicolon = css.indexOf(";", cursor);

    // `@layer a, b, c;` declares order and carries no rules.
    if (semicolon !== -1 && (open === -1 || semicolon < open)) {
      index = semicolon + 1;
      continue;
    }
    if (open === -1) {
      out += css.slice(start);
      break;
    }

    let depth = 0;
    let end = open;
    let quote = null;
    for (; end < css.length; end += 1) {
      const character = css[end];
      const previous = css[end - 1];
      if (quote) {
        if (character === quote && previous !== "\\") quote = null;
        continue;
      }
      if (character === '"' || character === "\u0027") {
        quote = character;
        continue;
      }
      if (character === "/" && css[end + 1] === "*") {
        const close = css.indexOf("*/", end + 2);
        if (close === -1) break;
        end = close + 1;
        continue;
      }
      if (character === "{") depth += 1;
      if (character === "}") {
        depth -= 1;
        if (depth === 0) break;
      }
    }

    // Keep what the layer contained, without its braces.
    out += css.slice(open + 1, end);
    index = end + 1;
  }

  return out;
}

export function legacyCss(css) {
  return lowerOklch(flattenLayers(css));
}

function selfTest() {
  const flatten = flattenLayers("@layer a, b;\n@layer utilities{.x{color:red}}");
  const lowered = lowerOklch(".x{color:oklch(0.7 0.1 200)}");
  const checks = [
    ["the layer statement goes", !flatten.includes("@layer")],
    ["the rules stay", flatten.includes(".x{color:red}")],
    ["oklch becomes rgb", lowered.startsWith(".x{color:rgb(")],
  ];
  let failed = 0;
  for (const [name, ok] of checks) {
    if (!ok) {
      console.error("self-test failed: " + name);
      failed += 1;
    }
  }
  return failed === 0;
}

if (process.argv.includes("--self-test")) {
  if (!selfTest()) process.exit(1);
  console.log("css legacy: self-test ok");
  process.exit(0);
}

let changed = 0;
for (const name of readdirSync(ASSETS)) {
  if (!name.endsWith(".css")) continue;
  const path = join(ASSETS, name);
  const before = readFileSync(path, "utf8");
  const after = legacyCss(before);
  if (after === before) continue;
  writeFileSync(path, after);
  changed += 1;
  const layers = (before.match(/@layer/g) ?? []).length;
  const colours = (before.match(/oklch\(/g) ?? []).length;
  console.log(`${name}: ${layers} @layer, ${colours} oklch -> lowered`);
}
if (changed === 0) console.log("css legacy: nothing to lower");
