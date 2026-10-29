/**
 * Disposición del texto bajo la firma. Debe coincidir con
 * `src-tauri/src/image_engine/art.rs`, que dibuja la imagen estampada:
 * cada línea ocupa `LINE_RATIO` del alto del rectángulo (hasta `MAX_RATIO` en
 * total), la letra es `FONT_RATIO` de la línea y la firma llena el resto.
 * Liberation Sans (la del estampado) tiene las mismas medidas que Arial.
 */
export const CAPTION_LINE_RATIO = 0.17;
export const CAPTION_MAX_RATIO = 0.5;
export const CAPTION_FONT_RATIO = 0.78;
export const CAPTION_MAX_WIDTH = 0.96;
export const CAPTION_MAX_LINES = 5;
export const CAPTION_MAX_LINE_CHARS = 80;
export const CAPTION_FONT = 'Arial, "Liberation Sans", Helvetica, sans-serif';

export type CaptionAlign = "left" | "center" | "right";

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

/** Campos que se sustituyen al firmar. */
export const CAPTION_TOKENS = [
  { token: "{nombre}", label: "Nombre" },
  { token: "{fecha}", label: "Fecha" },
  { token: "{fecha larga}", label: "Fecha larga" },
  { token: "{hora}", label: "Hora" },
] as const;

export interface CaptionSettings {
  /** Plantillas de línea con campos como `{nombre}` o `{fecha}`. */
  lines: string[];
  align: CaptionAlign;
}

export const EMPTY_CAPTION: CaptionSettings = { lines: [], align: "center" };
export const SUGGESTED_LINES = ["{nombre}", "Firmado el {fecha}"];

/** Ajustes guardados con el formato anterior (casillas de nombre/fecha). */
interface LegacyCaption {
  includeName?: boolean;
  name?: string;
  includeDate?: boolean;
  custom?: string;
}

export function normalizeCaption(raw: unknown): CaptionSettings {
  if (!raw || typeof raw !== "object") return EMPTY_CAPTION;
  const v = raw as Partial<CaptionSettings> & LegacyCaption;
  if (Array.isArray(v.lines)) {
    const align: CaptionAlign = v.align === "left" || v.align === "right" ? v.align : "center";
    return { lines: v.lines.filter((l) => typeof l === "string").slice(0, CAPTION_MAX_LINES), align };
  }
  const lines: string[] = [];
  if (v.includeName && v.name?.trim()) lines.push(v.name.trim());
  if (v.includeDate) lines.push("Firmado el {fecha}");
  if (v.custom?.trim()) lines.push(v.custom.trim());
  return { lines, align: "center" };
}

/**
 * Texto final: sustituye los campos y descarta las líneas vacías. `{nombre}`
 * es el nombre de «Identidad». Devuelve `null` si no queda nada.
 */
export function buildCaption(s: CaptionSettings, name: string | null, now = new Date()): string | null {
  const values: Record<string, string> = {
    "{nombre}": name?.trim() ?? "",
    "{fecha}": now.toLocaleDateString("es", { day: "2-digit", month: "2-digit", year: "numeric" }),
    "{fecha larga}": now.toLocaleDateString("es", { day: "numeric", month: "long", year: "numeric" }),
    "{hora}": now.toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit" }),
  };
  const lines = s.lines
    .map((l) => l.replace(/\{(nombre|fecha|fecha larga|hora)\}/g, (t) => values[t] ?? t).trim())
    .filter(Boolean)
    .slice(0, CAPTION_MAX_LINES)
    .map((l) => l.slice(0, CAPTION_MAX_LINE_CHARS));
  return lines.length ? lines.join("\n") : null;
}

/** `true` si alguna línea usa `{nombre}` (para avisar si aún no hay nombre). */
export const usesName = (s: CaptionSettings) => s.lines.some((l) => l.includes("{nombre}"));
