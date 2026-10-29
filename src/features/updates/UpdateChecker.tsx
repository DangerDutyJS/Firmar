import { useCallback, useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { create } from "zustand";

const AUTO_KEY = "firmar.autoUpdate";

function loadAuto(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) !== "false";
  } catch {
    return true;
  }
}

type Phase =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "upToDate" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; update: Update; done: number; total: number | null }
  | { kind: "installing"; update: Update }
  | { kind: "error"; message: string };

interface UpdateState {
  auto: boolean;
  phase: Phase;
  /** El usuario pulsó «Más tarde» en esta sesión. */
  dismissed: boolean;
  setAuto: (auto: boolean) => void;
  checkNow: (manual: boolean) => Promise<void>;
  install: () => Promise<void>;
  dismiss: () => void;
}

/**
 * Actualizaciones firmadas desde GitHub Releases (`latest.json`). La firma
 * minisign se verifica con la clave pública de `tauri.conf.json` antes de
 * instalar: un instalador manipulado se rechaza.
 */
export const useUpdates = create<UpdateState>((set, get) => ({
  auto: loadAuto(),
  phase: { kind: "idle" },
  dismissed: false,

  setAuto: (auto) => {
    try {
      localStorage.setItem(AUTO_KEY, String(auto));
    } catch {
      // Sin almacenamiento local: vale solo para esta sesión.
    }
    set({ auto });
  },

  checkNow: async (manual) => {
    const busy = get().phase.kind;
    if (busy === "checking" || busy === "downloading" || busy === "installing") return;
    set({ phase: { kind: "checking" }, dismissed: manual ? false : get().dismissed });
    try {
      const update = await check({ timeout: 15000 });
      set({ phase: update ? { kind: "available", update } : { kind: "upToDate" } });
    } catch (e) {
      // Sin conexión al arrancar no es un error que haya que mostrar.
      set({ phase: manual ? { kind: "error", message: String(e) } : { kind: "idle" } });
    }
  },

  install: async () => {
    const phase = get().phase;
    if (phase.kind !== "available") return;
    const { update } = phase;
    let done = 0;
    let total: number | null = null;
    set({ phase: { kind: "downloading", update, done, total } });
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? null;
        else if (event.event === "Progress") done += event.data.chunkLength;
        else if (event.event === "Finished") {
          set({ phase: { kind: "installing", update } });
          return;
        }
        set({ phase: { kind: "downloading", update, done, total } });
      });
      // En Windows el instalador cierra la app; en otros sistemas, reiniciamos.
      await relaunch();
    } catch (e) {
      set({ phase: { kind: "error", message: String(e) } });
    }
  },

  dismiss: () => set({ dismissed: true, phase: { kind: "idle" } }),
}));

const mb = (bytes: number) => `${(bytes / 1024 / 1024).toFixed(1)} MB`;

/** Busca al arrancar (si está activado) y muestra el aviso cuando hay versión nueva. */
export function UpdateChecker() {
  const { auto, phase, dismissed, checkNow, install, dismiss } = useUpdates();
  const [notesOpen, setNotesOpen] = useState(false);

  useEffect(() => {
    // Solo al arrancar: los cambios del ajuste no relanzan la búsqueda.
    if (auto) checkNow(false);
  }, []);

  const showDialog =
    !dismissed && (phase.kind === "available" || phase.kind === "downloading" || phase.kind === "installing");
  const update = "update" in phase ? phase.update : null;
  const onLater = useCallback(() => dismiss(), [dismiss]);

  if (!showDialog || !update) {
    return phase.kind === "error" && !dismissed ? (
      <div className="fixed right-5 bottom-5 z-50 w-96 rounded-xl border border-red-500/60 bg-ink-900 px-4 py-3 text-sm text-slate-200 shadow-2xl">
        <p className="font-medium">No se pudo actualizar</p>
        <p className="mt-1 text-xs break-words text-slate-400">{phase.message}</p>
        <button type="button" onClick={onLater} className="mt-2 text-xs text-slate-300 hover:text-white">
          Cerrar
        </button>
      </div>
    ) : null;
  }

  const progress =
    phase.kind === "downloading" && phase.total ? Math.min(100, Math.round((phase.done / phase.total) * 100)) : null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-ink-950/70 p-6 backdrop-blur-sm">
      <div className="w-full max-w-md space-y-4 rounded-2xl border border-ink-700 bg-ink-900 p-6 shadow-2xl">
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-500 text-lg font-bold text-white">↑</div>
          <div>
            <h2 className="font-semibold text-white">Hay una nueva versión disponible</h2>
            <p className="text-sm text-slate-400">
              Firmar Vecipp {update.version} · tienes la {update.currentVersion}
            </p>
          </div>
        </div>

        {update.body && (
          <div className="text-sm">
            <button type="button" onClick={() => setNotesOpen((v) => !v)} className="text-brand-400 hover:underline">
              {notesOpen ? "Ocultar novedades" : "Ver novedades"}
            </button>
            {notesOpen && (
              <pre className="mt-2 max-h-56 overflow-y-auto rounded-lg bg-ink-950/60 p-3 font-sans text-xs whitespace-pre-wrap text-slate-300 select-text">
                {update.body}
              </pre>
            )}
          </div>
        )}

        {phase.kind === "available" && (
          <>
            <p className="text-sm text-slate-400">
              Tus firmas, historial y documentos se conservan. La app se cerrará, se instalará la nueva versión y se
              abrirá de nuevo.
            </p>
            <div className="flex justify-end gap-2">
              <button type="button" onClick={onLater} className="px-3 py-2 text-sm text-slate-400 hover:text-slate-200">
                Más tarde
              </button>
              <button
                type="button"
                onClick={install}
                className="rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white hover:bg-brand-600"
              >
                Actualizar ahora
              </button>
            </div>
          </>
        )}

        {phase.kind === "downloading" && (
          <div className="space-y-1">
            <div className="h-2 overflow-hidden rounded-full bg-ink-800">
              <div
                className={`h-full bg-brand-500 transition-all ${progress === null ? "w-1/3 animate-pulse" : ""}`}
                style={progress === null ? undefined : { width: `${progress}%` }}
              />
            </div>
            <p className="text-xs text-slate-400">
              Descargando… {mb(phase.done)}
              {phase.total ? ` de ${mb(phase.total)}` : ""}
            </p>
          </div>
        )}

        {phase.kind === "installing" && (
          <p className="flex items-center gap-2 text-sm text-slate-300">
            <span className="h-4 w-4 animate-spin rounded-full border-2 border-brand-400 border-t-transparent" />
            Instalando… la app se reiniciará en unos segundos.
          </p>
        )}
      </div>
    </div>
  );
}

/** Ajustes de actualización para «Acerca de». */
export function UpdateSettings() {
  const { auto, phase, setAuto, checkNow } = useUpdates();
  const label: Record<Phase["kind"], string> = {
    idle: "",
    checking: "Buscando…",
    upToDate: "Tienes la última versión.",
    available: "Hay una versión nueva.",
    downloading: "Descargando…",
    installing: "Instalando…",
    error: "No se pudo comprobar. Revisa tu conexión.",
  };
  return (
    <div className="space-y-2 rounded-xl border border-ink-700 p-4">
      <p className="font-medium text-white">Actualizaciones</p>
      <label className="flex items-center gap-2">
        <input type="checkbox" checked={auto} onChange={(e) => setAuto(e.target.checked)} className="accent-brand-500" />
        Buscar actualizaciones al abrir la app
      </label>
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={() => checkNow(true)}
          disabled={phase.kind === "checking"}
          className="rounded-lg border border-ink-700 px-3 py-1.5 text-sm text-slate-200 hover:bg-ink-800 disabled:opacity-40"
        >
          Buscar ahora
        </button>
        <span className="text-xs text-slate-400">{label[phase.kind]}</span>
      </div>
      <p className="text-xs text-slate-500">
        Solo se consulta GitHub para saber si hay una versión nueva; no se envían tus documentos ni tus firmas.
      </p>
    </div>
  );
}
