import { useEffect, useMemo, useRef, useState } from "react";
import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist";
import type { DocumentFormat, DocxLayout, DocxPageSetup } from "../../lib/ipc";
import { useFirmarStore, type PlacedSignature } from "../../store/useFirmarStore";
import { PlacementBox } from "./PlacementBox";
import { loadPdf, pageSizes, type PageSize } from "./pdf";
import { DEFAULT_SIGNATURE_WIDTH, sizedPlacement } from "./placement";

interface Props {
  bytes: Uint8Array;
  format: DocumentFormat;
  docxLayout: DocxLayout | null;
  zoom: number;
  editable: boolean;
  signatureUrl: string | null;
  /** alto/ancho de la imagen de firma activa. */
  signatureAspect: number;
  /** Texto (y su alineación) que llevarán las firmas nuevas. */
  stamp: { caption: string | null; captionAlign: "left" | "center" | "right" };
  /** Tamaño de cada página, en cuanto se conoce. */
  onPagesReady: (sizes: PageSize[]) => void;
  onError: (message: string) => void;
}

const BASE_WIDTH = 860;

export const pageElementId = (index: number) => `firmar-page-${index}`;

export function DocumentViewer(props: Props) {
  const { bytes, format, docxLayout, onError, onPagesReady } = props;
  const [pdf, setPdf] = useState<PDFDocumentProxy | null>(null);
  const [sizes, setSizes] = useState<PageSize[] | null>(null);
  const [imageUrl, setImageUrl] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setPdf(null);
    setSizes(null);
    setImageUrl(null);

    if (format === "docx") {
      if (docxLayout) setSizes(docxLayout.pages.map((p) => ({ width: p.width, height: p.height })));
      return;
    }
    if (format === "pdf") {
      const task = loadPdf(bytes);
      task.promise
        .then(async (doc) => {
          const s = await pageSizes(doc);
          if (cancelled) return;
          setPdf(doc);
          setSizes(s);
        })
        .catch((e) => !cancelled && onError(`No se pudo abrir el PDF: ${e?.message ?? e}`));
      return () => {
        cancelled = true;
        task.destroy();
      };
    } else {
      const url = URL.createObjectURL(new Blob([bytes.slice()], { type: format === "png" ? "image/png" : "image/jpeg" }));
      const img = new Image();
      img.onload = () => {
        if (cancelled) return;
        setImageUrl(url);
        setSizes([{ width: img.naturalWidth, height: img.naturalHeight }]);
      };
      img.onerror = () => !cancelled && onError("No se pudo mostrar la imagen");
      img.src = url;
      return () => {
        cancelled = true;
        URL.revokeObjectURL(url);
      };
    }
  }, [bytes, format, docxLayout, onError]);

  useEffect(() => {
    if (sizes) onPagesReady(sizes);
  }, [sizes, onPagesReady]);

  if (!sizes) {
    return <div className="flex h-full items-center justify-center text-sm text-slate-400">Cargando documento…</div>;
  }

  return (
    <div className="flex items-start">
      {pdf && sizes.length > 1 && <Thumbnails pdf={pdf} sizes={sizes} />}
      <div className="flex min-w-0 flex-1 flex-col items-center gap-6 py-8">
        {sizes.map((size, index) => (
          <Page
            key={index}
            index={index}
            size={size}
            pdf={pdf}
            imageUrl={imageUrl}
            docxPage={docxLayout?.pages[index] ?? null}
            {...props}
          />
        ))}
      </div>
    </div>
  );
}

const THUMB_WIDTH = 104;

/** Miniaturas para saltar de página; marcan cuántas firmas lleva cada una. */
function Thumbnails({ pdf, sizes }: { pdf: PDFDocumentProxy; sizes: PageSize[] }) {
  const placements = useFirmarStore((s) => s.placements);
  const counts = useMemo(() => {
    const c = new Map<number, number>();
    for (const p of placements) c.set(p.page, (c.get(p.page) ?? 0) + 1);
    return c;
  }, [placements]);

  return (
    <nav
      aria-label="Páginas"
      className="sticky top-0 flex max-h-[calc(100vh-7rem)] w-36 shrink-0 flex-col items-center gap-3 overflow-y-auto border-r border-ink-800 bg-ink-900/60 py-4"
    >
      {sizes.map((size, i) => (
        <button
          key={i}
          type="button"
          onClick={() => document.getElementById(pageElementId(i))?.scrollIntoView({ behavior: "smooth", block: "start" })}
          className="group flex flex-col items-center gap-1"
          title={`Ir a la página ${i + 1}`}
        >
          <div className="relative bg-white ring-1 ring-ink-700 transition group-hover:ring-2 group-hover:ring-brand-500">
            <PdfCanvas pdf={pdf} pageNumber={i + 1} width={THUMB_WIDTH} height={Math.round((THUMB_WIDTH * size.height) / size.width)} />
            {counts.get(i) ? (
              <span className="absolute -top-1.5 -right-1.5 flex h-5 min-w-5 items-center justify-center rounded-full bg-brand-500 px-1 text-[10px] font-bold text-white">
                {counts.get(i)}
              </span>
            ) : null}
          </div>
          <span className="text-[11px] text-slate-500 group-hover:text-slate-300">{i + 1}</span>
        </button>
      ))}
    </nav>
  );
}

interface PageProps extends Props {
  index: number;
  size: PageSize;
  pdf: PDFDocumentProxy | null;
  imageUrl: string | null;
  docxPage: DocxPageSetup | null;
}

function Page({ index, size, pdf, imageUrl, docxPage, docxLayout, zoom, editable, signatureUrl, signatureAspect, stamp }: PageProps) {
  const placements = useFirmarStore((s) => s.placements);
  const addPlacement = useFirmarStore((s) => s.addPlacement);
  const updatePlacement = useFirmarStore((s) => s.updatePlacement);
  const removePlacement = useFirmarStore((s) => s.removePlacement);
  const checkpoint = useFirmarStore((s) => s.checkpoint);
  const rememberedWidth = useFirmarStore((s) => s.remembered?.width ?? DEFAULT_SIGNATURE_WIDTH);
  const mine = useMemo(() => placements.filter((p) => p.page === index), [placements, index]);

  const width = Math.round(BASE_WIDTH * zoom);
  const height = Math.round((width * size.height) / size.width);

  /** Doble clic: la firma aparece centrada en el cursor, con el último tamaño usado. */
  const place = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!editable) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const probe = sizedPlacement(index, size, signatureAspect, rememberedWidth, 0, 0, stamp);
    const cx = (e.clientX - rect.left) / rect.width;
    const cy = (e.clientY - rect.top) / rect.height;
    addPlacement(
      sizedPlacement(index, size, signatureAspect, probe.width, cx - probe.width / 2, cy - probe.height / 2, stamp),
    );
  };

  return (
    <div id={pageElementId(index)} className="flex scroll-mt-24 flex-col items-center gap-2">
      <div
        onDoubleClick={place}
        title={editable ? "Doble clic para colocar tu firma aquí" : undefined}
        className={`relative bg-white shadow-xl shadow-black/40 ${editable ? "cursor-crosshair" : ""}`}
        style={{ width, height }}
      >
        {pdf ? (
          <PdfCanvas pdf={pdf} pageNumber={index + 1} width={width} height={height} />
        ) : docxPage ? (
          <SchematicPage page={docxPage} />
        ) : (
          imageUrl && <img src={imageUrl} alt="" draggable={false} className="h-full w-full" />
        )}
        {mine.map((p: PlacedSignature) => (
          <PlacementBox
            key={p.key}
            placement={p}
            imageUrl={signatureUrl}
            editable={editable}
            pageWidth={width}
            pageHeight={height}
            onChange={(patch) => updatePlacement(p.key, patch)}
            onRemove={() => removePlacement(p.key)}
            onGestureStart={checkpoint}
          />
        ))}
      </div>
      {pdf && <span className="text-xs text-slate-500">Página {index + 1}</span>}
      {docxLayout && <span className="text-xs text-slate-500">{docxPageLabel(index, docxLayout)}</span>}
    </div>
  );
}

/** Renderiza una página solo cuando entra en pantalla, a la densidad de píxeles real. */
export function PdfCanvas({ pdf, pageNumber, width, height }: { pdf: PDFDocumentProxy; pageNumber: number; width: number; height: number }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const el = canvasRef.current!;
    const io = new IntersectionObserver(([entry]) => entry.isIntersecting && setVisible(true), { rootMargin: "600px" });
    io.observe(el);
    return () => io.disconnect();
  }, []);

  useEffect(() => {
    if (!visible) return;
    let task: RenderTask | null = null;
    let cancelled = false;
    (async () => {
      const page = await pdf.getPage(pageNumber);
      if (cancelled) return;
      const dpr = window.devicePixelRatio || 1;
      const base = page.getViewport({ scale: 1 });
      const viewport = page.getViewport({ scale: (width / base.width) * dpr });
      const canvas = canvasRef.current!;
      canvas.width = Math.floor(viewport.width);
      canvas.height = Math.floor(viewport.height);
      task = page.render({ canvas, canvasContext: canvas.getContext("2d")!, viewport });
      await task.promise.catch(() => undefined);
    })();
    return () => {
      cancelled = true;
      task?.cancel();
    };
  }, [pdf, pageNumber, width, visible]);

  return <canvas ref={canvasRef} className="block" style={{ width, height }} />;
}

function docxPageLabel(index: number, layout: DocxLayout) {
  if (layout.pages.length === 1) return "Página única";
  if (index === 0) return "Primera página";
  return layout.pagesHint ? `Última página (pág. ${layout.pagesHint})` : "Última página";
}

/**
 * Vista esquemática de una página de Word: tamaño y márgenes reales, con el
 * área de texto insinuada. La firma se ancla con coordenadas absolutas de
 * página, así que su posición aquí es exactamente la que tendrá en Word.
 */
function SchematicPage({ page }: { page: DocxPageSetup }) {
  const pct = (v: number, total: number) => `${(v / total) * 100}%`;
  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden">
      <div
        className="absolute flex flex-col gap-[1.6%] border border-dashed border-slate-200"
        style={{
          top: pct(page.marginTop, page.height),
          bottom: pct(page.marginBottom, page.height),
          left: pct(page.marginLeft, page.width),
          right: pct(page.marginRight, page.width),
        }}
      >
        {Array.from({ length: 24 }, (_, i) => (
          <div key={i} className="h-[1.2%] min-h-1 rounded-full bg-slate-100" style={{ width: `${70 + ((i * 37) % 30)}%` }} />
        ))}
      </div>
      <span className="absolute top-2 left-1/2 -translate-x-1/2 rounded bg-slate-100 px-2 py-0.5 text-[10px] text-slate-500">
        Vista esquemática · el contenido real se ve en Word
      </span>
    </div>
  );
}
