// Publica una versión de Firmar Vecipp en GitHub Releases con actualización automática.
//
//   npm run release -- 0.2.0 "Novedades de esta versión"
//   npm run release -- 0.2.0 ./notas.md
//
// 1. Sube la versión en package.json, tauri.conf.json y Cargo.toml.
// 2. Compila el instalador firmado con la clave de actualizaciones.
// 3. Genera latest.json (lo que consultan las copias instaladas).
// 4. Hace commit, crea la etiqueta vX.Y.Z, sube y crea el Release.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "DangerDutyJS/Firmar";
const ASSET_PREFIX = "Firmar-Vecipp";
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const keyPath = process.env.FIRMAR_UPDATER_KEY ?? join(homedir(), ".tauri", "firmar-vecipp.key");

const [version, notesArg = ""] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error('Uso: npm run release -- <versión X.Y.Z> ["notas" | archivo.md]');
  process.exit(1);
}
if (!existsSync(keyPath)) {
  console.error(`No se encontró la clave de actualizaciones en ${keyPath}.`);
  process.exit(1);
}
const notes = notesArg && existsSync(notesArg) ? readFileSync(notesArg, "utf8") : notesArg || `Firmar Vecipp ${version}`;
const tag = `v${version}`;

const run = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { cwd: root, stdio: "inherit", shell: process.platform === "win32", ...opts });
const output = (cmd, args) => execFileSync(cmd, args, { cwd: root, encoding: "utf8", shell: process.platform === "win32" }).trim();

if (output("git", ["status", "--porcelain"])) {
  console.error("Hay cambios sin confirmar. Haz commit antes de publicar.");
  process.exit(1);
}
if (output("git", ["tag", "--list", tag])) {
  console.error(`La etiqueta ${tag} ya existe.`);
  process.exit(1);
}

// 1. Versión en los tres manifiestos.
const editJson = (rel, fn) => {
  const path = join(root, rel);
  const data = JSON.parse(readFileSync(path, "utf8"));
  fn(data);
  writeFileSync(path, JSON.stringify(data, null, 2) + "\n");
};
editJson("package.json", (p) => (p.version = version));
editJson("src-tauri/tauri.conf.json", (c) => (c.version = version));
const cargoPath = join(root, "src-tauri", "Cargo.toml");
writeFileSync(cargoPath, readFileSync(cargoPath, "utf8").replace(/^version = ".*"$/m, `version = "${version}"`));

// 2. Instalador firmado (Cargo.lock se actualiza en la compilación).
run("npx", ["tauri", "build"], {
  env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY: keyPath, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "" },
});

const bundleDir = join(root, "src-tauri", "target", "release", "bundle", "nsis");
const installer = readdirSync(bundleDir).find((f) => f.endsWith(`_${version}_x64-setup.exe`));
if (!installer || !existsSync(join(bundleDir, `${installer}.sig`))) {
  console.error(`No se encontró el instalador firmado de ${version} en ${bundleDir}.`);
  process.exit(1);
}

// 3. Archivos del Release, sin espacios en el nombre (GitHub los cambiaría).
const outDir = join(root, "release", tag);
mkdirSync(outDir, { recursive: true });
const assetName = `${ASSET_PREFIX}_${version}_x64-setup.exe`;
copyFileSync(join(bundleDir, installer), join(outDir, assetName));
const signature = readFileSync(join(bundleDir, `${installer}.sig`), "utf8").trim();
writeFileSync(join(outDir, `${assetName}.sig`), signature);
writeFileSync(
  join(outDir, "latest.json"),
  JSON.stringify(
    {
      version,
      notes,
      pub_date: new Date().toISOString(),
      platforms: {
        "windows-x86_64": { signature, url: `https://github.com/${REPO}/releases/download/${tag}/${assetName}` },
      },
    },
    null,
    2,
  ),
);

// 4. Commit de la versión, etiqueta y Release.
run("git", ["add", "package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"]);
if (output("git", ["status", "--porcelain"])) run("git", ["commit", "-m", `Versión ${version}`]);
run("git", ["tag", "-a", tag, "-m", `Firmar Vecipp ${version}`]);
run("git", ["push", "origin", "HEAD", "--follow-tags"]);
writeFileSync(join(outDir, "NOTES.md"), notes);
run("gh", [
  "release", "create", tag,
  join(outDir, assetName), join(outDir, `${assetName}.sig`), join(outDir, "latest.json"),
  "--repo", REPO, "--title", `Firmar Vecipp ${tag}`, "--notes-file", join(outDir, "NOTES.md"),
]);
console.log(`\nPublicado ${tag}: https://github.com/${REPO}/releases/tag/${tag}`);
