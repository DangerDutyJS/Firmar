// Genera public/third-party-licenses.txt con las licencias de todo lo que se
// distribuye dentro de Firmar Vecipp: crates de Rust enlazadas en el ejecutable,
// paquetes npm de producción, la tipografía y los recursos de pdf.js.
// Las licencias con texto idéntico se agrupan para no repetirlas.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const LICENSE_FILE = /^(licen[cs]e|copying|notice|unlicense)([-._].*)?$/i;

function licenseTexts(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((f) => LICENSE_FILE.test(f))
    .sort()
    .map((f) => readFileSync(join(dir, f), "utf8").replace(/\r\n/g, "\n").trim())
    .filter(Boolean);
}

function cargoBinary() {
  const local = join(homedir(), ".cargo", "bin", process.platform === "win32" ? "cargo.exe" : "cargo");
  return process.env.CARGO || (existsSync(local) ? local : "cargo");
}

/** Crates enlazadas en el binario de Windows (dependencias normales, sin build ni dev). */
function rustPackages() {
  const metadata = JSON.parse(
    execFileSync(
      cargoBinary(),
      ["metadata", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc", "--manifest-path", join(root, "src-tauri", "Cargo.toml")],
      { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 },
    ),
  );
  const packages = new Map(metadata.packages.map((p) => [p.id, p]));
  const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]));
  const rootId = metadata.resolve.root;
  const seen = new Set();
  const stack = [rootId];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
    }
  }
  seen.delete(rootId);
  return [...seen].map((id) => {
    const p = packages.get(id);
    return {
      name: p.name,
      version: p.version,
      license: p.license ?? "ver texto",
      url: p.repository ?? "",
      texts: licenseTexts(dirname(p.manifest_path)),
    };
  });
}

/** Paquetes npm de producción, siguiendo la resolución de node_modules. */
function npmPackages() {
  const result = new Map();
  const visit = (name, fromDir) => {
    let dir = fromDir;
    let pkgDir = null;
    while (true) {
      const candidate = join(dir, "node_modules", name);
      if (existsSync(join(candidate, "package.json"))) {
        pkgDir = candidate;
        break;
      }
      const parent = dirname(dir);
      if (parent === dir) return;
      dir = parent;
    }
    const pkg = JSON.parse(readFileSync(join(pkgDir, "package.json"), "utf8"));
    const key = `${pkg.name}@${pkg.version}`;
    if (result.has(key)) return;
    const repo = typeof pkg.repository === "string" ? pkg.repository : (pkg.repository?.url ?? "");
    result.set(key, {
      name: pkg.name,
      version: pkg.version,
      license: typeof pkg.license === "string" ? pkg.license : "ver texto",
      url: repo,
      texts: licenseTexts(pkgDir),
    });
    for (const dep of Object.keys(pkg.dependencies ?? {})) visit(dep, pkgDir);
  };
  const app = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  for (const dep of Object.keys(app.dependencies ?? {})) visit(dep, root);
  return [...result.values()];
}

function assets() {
  const pdfjs = join(root, "node_modules", "pdfjs-dist");
  const file = (p) => (existsSync(p) ? [readFileSync(p, "utf8").replace(/\r\n/g, "\n").trim()] : []);
  return [
    {
      name: "Liberation Sans (tipografía del texto bajo la firma)",
      version: "2.x",
      license: "OFL-1.1",
      url: "https://github.com/liberationfonts/liberation-fonts",
      texts: file(join(root, "src-tauri", "assets", "fonts", "LICENSE_LIBERATION")),
    },
    {
      name: "Fuentes estándar de pdf.js (Foxit)",
      version: "",
      license: "BSD",
      url: "https://github.com/mozilla/pdf.js",
      texts: file(join(pdfjs, "standard_fonts", "LICENSE_FOXIT")),
    },
    {
      name: "CMaps de Adobe (pdf.js)",
      version: "",
      license: "BSD-3-Clause",
      url: "https://github.com/adobe-type-tools/cmap-resources",
      texts: licenseTexts(join(pdfjs, "cmaps")),
    },
  ];
}

const sections = [
  ["Componentes de Rust", rustPackages()],
  ["Componentes de JavaScript", npmPackages()],
  ["Tipografías y recursos", assets()],
];

const groups = new Map();
let total = 0;
for (const [, items] of sections) {
  for (const item of items) {
    total++;
    const text = item.texts.length ? item.texts.join("\n\n---\n\n") : `Licencia declarada: ${item.license}\n(El paquete no incluye archivo de licencia; ver ${item.url || "su registro"}.)`;
    const hash = createHash("sha256").update(text).digest("hex");
    if (!groups.has(hash)) groups.set(hash, { text, items: [] });
    groups.get(hash).items.push(item);
  }
}

const lines = [
  "FIRMAR VECIPP — AVISOS Y LICENCIAS DE TERCEROS",
  "=".repeat(60),
  "",
  `Firmar Vecipp incluye ${total} componentes de terceros. Agradecemos a sus autores.`,
  "Cada bloque indica los componentes que comparten el mismo texto de licencia.",
  "",
];
for (const [title, items] of sections) {
  lines.push(`${title}: ${items.length}`);
}
lines.push("");

const ordered = [...groups.values()].sort((a, b) => b.items.length - a.items.length);
for (const group of ordered) {
  lines.push("=".repeat(60));
  for (const i of group.items.sort((a, b) => a.name.localeCompare(b.name))) {
    lines.push(`* ${i.name}${i.version ? ` ${i.version}` : ""} — ${i.license}${i.url ? ` — ${i.url}` : ""}`);
  }
  lines.push("-".repeat(60), group.text, "");
}

const out = join(root, "public", "third-party-licenses.txt");
writeFileSync(out, lines.join("\n"), "utf8");
console.log(`Licencias de ${total} componentes (${groups.size} textos distintos) → public/third-party-licenses.txt`);
