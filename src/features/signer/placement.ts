import type { SignaturePlacement } from "../../lib/ipc";
import { captionRatio, type CaptionAlign } from "./caption";
import type { PageSize } from "./pdf";

export const DEFAULT_SIGNATURE_WIDTH = 0.26;
/** Posición por defecto la primera vez: abajo a la derecha. */
export const DEFAULT_POSITION = { x: 0.62, y: 0.8 };

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/**
 * Rectángulo de una firma con el ancho pedido y el alto que respeta su
 * proporción en esa página (más el texto, si lo hay), sin salirse de ella.
 * Misma regla que `fit_placement` en `src-tauri/src/services`.
 */
export function sizedPlacement(
  page: number,
  pageSize: PageSize,
  signatureAspect: number,
  width: number,
  x: number,
  y: number,
  stamp: { caption: string | null; captionAlign: CaptionAlign } = { caption: null, captionAlign: "center" },
): SignaturePlacement {
  const { caption, captionAlign } = stamp;
  let w = clamp(width, 0.03, 0.9);
  // Fracción de alto = fracción de ancho × (alto/ancho de la firma) × (ancho/alto de la página).
  let h = (w * signatureAspect * (pageSize.width / pageSize.height)) / (1 - captionRatio(caption));
  if (h > 0.9) {
    w *= 0.9 / h;
    h = 0.9;
  }
  return { page, width: w, height: h, x: clamp(x, 0, 1 - w), y: clamp(y, 0, 1 - h), caption, captionAlign };
}

/**
 * Nuevo alto al cambiar el texto, conservando el tamaño visible de la firma:
 * la parte de la firma mide lo mismo y solo crece o se encoge la del texto.
 */
export function resizeForCaption(p: SignaturePlacement, caption: string | null): Partial<SignaturePlacement> {
  const signaturePart = p.height * (1 - captionRatio(p.caption));
  let height = signaturePart / (1 - captionRatio(caption));
  let width = p.width;
  if (p.y + height > 1) {
    const shrink = (1 - p.y) / height;
    height *= shrink;
    width *= shrink;
  }
  return { width, height };
}
