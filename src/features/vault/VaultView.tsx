import { useCallback, useRef, useState } from "react";
import { errorMessage } from "../../lib/ipc";
import { useFirmarStore } from "../../store/useFirmarStore";
import { ImportSignature } from "./ImportSignature";
import { SignaturePad, type SignaturePadHandle } from "./SignaturePad";

const COLORS = [
  { label: "Negro", value: "#111827" },
  { label: "Azul", value: "#1d35a0" },
];

export function VaultView() {
  const profiles = useFirmarStore((s) => s.profiles);
  const activeProfileId = useFirmarStore((s) => s.activeProfileId);
  const setActiveProfile = useFirmarStore((s) => s.setActiveProfile);
  const saveProfile = useFirmarStore((s) => s.saveProfile);
  const deleteProfile = useFirmarStore((s) => s.deleteProfile);
  const notify = useFirmarStore((s) => s.notify);
  const setView = useFirmarStore((s) => s.setView);

  const [tab, setTab] = useState<"draw" | "import">("draw");
  const [name, setName] = useState("Mi firma");
  const [color, setColor] = useState(COLORS[0].value);
  const [thickness, setThickness] = useState(3);
  const [padEmpty, setPadEmpty] = useState(true);
  const [imported, setImported] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const padRef = useRef<SignaturePadHandle>(null);
  const onImported = useCallback((url: string | null) => setImported(url), []);

  const canSave = name.trim().length > 0 && (tab === "draw" ? !padEmpty : imported !== null);

  const save = async () => {
    const png = tab === "draw" ? padRef.current?.exportPng() : imported;
    if (!png) return;
    setSaving(true);
    try {
      await saveProfile(name.trim(), png);
      padRef.current?.clear();
      notify("success", "Firma guardada en la bóveda cifrada");
    } catch (e) {
      notify("error", errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const remove = async (id: string) => {
    setConfirmDelete(null);
    try {
      await deleteProfile(id);
      notify("info", "Firma eliminada de la bóveda");
    } catch (e) {
      notify("error", errorMessage(e));
    }
  };

  return (
    <div className="flex h-full min-h-0 gap-6 p-6">
      <section className="flex min-w-0 flex-1 flex-col gap-4 rounded-2xl border border-ink-800 bg-ink-900 p-5">
        <header className="flex items-center justify-between">
          <div>
            <h2 className="text-lg font-semibold text-white">Nueva firma</h2>
            <p className="text-sm text-slate-400">Se cifra con AES-256-GCM y la clave vive en el llavero del sistema.</p>
          </div>
          <div className="flex rounded-lg bg-ink-800 p-1 text-sm">
            {(["draw", "import"] as const).map((t) => (
              <button
                key={t}
                type="button"
                onClick={() => setTab(t)}
                className={`rounded-md px-3 py-1.5 ${tab === t ? "bg-ink-700 text-white" : "text-slate-400 hover:text-slate-200"}`}
              >
                {t === "draw" ? "Dibujar" : "Importar imagen"}
              </button>
            ))}
          </div>
        </header>

        <div className="min-h-0 flex-1">
          {tab === "draw" ? (
            <SignaturePad ref={padRef} color={color} thickness={thickness} onChange={setPadEmpty} />
          ) : (
            <ImportSignature onResult={onImported} />
          )}
        </div>

        <footer className="flex flex-wrap items-center gap-4">
          {tab === "draw" && (
            <>
              <div className="flex items-center gap-2">
                {COLORS.map((c) => (
                  <button
                    key={c.value}
                    type="button"
                    title={c.label}
                    onClick={() => setColor(c.value)}
                    className={`h-7 w-7 rounded-full ring-2 ring-offset-2 ring-offset-ink-900 ${color === c.value ? "ring-brand-400" : "ring-transparent"}`}
                    style={{ backgroundColor: c.value }}
                  />
                ))}
              </div>
              <label className="flex items-center gap-2 text-sm text-slate-400">
                Grosor
                <input
                  type="range"
                  min={1.5}
                  max={7}
                  step={0.5}
                  value={thickness}
                  onChange={(e) => setThickness(Number(e.target.value))}
                  className="accent-brand-500"
                />
              </label>
              <button
                type="button"
                onClick={() => padRef.current?.clear()}
                className="text-sm text-slate-400 hover:text-slate-200"
              >
                Limpiar
              </button>
            </>
          )}
          <div className="ml-auto flex items-center gap-3">
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={80}
              placeholder="Nombre de la firma"
              className="w-48 rounded-lg border border-ink-700 bg-ink-800 px-3 py-2 text-sm text-white outline-none focus:border-brand-500"
            />
            <button
              type="button"
              disabled={!canSave || saving}
              onClick={save}
              className="rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white hover:bg-brand-600 disabled:cursor-not-allowed disabled:opacity-40"
            >
              {saving ? "Guardando…" : "Guardar en la bóveda"}
            </button>
          </div>
        </footer>
      </section>

      <aside className="flex w-80 shrink-0 flex-col gap-3 rounded-2xl border border-ink-800 bg-ink-900 p-5">
        <h2 className="text-lg font-semibold text-white">Bóveda</h2>
        {profiles.length === 0 && (
          <p className="text-sm text-slate-400">Aún no tienes firmas guardadas. Dibuja o importa la primera.</p>
        )}
        <ul className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pr-1">
          {profiles.map((p) => (
            <li
              key={p.id}
              className={`group rounded-xl border p-3 transition ${p.id === activeProfileId ? "border-brand-500 bg-brand-500/10" : "border-ink-700 hover:border-ink-700/80 hover:bg-ink-800"}`}
            >
              <button type="button" className="w-full text-left" onClick={() => setActiveProfile(p.id)}>
                <div className="checkerboard flex h-20 items-center justify-center rounded-lg p-2">
                  <img src={p.dataUrl} alt={p.name} className="max-h-full max-w-full object-contain" />
                </div>
                <div className="mt-2 flex items-center justify-between text-sm">
                  <span className="truncate font-medium text-slate-100">{p.name}</span>
                  <span className="text-xs text-slate-500">{new Date(p.createdAt).toLocaleDateString()}</span>
                </div>
              </button>
              <div className="mt-2 flex justify-end gap-3 text-xs">
                {confirmDelete === p.id ? (
                  <>
                    <span className="text-slate-400">¿Eliminar?</span>
                    <button type="button" className="text-red-400 hover:text-red-300" onClick={() => remove(p.id)}>
                      Sí, eliminar
                    </button>
                    <button type="button" className="text-slate-400" onClick={() => setConfirmDelete(null)}>
                      Cancelar
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    className="text-slate-500 opacity-0 transition group-hover:opacity-100 hover:text-red-400"
                    onClick={() => setConfirmDelete(p.id)}
                  >
                    Eliminar
                  </button>
                )}
              </div>
            </li>
          ))}
        </ul>
        {profiles.length > 0 && (
          <button
            type="button"
            onClick={() => setView("signer")}
            className="rounded-lg border border-ink-700 px-4 py-2 text-sm text-slate-200 hover:bg-ink-800"
          >
            Ir a firmar documentos →
          </button>
        )}
      </aside>
    </div>
  );
}
