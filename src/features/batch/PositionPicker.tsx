import { useEffect, useMemo, useState } from "react";
import type { PDFDocumentProxy } from "pdfjs-dist";
import { api, type PageTarget } from "../../lib/ipc";
import type { PlacedSignature } from "../../store/useFirmarStore";
import { PdfCanvas } from "../signer/DocumentViewer";
import { PlacementBox } from "../signer/PlacementBox";
import { loadPdf, type PageSize } from "../signer/pdf";
import { sizedPlacement } from "../signer/placement";
import type { CaptionAlign } from "../signer/caption";

const A4: PageSize = { width: 595, height: 842 };
const COMPACT_WIDTH = 190;
const EXPANDED_WIDTH = 372;

interface Props {
  /** Primer documento de la lista: su página sirve de vista previa. */
  samplePath: string | null;
  pages: PageTarget;
  x: number;
  y: number;
  width: number;
  signatureUrl: string | null;
  signatureAspect: number;
  stamp: { caption: string | null; captionAlign: CaptionAlign };
  onChange: (next: { x: number; y: number; width: number }) => void;
}

type Sample =
  | { kind: "pdf"; pdf: PDFDocumentProxy; pageCount: number; sizes: PageSize[] }
  | { kind: "image"; url: string; size: PageSize }
  | { kind: "blank" };

const extension = (path: string) => path.split(".").pop()?.toLowerCase() ?? "";

/**
 * Página de muestra donde se ubica la firma del lote con el ratón: clic para
 * llevarla a un punto, arrastrar para moverla y esquinas para el tamaño. Se
 * agranda al pasar el ratón para ubicarla con precisión.
 */
export function PositionPicker({ samplePath, pages, x, y, width, signatureUrl, signatureAspect, stamp, onChange }: Props) {
  const [sample, setSample] = useState<Sample>({ kind: "blank" });
  const [hover, setHover] = useState(false);
  const [dragging, setDragging] = useState(false);
  const expanded = hover || dragging;

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;
    setSample({ kind: "blank" });
    if (!samplePath) return;
    const ext = extension(samplePath);
    if (ext === "pdf") {
      api
        .documentRead(samplePath)
        .then(async (buffer) => {
          const task = loadPdf(new Uint8Array(buffer));
          cleanup = () => task.destroy();
          const pdf = await task.promise;
          const sizes: PageSize[] = [];
          for (const n of [1, pdf.numPages]) {
            const vp = (await pdf.getPage(n)).getViewport({ scale: 1 });
            sizes.push({ width: vp.width, height: vp.height });
          }
          if (!cancelled) setSample({ kind: "pdf", pdf, pageCount: pdf.numPages, sizes });
        })
        .catch(() => undefined);
    } else if (["png", "jpg", "jpeg"].includes(ext)) {
      api
        .documentRead(samplePath)
        .then((buffer) => {
          const url = URL.createObjectURL(new Blob([buffer], { type: ext === "png" ? "image/png" : "image/jpeg" }));
          cleanup = () => URL.revokeObjectURL(url);
          const img = new Image();
          img.onload = () => !cancelled && setSample({ kind: "image", url, size: { width: img.naturalWidth, height: img.naturalHeight } });
          img.src = url;
        })
        .catch(() => undefined);
    }
    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, [samplePath]);

  // Página que se muestra: la última si el lote firma la última, si no la primera.
  const showLast = pages === "last";
  const pageSize =
    sample.kind === "pdf" ? sample.sizes[showLast ? 1 : 0] : sample.kind === "image" ? sample.size : A4;
  const pageNumber = sample.kind === "pdf" ? (showLast ? sample.pageCount : 1) : 1;

  const boxWidth = expanded ? EXPANDED_WIDTH : COMPACT_WIDTH;
  const boxHeight = Math.round((boxWidth * pageSize.height) / pageSize.width);

  const placement: PlacedSignature = useMemo(
    () => ({ key: "lote", ...sizedPlacement(0, pageSize, signatureAspect, width, x, y, stamp) }),
    [pageSize, signatureAspect, width, x, y, stamp],
  );

  const moveTo = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const cx = (e.clientX - rect.left) / rect.width;
    const cy = (e.clientY - rect.top) / rect.height;
    const p = sizedPlacement(0, pageSize, signatureAspect, width, cx - placement.width / 2, cy - placement.height / 2, stamp);
    onChange({ x: p.x, y: p.y, width: p.width });
  };

  const label =
    sample.kind === "pdf"
      ? `${samplePath?.split(/[\\/]/).pop()} · página ${pageNumber} de ${sample.pageCount}`
      : samplePath
        ? (samplePath.split(/[\\/]/).pop() ?? "")
        : "Página de ejemplo (A4)";

  return (
    <div className="space-y-1.5">
      <div
        onMouseEnter={() => setHover(true)}
        onMouseLeave={() => setHover(false)}
        onPointerDown={() => setDragging(true)}
        onPointerUp={() => setDragging(false)}
        className="flex justify-center"
      >
        <div
          onClick={moveTo}
          title="Haz clic para llevar la firma aquí; arrástrala o ajusta sus esquinas"
          className="relative cursor-crosshair bg-white shadow-lg shadow-black/40 ring-1 ring-ink-700 transition-[width,height] duration-200"
          style={{ width: boxWidth, height: boxHeight }}
        >
          {sample.kind === "pdf" && (
            <PdfCanvas key={`${pageNumber}-${boxWidth}`} pdf={sample.pdf} pageNumber={pageNumber} width={boxWidth} height={boxHeight} />
          )}
          {sample.kind === "image" && <img src={sample.url} alt="" draggable={false} className="h-full w-full" />}
          <PlacementBox
            placement={placement}
            imageUrl={signatureUrl}
            editable
            freeResize={false}
            pageWidth={boxWidth}
            pageHeight={boxHeight}
            onGestureStart={() => setDragging(true)}
            onChange={(patch) => {
              const next = { ...placement, ...patch };
              onChange({ x: next.x, y: next.y, width: next.width });
            }}
            onRemove={() => undefined}
          />
        </div>
      </div>
      <p className="truncate text-center text-xs text-slate-500" title={label}>
        {label}
      </p>
    </div>
  );
}
