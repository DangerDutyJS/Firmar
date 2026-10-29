import { useCallback, useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, errorMessage, type DocumentFormat } from "../../lib/ipc";
import { activeProfile, useFirmarStore, type OpenDocument } from "../../store/useFirmarStore";
import { buildCaption } from "./caption";
import { CaptionPanel } from "./CaptionPanel";
import { DocumentViewer, pageElementId } from "./DocumentViewer";
import type { PageSize } from "./pdf";
import { DEFAULT_POSITION, DEFAULT_SIGNATURE_WIDTH, sizedPlacement } from "./placement";
import { StatusBanner } from "./StatusBanner";

const EXTENSIONS: Record<DocumentFormat, string[]> = {
  pdf: ["pdf"],
  png: ["png"],
  jpeg: ["jpg", "jpeg"],
  docx: ["docx"],
  binary: [],
};

function suggestedOutput(path: string, mode: OpenDocument["mode"]) {
  if (mode === "resign") return path;
  const sep = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  const dot = path.lastIndexOf(".");
  return dot > sep ? `${path.slice(0, dot)}.firmado${path.slice(dot)}` : `${path}.firmado`;
}

export function SignerView() {
  const doc = useFirmarStore((s) => s.document);
  const placements = useFirmarStore((s) => s.placements);
  const profiles = useFirmarStore((s) => s.profiles);
  const profile = useFirmarStore(activeProfile);
  const setActiveProfile = useFirmarStore((s) => s.setActiveProfile);
  const setDocument = useFirmarStore((s) => s.setDocument);
  const addPlacement = useFirmarStore((s) => s.addPlacement);
  const updatePlacement = useFirmarStore((s) => s.updatePlacement);
  const setView = useFirmarStore((s) => s.setView);
  const notify = useFirmarStore((s) => s.notify);
  const busy = useFirmarStore((s) => s.busy);
  const setBusy = useFirmarStore((s) => s.setBusy);
  const captionSettings = useFirmarStore((s) => s.caption);
  const canUndo = useFirmarStore((s) => s.past.length > 0);
  const canRedo = useFirmarStore((s) => s.future.length > 0);
  const undo = useFirmarStore((s) => s.undo);
  const redo = useFirmarStore((s) => s.redo);
  const pendingOpen = useFirmarStore((s) => s.pendingOpen);
  const requestOpen = useFirmarStore((s) => s.requestOpen);

  const [zoom, setZoom] = useState(1);
  const [dragging, setDragging] = useState(false);
  const [pageSizes, setPageSizes] = useState<PageSize[]>([]);

  const aspect = profile ? profile.height / profile.width : 0.35;
  const editable = doc !== null && doc.mode !== "readonly";
  const visual = doc !== null && doc.inspection.format !== "binary";
  const caption = buildCaption(captionSettings);
  const canSign = profile !== null && busy === null && (!visual || placements.length > 0);

  const openPath = useCallback(
    async (path: string) => {
      setBusy("Verificando documento…");
      setPageSizes([]);
      try {
        const inspection = await api.documentInspect(path);
        const { status } = inspection;
        // Word no se renderiza: el visor dibuja páginas esquemáticas con su tamaño real.
        const docxLayout = inspection.format === "docx" ? await api.docxLayout(path) : null;
        const viewerHasBytes = inspection.format !== "binary" && inspection.format !== "docx";
        const ownedMeta =
          (status.state === "valid" || status.state === "modifiedAfterSigning") && status.owned ? status.metadata : null;

        // Un documento firmado se muestra tal cual quedó (sin controles de
        // edición); re-posicionar es una acción explícita.
        const bytes = viewerHasBytes ? new Uint8Array(await api.documentRead(path)) : new Uint8Array();
        const mode = status.state === "unsigned" ? "sign" : "readonly";
        // Word se ve en esquema, así que dibujamos ahí las firmas guardadas (sin marco).
        const shown = inspection.format === "docx" && ownedMeta ? ownedMeta.placements : [];
        setDocument({ inspection, bytes, docxLayout, mode }, shown);
      } catch (e) {
        notify("error", errorMessage(e));
      } finally {
        setBusy(null);
      }
    },
    [notify, setBusy, setDocument],
  );

  const status = doc?.inspection.status;
  const ownedMeta =
    status && (status.state === "valid" || status.state === "modifiedAfterSigning") && status.owned
      ? status.metadata
      : null;

  /** Carga el original sin firma desde la bóveda y deja editar las posiciones. */
  const startResign = async () => {
    if (!doc || !ownedMeta) return;
    setBusy("Cargando original desde la bóveda…");
    try {
      const path = doc.inspection.path;
      const viewerHasBytes = doc.inspection.format !== "binary" && doc.inspection.format !== "docx";
      const bytes = viewerHasBytes ? new Uint8Array(await api.documentOriginal(path)) : new Uint8Array();
      if (useFirmarStore.getState().profiles.some((p) => p.id === ownedMeta.profileId)) {
        setActiveProfile(ownedMeta.profileId);
      }
      setDocument({ ...doc, bytes, mode: "resign" }, ownedMeta.placements);
    } catch (e) {
      notify("error", errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  // Archivos que llegan de fuera: «Firmar con Firmar», arrastrar varios, historial…
  useEffect(() => {
    if (pendingOpen && profiles.length > 0) {
      requestOpen(null);
      openPath(pendingOpen);
    }
  }, [pendingOpen, profiles.length, openPath, requestOpen]);

  // Ctrl+Z / Ctrl+Y (o Ctrl+Mayús+Z), salvo mientras se escribe en un campo.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || (e.target as HTMLElement)?.closest("input, textarea, select")) return;
      const key = e.key.toLowerCase();
      if (key === "z" && !e.shiftKey) {
        e.preventDefault();
        undo();
      } else if (key === "y" || (key === "z" && e.shiftKey)) {
        e.preventDefault();
        redo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === "enter" || p.type === "over") setDragging(true);
        else if (p.type === "leave") setDragging(false);
        else if (p.type === "drop") {
          setDragging(false);
          if (p.paths.length > 1) {
            // Varios archivos: van al firmado por lotes.
            useFirmarStore.getState().setBatchQueue(p.paths);
          } else if (p.paths[0]) {
            openPath(p.paths[0]);
          }
        }
      })
      .then((fn) => (unlisten = fn));
    return () => unlisten?.();
  }, [openPath]);

  // Al cambiar de firma, las ya colocadas adoptan la proporción de la nueva
  // conservando su ancho (o encogiéndose si se saldrían de la página).
  const previousAspect = useRef(aspect);
  useEffect(() => {
    const factor = aspect / previousAspect.current;
    previousAspect.current = aspect;
    if (Math.abs(factor - 1) < 1e-6) return;
    for (const p of useFirmarStore.getState().placements) {
      let width = p.width;
      let height = p.height * factor;
      if (p.y + height > 1) {
        const shrink = (1 - p.y) / height;
        height *= shrink;
        width *= shrink;
      }
      updatePlacement(p.key, { width, height });
    }
  }, [aspect, updatePlacement]);

  const onViewerError = useCallback((message: string) => notify("error", message), [notify]);

  /**
   * Coloca la firma en la última página, en el mismo sitio y con el mismo
   * ancho que la anterior (o abajo a la derecha la primera vez).
   */
  const addSignature = () => {
    if (pageSizes.length === 0) return;
    const page = pageSizes.length - 1;
    const r = useFirmarStore.getState().remembered;
    addPlacement(
      sizedPlacement(
        page,
        pageSizes[page],
        aspect,
        r?.width ?? DEFAULT_SIGNATURE_WIDTH,
        r?.x ?? DEFAULT_POSITION.x,
        r?.y ?? DEFAULT_POSITION.y,
        caption,
      ),
    );
    document.getElementById(pageElementId(page))?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const pickFile = async () => {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [
        { name: "Documentos", extensions: ["pdf", "docx", "png", "jpg", "jpeg"] },
        { name: "Cualquier archivo", extensions: ["*"] },
      ],
    });
    if (typeof selected === "string") openPath(selected);
  };

  /**
   * Por defecto se guarda sin preguntar: junto al original como
   * `nombre.firmado.ext` (o sobre el mismo archivo al re-posicionar).
   */
  const sign = async (saveAs = false) => {
    if (!doc || !profile) return;
    const { inspection, mode } = doc;
    let output: string | null = null;
    if (saveAs) {
      output = await save({
        defaultPath: suggestedOutput(inspection.path, mode),
        filters: EXTENSIONS[inspection.format].length
          ? [{ name: inspection.format.toUpperCase(), extensions: EXTENSIONS[inspection.format] }]
          : undefined,
      });
      if (!output) return;
    }

    setBusy(mode === "resign" ? "Re-posicionando firma…" : "Firmando…");
    try {
      const plain = placements.map(({ key: _key, ...p }) => p);
      const result =
        mode === "resign"
          ? await api.documentResign(inspection.path, output, profile.id, plain)
          : await api.documentSign(inspection.path, output, profile.id, plain);
      const name = result.outputPath.split(/[\\/]/).pop();
      const fail = (e: unknown) => notify("error", errorMessage(e));
      notify(
        "success",
        result.sidecarPath
          ? `Firmado: ${name}. Mantén el archivo .firmar junto a él; sin él no se puede verificar.`
          : mode === "resign"
            ? `Firma actualizada en ${name}`
            : `Firmado y guardado como ${name}`,
        [
          { label: "Abrir", run: () => api.openPath(result.outputPath).catch(fail) },
          { label: "Mostrar en carpeta", run: () => api.revealPath(result.outputPath).catch(fail) },
        ],
      );
      setBusy(null);
      await openPath(result.outputPath);
    } catch (e) {
      notify("error", errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  if (profiles.length === 0) {
    return (
      <Empty>
        <p className="text-slate-300">Primero guarda tu firma en la bóveda.</p>
        <button
          type="button"
          onClick={() => setView("vault")}
          className="mt-4 rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white hover:bg-brand-600"
        >
          Crear mi firma
        </button>
      </Empty>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-3 border-b border-ink-800 bg-ink-900 px-5 py-3">
        <button
          type="button"
          onClick={pickFile}
          className="rounded-lg border border-ink-700 px-3 py-2 text-sm text-slate-200 hover:bg-ink-800"
        >
          Abrir documento
        </button>
        {doc && (
          <span className="max-w-xs truncate text-sm text-slate-400" title={doc.inspection.path}>
            {doc.inspection.fileName}
          </span>
        )}

        <div className="ml-auto flex items-center gap-3">
          <select
            value={profile?.id ?? ""}
            onChange={(e) => setActiveProfile(e.target.value)}
            className="rounded-lg border border-ink-700 bg-ink-800 px-3 py-2 text-sm text-slate-200 outline-none focus:border-brand-500"
          >
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          {visual && editable && (
            <div className="flex rounded-lg border border-ink-700 text-sm">
              <button
                type="button"
                onClick={undo}
                disabled={!canUndo}
                title="Deshacer (Ctrl+Z)"
                aria-label="Deshacer"
                className="px-2.5 py-2 text-slate-300 hover:text-white disabled:opacity-30"
              >
                ↶
              </button>
              <button
                type="button"
                onClick={redo}
                disabled={!canRedo}
                title="Rehacer (Ctrl+Y)"
                aria-label="Rehacer"
                className="border-l border-ink-700 px-2.5 py-2 text-slate-300 hover:text-white disabled:opacity-30"
              >
                ↷
              </button>
            </div>
          )}
          {visual && editable && <CaptionPanel />}
          {visual && editable && (
            <div className="flex items-center gap-2">
              <span className="hidden text-xs text-slate-500 xl:inline">Doble clic en la página para colocarla ahí, o</span>
              <button
                type="button"
                onClick={addSignature}
                disabled={pageSizes.length === 0}
                className="rounded-lg border border-ink-700 px-3 py-2 text-sm text-slate-200 hover:bg-ink-800 disabled:opacity-40"
              >
                + Agregar firma
              </button>
            </div>
          )}
          {visual && (
            <div className="flex items-center rounded-lg border border-ink-700 text-sm">
              <button type="button" className="px-2.5 py-2 text-slate-300 hover:text-white" onClick={() => setZoom((z) => Math.max(0.5, +(z - 0.1).toFixed(2)))}>
                −
              </button>
              <span className="w-12 text-center text-xs text-slate-400">{Math.round(zoom * 100)}%</span>
              <button type="button" className="px-2.5 py-2 text-slate-300 hover:text-white" onClick={() => setZoom((z) => Math.min(2, +(z + 0.1).toFixed(2)))}>
                +
              </button>
            </div>
          )}
          {doc?.mode === "readonly" && ownedMeta && (
            <button
              type="button"
              onClick={startResign}
              className="rounded-lg border border-ink-700 px-3 py-2 text-sm text-slate-200 hover:bg-ink-800"
            >
              Re-posicionar firma
            </button>
          )}
          {doc?.mode === "resign" && (
            <button
              type="button"
              onClick={() => openPath(doc.inspection.path)}
              className="rounded-lg px-3 py-2 text-sm text-slate-400 hover:text-slate-200"
            >
              Cancelar
            </button>
          )}
          {editable && (
            <div className="flex overflow-hidden rounded-lg">
              <button
                type="button"
                disabled={!canSign}
                onClick={() => sign()}
                title={doc?.mode === "resign" ? "Actualiza este mismo archivo" : "Se guarda junto al original como «nombre.firmado»"}
                className="bg-brand-500 px-4 py-2 text-sm font-medium text-white hover:bg-brand-600 disabled:cursor-not-allowed disabled:opacity-40"
              >
                {doc?.mode === "resign" ? "Guardar cambios" : "Firmar"}
              </button>
              <button
                type="button"
                disabled={!canSign}
                onClick={() => sign(true)}
                title="Guardar como…"
                aria-label="Guardar como…"
                className="border-l border-brand-600 bg-brand-500 px-2.5 py-2 text-sm text-white hover:bg-brand-600 disabled:cursor-not-allowed disabled:opacity-40"
              >
                …
              </button>
            </div>
          )}
        </div>
      </div>

      <div className="relative min-h-0 flex-1 overflow-auto bg-ink-950">
        {doc && (
          <div className="sticky top-0 z-10 mx-auto max-w-3xl px-5 pt-4">
            <StatusBanner inspection={doc.inspection} />
          </div>
        )}

        {!doc ? (
          <Empty>
            <p className="text-slate-300">Arrastra un PDF, un Word, una imagen o cualquier archivo aquí</p>
            <p className="mt-1 text-sm text-slate-500">o usa «Abrir documento». Nada sale de tu equipo.</p>
            <p className="mt-4 text-xs text-slate-600">¿Muchos documentos? Arrastra varios a la vez para firmarlos de una sola vez.</p>
          </Empty>
        ) : visual ? (
          <DocumentViewer
            bytes={doc.bytes}
            format={doc.inspection.format}
            docxLayout={doc.docxLayout}
            zoom={zoom}
            editable={editable}
            signatureUrl={profile?.dataUrl ?? null}
            signatureAspect={aspect}
            caption={caption}
            onPagesReady={setPageSizes}
            onError={onViewerError}
          />
        ) : (
          <Empty>
            <p className="text-slate-300">{doc.inspection.fileName}</p>
            <p className="mt-1 max-w-md text-sm text-slate-500">
              En este tipo de archivo la firma no se ve dentro del documento. Firmar Vecipp crea un archivo
              <code> .firmar</code> a su lado que demuestra que es tuyo y que nadie lo cambió.
            </p>
          </Empty>
        )}

        {dragging && (
          <div className="pointer-events-none absolute inset-3 z-20 flex items-center justify-center rounded-2xl border-2 border-dashed border-brand-500 bg-brand-500/10 text-brand-400">
            Suelta para abrir en Firmar Vecipp
          </div>
        )}
      </div>
    </div>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return <div className="flex h-full min-h-80 flex-col items-center justify-center p-10 text-center">{children}</div>;
}
