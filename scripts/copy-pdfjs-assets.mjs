// Copia los recursos de pdf.js a public/pdfjs para que el visor funcione sin
// red: fuentes estándar, CMaps (CJK), perfiles ICC y decodificadores wasm.
import { cpSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const source = join(root, "node_modules", "pdfjs-dist");
const target = join(root, "public", "pdfjs");

mkdirSync(target, { recursive: true });
for (const dir of ["cmaps", "standard_fonts", "iccs", "wasm"]) {
  const from = join(source, dir);
  if (existsSync(from)) cpSync(from, join(target, dir), { recursive: true });
}
