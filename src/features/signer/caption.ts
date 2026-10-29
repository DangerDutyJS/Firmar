/**
 * Disposición del texto bajo la firma. Debe coincidir con
 * `src-tauri/src/image_engine/art.rs`, que dibuja la imagen estampada:
 * cada línea ocupa `LINE_RATIO` del alto del rectángulo, la letra es
 * `FONT_RATIO` de la línea y la firma llena el resto, centrada.
 * Liberation Sans (la del estampado) tiene las mismas medidas que Arial.
 */
export const CAPTION_LINE_RATIO = 0.17;
export const CAPTION_MAX_RATIO = 0.5;
export const CAPTION_FONT_RATIO = 0.78;
export const CAPTION_MAX_WIDTH = 0.96;
export const CAPTION_FONT = 'Arial, "Liberation Sans", Helvetica, sans-serif';

export function captionLines(caption: string | null | undefined): string[] {
  return (caption ?? "")
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
}

/** Fracción del alto del rectángulo que ocupa el texto. */
export function captionRatio(caption: string | null | undefined): number {
  return Math.min(CAPTION_MAX_RATIO, CAPTION_LINE_RATIO * captionLines(caption).length);
}

let measureCtx: CanvasRenderingContext2D | null = null;

/** Tamaño de letra (px) de una línea para un rectángulo en pantalla. */
export function lineFontSize(line: string, boxWidth: number, lineHeight: number): number {
  let px = lineHeight * CAPTION_FONT_RATIO;
  measureCtx ??= document.createElement("canvas").getContext("2d");
  if (measureCtx) {
    measureCtx.font = `${px}px ${CAPTION_FONT}`;
    const width = measureCtx.measureText(line).width;
    const max = boxWidth * CAPTION_MAX_WIDTH;
    if (width > max) px *= max / width;
  }
  return px;
}

export interface CaptionSettings {
  includeName: boolean;
  name: string;
  includeDate: boolean;
  custom: string;
}

export const EMPTY_CAPTION: CaptionSettings = { includeName: false, name: "", includeDate: false, custom: "" };

/** Texto final a partir de los ajustes; `null` si no hay nada que poner. */
export function buildCaption(s: CaptionSettings, now = new Date()): string | null {
  const lines: string[] = [];
  if (s.includeName && s.name.trim()) lines.push(s.name.trim());
  if (s.includeDate) lines.push(`Firmado el ${now.toLocaleDateString("es", { day: "2-digit", month: "2-digit", year: "numeric" })}`);
  if (s.custom.trim()) lines.push(s.custom.trim());
  return lines.length ? lines.slice(0, 3).map((l) => l.slice(0, 80)).join("\n") : null;
}
