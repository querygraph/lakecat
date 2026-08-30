import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const [, , inputArg, outputArg, diagramDirArg] = process.argv;
if (!inputArg || !outputArg || !diagramDirArg) {
  throw new Error("usage: prepare-paper.mjs <input.md> <output.md> <diagram-dir>");
}

const EXPECTED_DIAGRAMS = 5;
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

if (index !== EXPECTED_DIAGRAMS) {
  throw new Error(`expected ${EXPECTED_DIAGRAMS} diagrams, found ${index}`);
}
writeFileSync(outputArg, rendered);
