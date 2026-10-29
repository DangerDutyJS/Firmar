import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorMessage, type HistoryEntry, type SignatureStatus } from "../../lib/ipc";
import { useFirmarStore } from "../../store/useFirmarStore";

type Check = "checking" | "missing" | SignatureStatus["state"];

const BADGES: Record<Check, { label: string; className: string }> = {
  checking: { label: "Verificando…", className: "bg-ink-700 text-slate-300" },
  valid: { label: "Íntegro", className: "bg-emerald-500/15 text-emerald-300" },
  modifiedAfterSigning: { label: "Con cambios añadidos", className: "bg-amber-500/15 text-amber-300" },
  tampered: { label: "Alterado", className: "bg-red-500/15 text-red-300" },
  unsigned: { label: "Ya no tiene firma", className: "bg-red-500/15 text-red-300" },
  missing: { label: "No encontrado", className: "bg-ink-700 text-slate-400" },
};

/** Documentos firmados en este equipo, con verificación de su estado actual. */
export function HistoryView() {
  const notify = useFirmarStore((s) => s.notify);
  const requestOpen = useFirmarStore((s) => s.requestOpen);
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null);
  const [checks, setChecks] = useState<Record<string, Check>>({});
  const [query, setQuery] = useState("");

  const load = useCallback(async () => {
    try {
      setEntries(await api.historyList());
    } catch (e) {
      notify("error", errorMessage(e));
    }
  }, [notify]);

  useEffect(() => {
    load();
  }, [load]);

  const verify = useCallback(async (entry: HistoryEntry) => {
    setChecks((c) => ({ ...c, [entry.documentId]: "checking" }));
    let result: Check;
    try {
      result = (await api.documentInspect(entry.outputPath)).status.state;
    } catch {
      result = "missing";
    }
    setChecks((c) => ({ ...c, [entry.documentId]: result }));
  }, []);

  const verifyAll = async () => {
    for (const e of filtered) await verify(e);
  };

  const forget = async (entry: HistoryEntry) => {
    try {
      await api.historyForget(entry.documentId);
      await load();
    } catch (e) {
      notify("error", errorMessage(e));
    }
  };

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (entries ?? []).filter((e) => !q || e.fileName.toLowerCase().includes(q) || e.outputPath.toLowerCase().includes(q));
  }, [entries, query]);

  return (
    <div className="mx-auto flex h-full max-w-5xl flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center gap-3">
        <div className="mr-auto">
          <h2 className="text-lg font-semibold text-white">Historial</h2>
          <p className="text-sm text-slate-400">Documentos firmados en este equipo. Se guarda cifrado en tu bóveda.</p>
        </div>
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Buscar por nombre…"
          className="w-56 rounded-lg border border-ink-700 bg-ink-800 px-3 py-2 text-sm text-white outline-none focus:border-brand-500"
        />
        <button
          type="button"
          onClick={verifyAll}
          disabled={filtered.length === 0}
          className="rounded-lg border border-ink-700 px-3 py-2 text-sm text-slate-200 hover:bg-ink-800 disabled:opacity-40"
        >
          Verificar todos
        </button>
      </header>

      {entries === null ? (
        <p className="text-sm text-slate-400">Cargando…</p>
      ) : filtered.length === 0 ? (
        <div className="flex flex-1 items-center justify-center rounded-2xl border border-dashed border-ink-700 text-sm text-slate-400">
          {entries.length === 0 ? "Aún no has firmado documentos." : "Ningún documento coincide con la búsqueda."}
        </div>
      ) : (
        <ul className="min-h-0 flex-1 divide-y divide-ink-800 overflow-y-auto rounded-2xl border border-ink-800 bg-ink-900">
          {filtered.map((e) => {
            const check = checks[e.documentId];
            return (
              <li key={e.documentId} className="group flex items-center gap-4 px-4 py-3 text-sm">
                <span className="w-12 shrink-0 rounded bg-ink-800 py-1 text-center text-[10px] font-bold text-slate-400 uppercase">
                  {e.format === "jpeg" ? "jpg" : e.format}
                </span>
                <div className="min-w-0 flex-1">
                  <p className="truncate font-medium text-slate-100">{e.fileName}</p>
                  <p className="truncate text-xs text-slate-500" title={e.outputPath}>
                    {new Date(e.signedAt).toLocaleString()} · {e.signatures} firma(s) · {e.outputPath}
                  </p>
                </div>
                {check ? (
                  <span className={`shrink-0 rounded-full px-2.5 py-1 text-xs ${BADGES[check].className}`}>{BADGES[check].label}</span>
                ) : (
                  <button type="button" onClick={() => verify(e)} className="shrink-0 text-xs text-brand-400 hover:underline">
                    Verificar
                  </button>
                )}
                <div className="flex shrink-0 gap-3 text-xs">
                  <button type="button" onClick={() => requestOpen(e.outputPath)} className="text-slate-300 hover:text-white">
                    Abrir en Firmar Vecipp
                  </button>
                  <button
                    type="button"
                    onClick={() => api.revealPath(e.outputPath).catch(() => notify("error", "El archivo ya no está en esa ubicación"))}
                    className="text-slate-400 hover:text-slate-200"
                  >
                    Carpeta
                  </button>
                  <button
                    type="button"
                    onClick={() => forget(e)}
                    title="Quitar del historial (no borra el archivo)"
                    className="text-slate-600 opacity-0 group-hover:opacity-100 hover:text-red-400"
                  >
                    Quitar
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
