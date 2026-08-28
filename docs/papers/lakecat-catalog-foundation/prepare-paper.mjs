import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const [, , inputArg, outputArg, diagramDirArg] = process.argv;
if (!inputArg || !outputArg || !diagramDirArg) {
  throw new Error("usage: prepare-paper.mjs <input.md> <output.md> <diagram-dir>");
}

const diagramDir = path.resolve(diagramDirArg);
let index = 0;
const rendered = readFileSync(inputArg, "utf8").replace(
  /```mermaid\n([\s\S]*?)\n```/g,
  (_match, body) => {
    index += 1;
    const stem = `diagram-${String(index).padStart(2, "0")}`;
    writeFileSync(path.join(diagramDir, `${stem}.mmd`), `${body.trim()}\n`);
    return `![Diagram ${index}](diagrams/${stem}.png)`;
  },
);

if (index !== 4) throw new Error(`expected 4 diagrams, found ${index}`);
writeFileSync(outputArg, rendered);

