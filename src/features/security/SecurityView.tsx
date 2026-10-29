import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api, errorMessage, type BackupSummary } from "../../lib/ipc";
import { useFirmarStore } from "../../store/useFirmarStore";
import { WipeCard } from "../../legal/LegalViews";

const MIN_PASSWORD = 10;
const BACKUP_EXTENSION = "firmarbackup";

/** Copia de seguridad de la identidad: claves, firmas y originales de la bóveda. */
export function SecurityView() {
  return (
    <div className="mx-auto flex h-full max-w-4xl flex-col gap-6 overflow-y-auto p-6">
      <header>
        <h2 className="text-lg font-semibold text-white">Copia de seguridad</h2>
        <p className="mt-1 max-w-2xl text-sm text-slate-400">
          Tu identidad de firma vive en el llavero de este equipo. Si reinstalas Windows o cambias de computadora sin
          una copia, perderás tus firmas guardadas y no podrás volver a gestionar los documentos que firmaste.
        </p>
      </header>
      <div className="grid gap-6 lg:grid-cols-2">
        <ExportCard />
        <RestoreCard />
        <WipeCard />
      </div>
    </div>
  );
}

function ExportCard() {
  const notify = useFirmarStore((s) => s.notify);
  const setBusy = useFirmarStore((s) => s.setBusy);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");

  const tooShort = password.length > 0 && password.length < MIN_PASSWORD;
  const mismatch = confirm.length > 0 && confirm !== password;
  const ready = password.length >= MIN_PASSWORD && confirm === password;

  const exportBackup = async () => {
    const today = new Date().toISOString().slice(0, 10);
    const path = await save({
      defaultPath: `firmar-respaldo-${today}.${BACKUP_EXTENSION}`,
      filters: [{ name: "Copia de Firmar Vecipp", extensions: [BACKUP_EXTENSION] }],
    });
    if (!path) return;
    setBusy("Cifrando copia de seguridad…");
    try {
      await api.backupExport(path, password);
      setPassword("");
      setConfirm("");
      notify("success", "Copia guardada. Guárdala en un USB o en la nube y no olvides la contraseña.", [
        { label: "Mostrar en carpeta", run: () => api.revealPath(path).catch(() => undefined) },
      ]);
    } catch (e) {
      notify("error", errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Card title="Crear copia" description="Guarda tus firmas, tu identidad y los originales en un archivo cifrado con contraseña.">
      <PasswordInput label="Contraseña" value={password} onChange={setPassword} autoComplete="new-password" />
      {tooShort && <Hint tone="warn">Usa al menos {MIN_PASSWORD} caracteres.</Hint>}
      <PasswordInput label="Repite la contraseña" value={confirm} onChange={setConfirm} autoComplete="new-password" />
      {mismatch && <Hint tone="warn">Las contraseñas no coinciden.</Hint>}
      <Hint>Si olvidas esta contraseña, nadie podrá abrir la copia: ni siquiera Firmar Vecipp.</Hint>
      <button
        type="button"
        disabled={!ready}
        onClick={exportBackup}
        className="mt-2 rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white hover:bg-brand-600 disabled:cursor-not-allowed disabled:opacity-40"
      >
        Guardar copia…
      </button>
    </Card>
  );
}

function RestoreCard() {
  const notify = useFirmarStore((s) => s.notify);
  const setBusy = useFirmarStore((s) => s.setBusy);
  const identity = useFirmarStore((s) => s.identity);
  const bootstrap = useFirmarStore((s) => s.bootstrap);
  const [path, setPath] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [summary, setSummary] = useState<BackupSummary | null>(null);

  const reset = () => {
    setPath(null);
    setPassword("");
    setSummary(null);
  };

  const pick = async () => {
    const selected = await open({ multiple: false, filters: [{ name: "Copia de Firmar Vecipp", extensions: [BACKUP_EXTENSION] }] });
    if (typeof selected === "string") {
      setPath(selected);
      setSummary(null);
    }
  };

  const check = async () => {
    if (!path) return;
    setBusy("Comprobando la copia…");
    try {
      setSummary(await api.backupInspect(path, password));
    } catch (e) {
      notify("error", errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  const restore = async () => {
    if (!path) return;
    setBusy("Restaurando identidad…");
    try {
      await api.backupRestore(path, password);
      await bootstrap();
      useFirmarStore.getState().setDocument(null);
      reset();
      notify("success", "Identidad restaurada. Ya puedes gestionar tus documentos firmados.");
    } catch (e) {
      notify("error", errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  const sameIdentity = summary && identity && summary.fingerprint === identity.fingerprint;

  return (
    <Card title="Restaurar copia" description="Recupera tu identidad en este equipo. Las firmas que ya tengas aquí se conservan.">
      <button
        type="button"
        onClick={pick}
        className="truncate rounded-lg border border-dashed border-ink-700 px-3 py-2 text-left text-sm text-slate-300 hover:border-brand-500"
      >
        {path ? path.split(/[\\/]/).pop() : "Elegir archivo de copia…"}
      </button>
      {path && !summary && (
        <>
          <PasswordInput label="Contraseña de la copia" value={password} onChange={setPassword} autoComplete="current-password" />
          <button
            type="button"
            disabled={password.length === 0}
            onClick={check}
            className="mt-2 rounded-lg border border-ink-700 px-4 py-2 text-sm text-slate-200 hover:bg-ink-800 disabled:opacity-40"
          >
            Comprobar
          </button>
        </>
      )}
      {summary && (
        <div className="space-y-3 rounded-lg border border-ink-700 bg-ink-800/60 p-3 text-sm">
          <p className="text-slate-200">
            La copia contiene {summary.profiles} firma(s) y {summary.documents} documento(s).
          </p>
          {sameIdentity ? (
            <Hint>Es la misma identidad que ya usa este equipo: solo se añadirá lo que falte.</Hint>
          ) : (
            <Hint tone="warn">
              Este equipo pasará a usar la identidad de la copia. Los documentos firmados aquí con la identidad actual
              (<span className="font-mono">{identity?.fingerprint}</span>) seguirán siendo válidos, pero ya no podrás
              re-posicionar su firma. Si los tienes, crea antes una copia de la identidad actual.
            </Hint>
          )}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={restore}
              className="rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white hover:bg-brand-600"
            >
              Restaurar
            </button>
            <button type="button" onClick={reset} className="rounded-lg px-3 py-2 text-sm text-slate-400 hover:text-slate-200">
              Cancelar
            </button>
          </div>
        </div>
      )}
    </Card>
  );
}

function Card({ title, description, children }: { title: string; description: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-3 rounded-2xl border border-ink-800 bg-ink-900 p-5">
      <div>
        <h3 className="font-semibold text-white">{title}</h3>
        <p className="mt-1 text-sm text-slate-400">{description}</p>
      </div>
      {children}
    </section>
  );
}

function PasswordInput({
  label,
  value,
  onChange,
  autoComplete,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  autoComplete: string;
}) {
  return (
    <label className="flex flex-col gap-1 text-sm text-slate-400">
      {label}
      <input
        type="password"
        value={value}
        autoComplete={autoComplete}
        onChange={(e) => onChange(e.target.value)}
        className="rounded-lg border border-ink-700 bg-ink-800 px-3 py-2 text-white outline-none focus:border-brand-500"
      />
    </label>
  );
}

function Hint({ tone = "info", children }: { tone?: "info" | "warn"; children: React.ReactNode }) {
  return <p className={`text-xs ${tone === "warn" ? "text-amber-300" : "text-slate-500"}`}>{children}</p>;
}
