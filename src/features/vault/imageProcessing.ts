/** Procesamiento local de imágenes de firma (todo en el cliente, sin red). */

const ALPHA_THRESHOLD = 12;

/** Recorta el lienzo al área con tinta, dejando un margen. */
export function trimToInk(source: HTMLCanvasElement, padding = 12): HTMLCanvasElement | null {
  const ctx = source.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  const { width, height } = source;
  const data = ctx.getImageData(0, 0, width, height).data;

  let minX = width, minY = height, maxX = -1, maxY = -1;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (data[(y * width + x) * 4 + 3] > ALPHA_THRESHOLD) {
        if (x < minX) minX = x;
        if (x > maxX) maxX = x;
        if (y < minY) minY = y;
        if (y > maxY) maxY = y;
      }
    }
  }
  if (maxX < 0) return null;

  minX = Math.max(0, minX - padding);
  minY = Math.max(0, minY - padding);
  maxX = Math.min(width - 1, maxX + padding);
  maxY = Math.min(height - 1, maxY + padding);

  const out = document.createElement("canvas");
  out.width = maxX - minX + 1;
  out.height = maxY - minY + 1;
  out.getContext("2d")!.drawImage(source, minX, minY, out.width, out.height, 0, 0, out.width, out.height);
  return out;
}

export interface BackgroundRemovalOptions {
  /** 0..100: cuánto se aleja un píxel del fondo antes de considerarse tinta. */
  sensitivity: number;
  /** Fuerza el color de la tinta (p. ej. azul o negro uniforme). `null` conserva el original. */
  inkColor: [number, number, number] | null;
}

/**
 * Convierte el fondo (papel) en transparente. La tinta siempre es más oscura
 * que el papel, así que la opacidad de cada píxel sale de cuánto más oscuro es
 * que la luminosidad del papel (percentil 90 de la imagen), con una rampa
 * suave para no dejar bordes dentados. Todo lo igual de claro o más claro que
 * el papel queda 100 % transparente: así no queda un velo blanco que aclare
 * el documento debajo de la firma.
 */
export function removeBackground(
  img: HTMLImageElement | ImageBitmap,
  { sensitivity, inkColor }: BackgroundRemovalOptions,
  maxSide = 1600,
): HTMLCanvasElement {
  const scale = Math.min(1, maxSide / Math.max(img.width, img.height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(img.width * scale));
  canvas.height = Math.max(1, Math.round(img.height * scale));
  const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

  const image = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const d = image.data;
  const paper = paperLuminance(d);

  // Diferencia de luminosidad (0..255) a partir de la cual un píxel es tinta.
  const low = 10 + (100 - sensitivity) * 0.6;
  const high = low + 50;

  for (let i = 0; i < d.length; i += 4) {
    const darker = paper - luminance(d[i], d[i + 1], d[i + 2]);
    const t = Math.min(1, Math.max(0, (darker - low) / (high - low)));
    const alpha = t * t * (3 - 2 * t) * (d[i + 3] / 255);
    if (inkColor) {
      d[i] = inkColor[0];
      d[i + 1] = inkColor[1];
      d[i + 2] = inkColor[2];
    }
    d[i + 3] = Math.round(alpha * 255);
  }
  ctx.putImageData(image, 0, 0);
  return canvas;
}

/**
 * Elimina marcos y recuadros (sólidos o punteados) pegados al borde de la
 * imagen, típicos de capturas de formularios. Etiqueta los trazos como
 * componentes conexos y borra los que tienen **todos** sus píxeles dentro de
 * la franja exterior: un trazo de la firma que toca el borde también se
 * adentra en la imagen y por eso se conserva.
 */
export function removeEdgeFrames(canvas: HTMLCanvasElement, bandRatio = 0.06): number {
  const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
  const { width: w, height: h } = canvas;
  const image = ctx.getImageData(0, 0, w, h);
  const d = image.data;
  const band = Math.max(4, Math.round(Math.min(w, h) * bandRatio));
  const inBand = (x: number, y: number) => x < band || y < band || x >= w - band || y >= h - band;

  const labels = new Int32Array(w * h);
  const stack = new Int32Array(w * h);
  let next = 0;
  let removed = 0;

  for (let start = 0; start < w * h; start++) {
    if (labels[start] !== 0 || d[start * 4 + 3] <= ALPHA_THRESHOLD) continue;
    const label = ++next;
    labels[start] = label;
    let top = 0;
    stack[top++] = start;
    let onlyBand = true;
    const pixels: number[] = [];

    while (top > 0) {
      const i = stack[--top];
      pixels.push(i);
      const x = i % w;
      const y = (i - x) / w;
      if (onlyBand && !inBand(x, y)) onlyBand = false;
      for (let dy = -1; dy <= 1; dy++) {
        for (let dx = -1; dx <= 1; dx++) {
          const nx = x + dx;
          const ny = y + dy;
          if (nx < 0 || ny < 0 || nx >= w || ny >= h) continue;
          const j = ny * w + nx;
          if (labels[j] === 0 && d[j * 4 + 3] > ALPHA_THRESHOLD) {
            labels[j] = label;
            stack[top++] = j;
          }
        }
      }
    }

    if (onlyBand) {
      for (const i of pixels) d[i * 4 + 3] = 0;
      removed++;
    }
  }

  // Restos semitransparentes del antialiasing de los marcos borrados.
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      if (inBand(x, y) && labels[i] === 0) d[i * 4 + 3] = 0;
    }
  }

  ctx.putImageData(image, 0, 0);
  return removed;
}

const luminance = (r: number, g: number, b: number) => 0.2126 * r + 0.7152 * g + 0.0722 * b;

/** Luminosidad del papel: percentil 90 de los píxeles visibles (el papel domina y es lo más claro). */
function paperLuminance(d: Uint8ClampedArray): number {
  const histogram = new Uint32Array(256);
  let total = 0;
  for (let i = 0; i < d.length; i += 4) {
    if (d[i + 3] === 0) continue;
    histogram[Math.round(luminance(d[i], d[i + 1], d[i + 2]))]++;
    total++;
  }
  if (total === 0) return 255;
  let seen = 0;
  for (let l = 0; l < 256; l++) {
    seen += histogram[l];
    if (seen >= total * 0.9) return l;
  }
  return 255;
}

export function loadImage(file: File): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const img = new Image();
    img.onload = () => {
      URL.revokeObjectURL(url);
      resolve(img);
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error("No se pudo leer la imagen"));
    };
    img.src = url;
  });
}
