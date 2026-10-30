#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const penna = join(here, "../..");
const out = resolve(process.argv[2] ?? join(penna, "target/icon-sheet/penna-icons.html"));
const searched = process.argv.length > 3 ? process.argv.slice(3).map((dir) => resolve(dir)) : [join(penna, "src")];

const sets = [JSON.parse(readFileSync(join(penna, "src/icon/solar.json"), "utf8"))];
const set = sets[0];
const renderer = readFileSync(join(here, "glyph.js"), "utf8");
const template = readFileSync(join(here, "sheet.html"), "utf8");
const lilex = readFileSync(join(here, "../fonts/lilex/Lilex-Regular.ttf")).toString("base64");

const usage = {};
for (const icon of set.icons) {
  let files = 0;
  try {
    files = execFileSync("rg", ["-l", `IconName::${icon.variant}\\b`, ...searched, "-g", "*.rs",
      "-g", "!**/src/icon/**", "-g", "!**/storybook/**"], { encoding: "utf8" })
      .split("\n").filter(Boolean).length;
  } catch {
    files = 0;
  }
  usage[icon.variant] = { files };
}

const page = template
  .replace("/*LILEX*/", lilex)
  .replace("/*RENDERER*/", renderer)
  .replace("/*SETS*/", JSON.stringify(sets))
  .replace("/*USAGE*/", JSON.stringify(usage));
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, page);
console.log(`${set.icons.length} icons -> ${out} (${(page.length / 1024).toFixed(0)} KB)`);
