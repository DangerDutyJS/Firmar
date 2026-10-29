import { useRef } from "react";
import type { PlacedSignature } from "../../store/useFirmarStore";
import type { SignaturePlacement } from "../../lib/ipc";
import { CAPTION_FONT, captionLines, captionRatio, lineFontSize } from "./caption";

interface Props {
  placement: PlacedSignature;
  imageUrl: string | null;
  editable: boolean;
  /** Tamaño en píxeles de la página en pantalla. */
  pageWidth: number;
  pageHeight: number;
  onChange: (patch: Partial<SignaturePlacement>) => void;
  onRemove: () => void;
  /** Antes de mover o redimensionar, para poder deshacerlo. */
  onGestureStart: () => void;
}

/** Bordes que mueve cada tirador: n = arriba, s = abajo, e = derecha, w = izquierda. */
type Handle = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";
type Mode = "move" | Handle;

const MIN_SIZE = 0.02;
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

const HANDLES: { handle: Handle; className: string; cursor: string }[] = [
  { handle: "nw", className: "-top-1.5 -left-1.5 h-3 w-3", cursor: "cursor-nwse-resize" },
  { handle: "ne", className: "-top-1.5 -right-1.5 h-3 w-3", cursor: "cursor-nesw-resize" },
  { handle: "sw", className: "-bottom-1.5 -left-1.5 h-3 w-3", cursor: "cursor-nesw-resize" },
  { handle: "se", className: "-right-1.5 -bottom-1.5 h-3 w-3", cursor: "cursor-nwse-resize" },
  { handle: "n", className: "-top-1 left-1/2 h-2 w-6 -translate-x-1/2", cursor: "cursor-ns-resize" },
  { handle: "s", className: "-bottom-1 left-1/2 h-2 w-6 -translate-x-1/2", cursor: "cursor-ns-resize" },
  { handle: "w", className: "top-1/2 -left-1 h-6 w-2 -translate-y-1/2", cursor: "cursor-ew-resize" },
  { handle: "e", className: "top-1/2 -right-1 h-6 w-2 -translate-y-1/2", cursor: "cursor-ew-resize" },
];

/**
 * Firma colocada. Se arrastra para moverla; los tiradores de los lados estiran
 * libremente ese lado y los de las esquinas escalan manteniendo la proporción
 * (con Mayús se estiran libremente). La guía solo se ve al pasar el ratón o con
 * la firma seleccionada: nunca forma parte del documento.
 */
export function PlacementBox({ placement, imageUrl, editable, pageWidth, pageHeight, onChange, onRemove, onGestureStart }: Props) {
  const start = useRef<{ x: number; y: number; p: SignaturePlacement; mode: Mode } | null>(null);

  const begin = (mode: Mode) => (e: React.PointerEvent) => {
    if (!editable) return;
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    onGestureStart();
    start.current = { x: e.clientX, y: e.clientY, p: { ...placement }, mode };
  };

  const move = (e: React.PointerEvent) => {
    const s = start.current;
    if (!s) return;
    const dx = (e.clientX - s.x) / pageWidth;
    const dy = (e.clientY - s.y) / pageHeight;
    if (s.mode === "move") {
      onChange({
        x: clamp(s.p.x + dx, 0, 1 - s.p.width),
        y: clamp(s.p.y + dy, 0, 1 - s.p.height),
      });
      return;
    }
    onChange(resize(s.p, s.mode, dx, dy, s.mode.length === 2 && !e.shiftKey));
  };

  const end = () => {
    start.current = null;
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!editable) return;
    const step = e.shiftKey ? 0.01 : 0.002;
    const { x, y, width, height } = placement;
    const moves: Record<string, Partial<SignaturePlacement>> = {
      ArrowLeft: { x: clamp(x - step, 0, 1 - width) },
      ArrowRight: { x: clamp(x + step, 0, 1 - width) },
      ArrowUp: { y: clamp(y - step, 0, 1 - height) },
      ArrowDown: { y: clamp(y + step, 0, 1 - height) },
    };
    if (moves[e.key]) {
      e.preventDefault();
      if (!e.repeat) onGestureStart();
      onChange(moves[e.key]);
    } else if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      onRemove();
    }
  };

  return (
    <div
      role="button"
      tabIndex={editable ? 0 : -1}
      aria-label="Firma colocada"
      onKeyDown={onKeyDown}
      onPointerDown={begin("move")}
      onPointerMove={move}
      onPointerUp={end}
      onPointerCancel={end}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      className={`group absolute outline-none ${editable ? "cursor-move" : ""}`}
      style={{
        left: `${placement.x * 100}%`,
        top: `${placement.y * 100}%`,
        width: `${placement.width * 100}%`,
        height: `${placement.height * 100}%`,
      }}
    >
      <SignaturePreview
        imageUrl={imageUrl}
        caption={placement.caption ?? null}
        boxWidth={placement.width * pageWidth}
        boxHeight={placement.height * pageHeight}
      />
      {editable && (
        <>
          <div className="pointer-events-none absolute inset-0 rounded-sm border border-dashed border-transparent group-hover:border-brand-500/70 group-focus:border-brand-500 group-focus:bg-brand-500/10" />
          <button
            type="button"
            aria-label="Quitar firma"
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              onRemove();
            }}
            className="absolute -top-7 right-0 hidden h-6 w-6 items-center justify-center rounded-full bg-red-500 text-xs text-white shadow group-hover:flex group-focus:flex"
          >
            ✕
          </button>
          {HANDLES.map(({ handle, className, cursor }) => (
            <div
              key={handle}
              onPointerDown={begin(handle)}
              className={`absolute ${className} ${cursor} rounded-sm border-2 border-white bg-brand-500 opacity-0 shadow group-hover:opacity-100 group-focus:opacity-100`}
            />
          ))}
        </>
      )}
    </div>
  );
}

/**
 * Lo mismo que se estampará. Sin texto, la firma se estira al rectángulo. Con
 * texto, la firma va arriba sin deformarse y cada línea abajo, con la misma
 * disposición que `image_engine/art.rs`.
 */
function SignaturePreview({
  imageUrl,
  caption,
  boxWidth,
  boxHeight,
}: {
  imageUrl: string | null;
  caption: string | null;
  boxWidth: number;
  boxHeight: number;
}) {
  const lines = captionLines(caption);
  if (lines.length === 0) {
    return imageUrl ? <img src={imageUrl} alt="" draggable={false} className="pointer-events-none h-full w-full object-fill" /> : null;
  }
  const ratio = captionRatio(caption);
  const lineHeight = (boxHeight * ratio) / lines.length;
  return (
    <div className="pointer-events-none flex h-full w-full flex-col">
      <div className="min-h-0" style={{ height: `${(1 - ratio) * 100}%` }}>
        {imageUrl && <img src={imageUrl} alt="" draggable={false} className="h-full w-full object-contain" />}
      </div>
      {lines.map((line, i) => (
        <div
          key={i}
          className="overflow-visible text-center whitespace-nowrap"
          style={{
            height: lineHeight,
            lineHeight: `${lineHeight}px`,
            fontSize: lineFontSize(line, boxWidth, lineHeight),
            fontFamily: CAPTION_FONT,
            color: "rgb(31 41 55)",
          }}
        >
          {line}
        </div>
      ))}
    </div>
  );
}

/** Nuevo rectángulo al arrastrar un tirador, sin salir de la página. */
function resize(p: SignaturePlacement, handle: Handle, dx: number, dy: number, keepAspect: boolean): SignaturePlacement {
  let left = p.x;
  let top = p.y;
  let right = p.x + p.width;
  let bottom = p.y + p.height;

  if (handle.includes("w")) left = clamp(left + dx, 0, right - MIN_SIZE);
  if (handle.includes("e")) right = clamp(right + dx, left + MIN_SIZE, 1);
  if (handle.includes("n")) top = clamp(top + dy, 0, bottom - MIN_SIZE);
  if (handle.includes("s")) bottom = clamp(bottom + dy, top + MIN_SIZE, 1);

  if (keepAspect) {
    // Esquina: manda el ancho y el alto lo sigue, anclado a la esquina opuesta.
    const ratio = p.height / p.width;
    let width = right - left;
    let height = width * ratio;
    const maxHeight = handle.includes("n") ? p.y + p.height : 1 - p.y;
    if (height > maxHeight) {
      height = maxHeight;
      width = height / ratio;
    }
    if (handle.includes("w")) left = p.x + p.width - width;
    else right = left + width;
    if (handle.includes("n")) top = p.y + p.height - height;
    else bottom = top + height;
  }

  return { ...p, x: left, y: top, width: right - left, height: bottom - top };
}
