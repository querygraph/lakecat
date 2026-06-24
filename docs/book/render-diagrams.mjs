// Preprocess the LakeCat book: render fenced ```mermaid blocks to PNG via mmdc
// and rewrite them as image references, so pandoc/typst (which do not render
// Mermaid) emit real diagrams. Tracked PNG/MMD sources land in docs/book/diagrams.
//
// Usage: node render-diagrams.mjs <input.md> <output.md> <diagram-dir>
import { readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";

const [, , inputArg, outputArg, diagramDirArg] = process.argv;
if (!inputArg || !outputArg || !diagramDirArg) {
  console.error("usage: render-diagrams.mjs <input.md> <output.md> <diagram-dir>");
  process.exit(2);
}

const root = path.resolve(import.meta.dirname);
const input = path.resolve(inputArg);
const output = path.resolve(outputArg);
const diagramDir = path.resolve(diagramDirArg);
const puppeteerConfig = path.join(root, "puppeteer-config.json");

rmSync(diagramDir, { recursive: true, force: true });
mkdirSync(diagramDir, { recursive: true });

const renderMermaid = (src, png) => {
  const result = spawnSync(
    "mmdc",
    ["-i", src, "-o", png, "-b", "white", "-p", puppeteerConfig, "-s", "2"],
    { stdio: "inherit" },
  );
  if (result.status !== 0) throw new Error(`mmdc failed for ${src}`);
};

const source = readFileSync(input, "utf8");
let index = 0;
const rendered = source.replace(/```mermaid\n([\s\S]*?)\n```/g, (_m, body) => {
  index += 1;
  const stem = `diagram-${String(index).padStart(2, "0")}`;
  const mmd = path.join(diagramDir, `${stem}.mmd`);
  const png = path.join(diagramDir, `${stem}.png`);
  writeFileSync(mmd, `${body.trim()}\n`);
  renderMermaid(mmd, png);
  return `![Diagram ${index}](diagrams/${stem}.png)`;
});

writeFileSync(output, rendered);
console.log(`Rendered ${index} Mermaid diagram(s) -> ${diagramDir}`);
