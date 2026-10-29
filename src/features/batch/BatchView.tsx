import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, errorMessage, type BatchItem, type BatchProgress, type PageTarget } from "../../lib/ipc";
import { activeProfile, useFirmarStore } from "../../store/useFirmarStore";
import { CaptionPanel, useStamp } from "../signer/CaptionPanel";
import { PositionPicker } from "./PositionPicker";
import { DEFAULT_POSITION, DEFAULT_SIGNATURE_WIDTH } from "../signer/placement";

const PAGE_OPTIONS: { value: PageTarget; label: string; hint: string }[] = [
  { value: "last", label: "Última página", hint: "Lo habitual en contratos y actas" },
  { value: "first", label: "Primera página", hint: "Portadas y formularios" },
  { value: "all", label: "Todas las páginas", hint: "Rubricar hoja por hoja (en Word: primera y última)" },
];

const PRESETS = [
  { label: "Abajo izq.", x: 0.08, y: 0.8 },
  { label: "Abajo centro", x: 0.37, y: 0.8 },
  { label: "Abajo der.", x: 0.62, y: 0.8 },
];

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** Firma varios documentos con la misma posición, guardándolos junto a sus originales. */
export function BatchView() {
  const queue = useFirmarStore((s) => s.batchQueue);
  const setQueue = useFirmarStore((s) => s.setBatchQueue);
  const profiles = useFirmarStore((s) => s.profiles);
  const profile = useFirmarStore(activeProfile);
  const setActiveProfile = useFirmarStore((s) => s.setActiveProfile);
  const remembered = useFirmarStore((s) => s.remembered);
  const notify = useFirmarStore((s) => s.notify);
  const requestOpen = useFirmarStore((s) => s.requestOpen);

  const [pages, setPages] = useState<PageTarget>("last");
  const [position, setPosition] = useState({
    x: remembered?.x ?? DEFAULT_POSITION.x,
    y: remembered?.y ?? DEFAULT_POSITION.y,
  });
  const [width, setWidth] = useState(remembered?.width ?? DEFAULT_SIGNATURE_WIDTH);
  const [progress, setProgress] = useState<BatchProgress | null>(null);
  const [results, setResults] = useState<BatchItem[] | null>(null);
  const [dragging, setDragging] = useState(false);
  const running = progress !== null;

  const stamp = useStamp();
  const caption = stamp.caption;
  const add = (paths: string[]) => {
    setResults(null);
    setQueue([...useFirmarStore.getState().batchQueue, ...paths]);
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === "enter" || p.type === "over") setDragging(true);
        else if (p.type === "leave") setDragging(false);
        else if (p.type === "drop") {
          setDragging(false);
          add(p.paths);
        }
      })
      .then((fn) => (unlisten = fn));
    return () => unlisten?.();
    // Solo al montar: `add` lee la cola actual del store en cada llamada.
  }, []);

  const pick = async () => {
    const selected = await open({
      multiple: true,
      filters: [
        { name: "Documentos", extensions: ["pdf", "docx", "png", "jpg", "jpeg"] },
        { name: "Cualquier archivo", extensions: ["*"] },
      ],
    });
    if (Array.isArray(selected)) add(selected);
  };

  const run = async () => {
    if (!profile || queue.length === 0) return;
    setResults(null);
    setProgress({ done: 0, total: queue.length, current: "" });
    const unlisten = await listen<BatchProgress>("batch-progress", (e) => setProgress(e.payload));
    try {
      const items = await api.documentSignBatch(queue, profile.id, {
        pages,
        x: position.x,
        y: position.y,
        width,
        caption,
        captionAlign: stamp.captionAlign,
      });
      setResults(items);
      const ok = items.filter((i) => i.outputPath).length;
      notify(ok === items.length ? "success" : "info", `${ok} de ${items.length} documentos firmados`);
      setQueue(items.filter((i) => i.error).map((i) => i.path));
    } catch (e) {
      notify("error", errorMessage(e));
    } finally {
      unlisten();
      setProgress(null);
    }
  };

  return (
    <div className="relative flex h-full min-h-0 gap-6 p-6">
      <section className="flex min-w-0 flex-1 flex-col gap-4 rounded-2xl border border-ink-800 bg-ink-900 p-5">
        <header className="flex items-center justify-between">
          <div>
            <h2 className="text-lg font-semibold text-white">Firmar varios documentos</h2>
            <p className="text-sm text-slate-400">Cada uno se guarda junto a su original como «nombre.firmado».</p>
          </div>
          <div className="flex gap-2">
            {queue.length > 0 && !running && (
              <button type="button" onClick={() => setQueue([])} className="px-3 py-2 text-sm text-slate-400 hover:text-slate-200">
                Vaciar
              </button>
            )}
            <button
              type="button"
              onClick={pick}
              disabled={running}
              className="rounded-lg border border-ink-700 px-3 py-2 text-sm text-slate-200 hover:bg-ink-800 disabled:opacity-40"
            >
              + Agregar archivos
            </button>
          </div>
        </header>

        <ul className="min-h-0 flex-1 space-y-1 overflow-y-auto">
          {queue.length === 0 && !results && (
            <li className="flex h-full flex-col items-center justify-center rounded-xl border-2 border-dashed border-ink-700 p-10 text-center text-sm text-slate-400">
              Arrastra aquí los documentos o usa «Agregar archivos».
            </li>
          )}
          {results?.map((r) => (
            <li key={r.path} className="flex items-center gap-3 rounded-lg bg-ink-800/60 px-3 py-2 text-sm">
              <span className={r.outputPath ? "text-emerald-400" : "text-red-400"}>{r.outputPath ? "✓" : "✕"}</span>
              <span className="min-w-0 flex-1 truncate text-slate-200" title={r.path}>
                {fileName(r.path)}
                {r.error && <span className="ml-2 text-xs text-red-300">{r.error}</span>}
              </span>
              {r.outputPath && (
                <>
                  <button type="button" onClick={() => requestOpen(r.outputPath)} className="text-xs text-brand-400 hover:underline">
                    Ver
                  </button>
                  <button type="button" onClick={() => api.revealPath(r.outputPath!)} className="text-xs text-slate-400 hover:text-slate-200">
                    Carpeta
                  </button>
                </>
              )}
            </li>
          ))}
          {!results &&
            queue.map((path) => (
              <li key={path} className="flex items-center gap-3 rounded-lg bg-ink-800/60 px-3 py-2 text-sm">
                <span className="min-w-0 flex-1 truncate text-slate-200" title={path}>
                  {fileName(path)}
                </span>
                {!running && (
                  <button
                    type="button"
                    aria-label="Quitar"
                    onClick={() => setQueue(queue.filter((p) => p !== path))}
                    className="text-slate-500 hover:text-red-400"
                  >
                    ✕
                  </button>
                )}
              </li>
            ))}
        </ul>

        {progress && (
          <div className="space-y-1">
            <div className="h-2 overflow-hidden rounded-full bg-ink-800">
              <div className="h-full bg-brand-500 transition-all" style={{ width: `${(progress.done / progress.total) * 100}%` }} />
            </div>
            <p className="text-xs text-slate-400">
              Firmando {Math.min(progress.done + 1, progress.total)} de {progress.total}
              {progress.current && ` · ${progress.current}`}
            </p>
          </div>
        )}
      </section>

      <aside className="flex w-[420px] shrink-0 flex-col gap-4 overflow-y-auto rounded-2xl border border-ink-800 bg-ink-900 p-5 text-sm">
        <label className="flex flex-col gap-1 text-slate-400">
          Firma
          <select
            value={profile?.id ?? ""}
            onChange={(e) => setActiveProfile(e.target.value)}
            className="rounded-lg border border-ink-700 bg-ink-800 px-3 py-2 text-slate-200 outline-none focus:border-brand-500"
          >
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>

        <fieldset className="space-y-1">
          <legend className="mb-1 text-slate-400">Páginas</legend>
          {PAGE_OPTIONS.map((o) => (
            <label key={o.value} className="flex cursor-pointer items-start gap-2 rounded-lg px-2 py-1.5 hover:bg-ink-800">
              <input type="radio" name="pages" checked={pages === o.value} onChange={() => setPages(o.value)} className="mt-1 accent-brand-500" />
              <span>
                <span className="text-slate-200">{o.label}</span>
                <span className="block text-xs text-slate-500">{o.hint}</span>
              </span>
            </label>
          ))}
        </fieldset>

        <div className="space-y-2">
          <p className="text-slate-400">
            Posición <span className="text-xs text-slate-500">· pasa el ratón para agrandar, haz clic o arrastra la firma</span>
          </p>
          <PositionPicker
            samplePath={queue[0] ?? null}
            pages={pages}
            x={position.x}
            y={position.y}
            width={width}
            signatureUrl={profile?.dataUrl ?? null}
            signatureAspect={profile ? profile.height / profile.width : 0.35}
            stamp={stamp}
            onChange={(next) => {
              setPosition({ x: next.x, y: next.y });
              setWidth(next.width);
            }}
          />
          <div className="flex flex-wrap justify-center gap-1">
            <div className="contents">
              {PRESETS.map((p) => (
                <button
                  key={p.label}
                  type="button"
                  onClick={() => setPosition({ x: p.x, y: p.y })}
                  className="rounded-md bg-ink-800 px-2 py-1 text-xs text-slate-300 hover:bg-ink-700"
                >
                  {p.label}
                </button>
              ))}
              {remembered && (
                <button
                  type="button"
                  onClick={() => {
                    setPosition({ x: remembered.x, y: remembered.y });
                    setWidth(remembered.width);
                  }}
                  className="rounded-md bg-ink-800 px-2 py-1 text-xs text-slate-300 hover:bg-ink-700"
                >
                  Como la última
                </button>
              )}
            </div>
          </div>
          <label className="flex items-center gap-2 text-slate-400">
            Tamaño
            <input
              type="range"
              min={0.1}
              max={0.5}
              step={0.01}
              value={width}
              onChange={(e) => setWidth(Number(e.target.value))}
              className="flex-1 accent-brand-500"
            />
          </label>
        </div>

        <CaptionPanel inline summary={caption ? caption.split("\n").join(" · ") : "Sin texto bajo la firma"} />

        <button
          type="button"
          disabled={!profile || queue.length === 0 || running}
          onClick={run}
          className="mt-auto rounded-lg bg-brand-500 px-4 py-2.5 font-medium text-white hover:bg-brand-600 disabled:cursor-not-allowed disabled:opacity-40"
        >
          {running ? "Firmando…" : `Firmar ${queue.length || ""} documento${queue.length === 1 ? "" : "s"}`}
        </button>
      </aside>

      {dragging && (
        <div className="pointer-events-none absolute inset-3 z-20 flex items-center justify-center rounded-2xl border-2 border-dashed border-brand-500 bg-brand-500/10 text-brand-400">
          Suelta para añadir a la lista
        </div>
      )}
    </div>
  );
}
