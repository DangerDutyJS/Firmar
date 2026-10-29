import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import { trimToInk } from "./imageProcessing";

interface Point {
  x: number;
  y: number;
  pressure: number;
}

type Stroke = { color: string; width: number; points: Point[] };

export interface SignaturePadHandle {
  /** PNG recortado al trazo, o `null` si el lienzo está vacío. */
  exportPng: () => string | null;
  clear: () => void;
}

interface Props {
  color: string;
  thickness: number;
  onChange?: (empty: boolean) => void;
}

/** Resolución de exportación respecto al tamaño en pantalla. */
const EXPORT_SCALE = 3;

export const SignaturePad = forwardRef<SignaturePadHandle, Props>(function SignaturePad(
  { color, thickness, onChange },
  ref,
) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const strokes = useRef<Stroke[]>([]);
  const current = useRef<Stroke | null>(null);
  const [canUndo, setCanUndo] = useState(false);

  const redraw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    for (const s of strokes.current) drawStroke(ctx, s);
    if (current.current) drawStroke(ctx, current.current);
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current!;
    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      const rect = canvas.getBoundingClientRect();
      canvas.width = Math.round(rect.width * dpr);
      canvas.height = Math.round(rect.height * dpr);
      redraw();
    };
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [redraw]);

  const notify = () => {
    setCanUndo(strokes.current.length > 0);
    onChange?.(strokes.current.length === 0);
  };

  const pointFrom = (e: React.PointerEvent<HTMLCanvasElement>): Point => {
    const rect = e.currentTarget.getBoundingClientRect();
    // Ratón y touch reportan 0 / 0.5 fijos; solo el lápiz da presión real.
    const pressure = e.pointerType === "pen" && e.pressure > 0 ? e.pressure : 0.5;
    return { x: e.clientX - rect.left, y: e.clientY - rect.top, pressure };
  };

  const onPointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    current.current = { color, width: thickness, points: [pointFrom(e)] };
    redraw();
  };

  const onPointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!current.current) return;
    const events = e.nativeEvent.getCoalescedEvents?.() ?? [e.nativeEvent];
    const rect = e.currentTarget.getBoundingClientRect();
    for (const ev of events) {
      const pressure = ev.pointerType === "pen" && ev.pressure > 0 ? ev.pressure : 0.5;
      current.current.points.push({ x: ev.clientX - rect.left, y: ev.clientY - rect.top, pressure });
    }
    redraw();
  };

  const onPointerUp = () => {
    if (!current.current) return;
    strokes.current.push(current.current);
    current.current = null;
    redraw();
    notify();
  };

  const undo = () => {
    strokes.current.pop();
    redraw();
    notify();
  };

  useImperativeHandle(ref, () => ({
    clear: () => {
      strokes.current = [];
      redraw();
      notify();
    },
    exportPng: () => {
      const canvas = canvasRef.current;
      if (!canvas || strokes.current.length === 0) return null;
      const rect = canvas.getBoundingClientRect();
      const off = document.createElement("canvas");
      off.width = Math.round(rect.width * EXPORT_SCALE);
      off.height = Math.round(rect.height * EXPORT_SCALE);
      const ctx = off.getContext("2d")!;
      ctx.scale(EXPORT_SCALE, EXPORT_SCALE);
      for (const s of strokes.current) drawStroke(ctx, s);
      return trimToInk(off, 8 * EXPORT_SCALE)?.toDataURL("image/png") ?? null;
    },
  }));

  return (
    <div className="relative h-full w-full">
      <canvas
        ref={canvasRef}
        className="h-full w-full touch-none cursor-crosshair rounded-xl bg-white"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      />
      <div className="pointer-events-none absolute inset-x-10 bottom-14 border-b border-dashed border-slate-300" />
      <span className="pointer-events-none absolute bottom-6 left-10 text-xs text-slate-400">Firma aquí</span>
      <button
        type="button"
        onClick={undo}
        disabled={!canUndo}
        className="absolute top-3 right-3 rounded-lg bg-slate-100 px-3 py-1.5 text-xs font-medium text-slate-600 hover:bg-slate-200 disabled:opacity-40"
      >
        Deshacer
      </button>
    </div>
  );
});

/** Trazo suavizado con curvas cuadráticas por puntos medios y grosor según presión/velocidad. */
function drawStroke(ctx: CanvasRenderingContext2D, s: Stroke) {
  const pts = s.points;
  ctx.strokeStyle = s.color;
  ctx.fillStyle = s.color;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";

  if (pts.length === 1) {
    ctx.beginPath();
    ctx.arc(pts[0].x, pts[0].y, s.width * 0.5, 0, Math.PI * 2);
    ctx.fill();
    return;
  }

  let prevWidth = s.width;
  for (let i = 1; i < pts.length; i++) {
    const p0 = pts[i - 1];
    const p1 = pts[i];
    const velocity = Math.hypot(p1.x - p0.x, p1.y - p0.y);
    const target = s.width * (0.55 + p1.pressure * 0.9) * Math.max(0.45, 1 - velocity / 60);
    const width = prevWidth + (target - prevWidth) * 0.35;
    prevWidth = width;

    const mid0 = i > 1 ? midpoint(pts[i - 2], p0) : p0;
    const mid1 = midpoint(p0, p1);
    ctx.lineWidth = width;
    ctx.beginPath();
    ctx.moveTo(mid0.x, mid0.y);
    ctx.quadraticCurveTo(p0.x, p0.y, mid1.x, mid1.y);
    ctx.stroke();
  }
  const last = pts[pts.length - 1];
  const mid = midpoint(pts[pts.length - 2], last);
  ctx.beginPath();
  ctx.moveTo(mid.x, mid.y);
  ctx.lineTo(last.x, last.y);
  ctx.stroke();
}

const midpoint = (a: Point, b: Point) => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });
