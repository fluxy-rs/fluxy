// Verify every relative Markdown link in README and docs/*.md resolves.
// Anchors and external URLs are skipped; only file targets are checked.
import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const files = [
  "README.md",
  ...fs
    .readdirSync("docs")
    .filter((f) => f.endsWith(".md"))
    .map((f) => path.join("docs", f)),
];
const linkPattern = /\[[^\]]*\]\(([^)\s]+)\)/g;
const problems = [];
let checked = 0;

for (const file of files) {
  const text = fs.readFileSync(file, "utf8");
  let match;
  while ((match = linkPattern.exec(text)) !== null) {
    const target = match[1];
    if (/^(https?:|mailto:|#)/.test(target)) continue;
    const withoutAnchor = target.split("#")[0];
    if (!withoutAnchor) continue;
    const resolved = path.resolve(path.dirname(file), withoutAnchor);
    checked += 1;
    if (!fs.existsSync(resolved)) {
      problems.push(`${file} -> ${target}`);
    }
  }
}

console.log(`files=${files.length} links=${checked} broken=${problems.length}`);
if (problems.length > 0) {
  for (const problem of problems) console.log("  broken: " + problem);
  process.exit(1);
}
