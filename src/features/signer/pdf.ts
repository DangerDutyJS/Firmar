import { GlobalWorkerOptions, getDocument, type PDFDocumentLoadingTask, type PDFDocumentProxy } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

GlobalWorkerOptions.workerSrc = workerUrl;

const assets = `${import.meta.env.BASE_URL}pdfjs/`;

/** Devuelve la tarea de carga para poder destruirla (y liberar el worker) al cerrar. */
export function loadPdf(bytes: Uint8Array): PDFDocumentLoadingTask {
  // pdf.js transfiere el buffer al worker; pasamos una copia para conservar el original.
  return getDocument({
    data: bytes.slice(),
    cMapUrl: `${assets}cmaps/`,
    cMapPacked: true,
    standardFontDataUrl: `${assets}standard_fonts/`,
    iccUrl: `${assets}iccs/`,
    wasmUrl: `${assets}wasm/`,
    enableXfa: false,
  });
}

export interface PageSize {
  /** Tamaño en puntos de la página tal como se ve (ya rotada). */
  width: number;
  height: number;
}

export async function pageSizes(pdf: PDFDocumentProxy): Promise<PageSize[]> {
  const sizes: PageSize[] = [];
  for (let i = 1; i <= pdf.numPages; i++) {
    const vp = (await pdf.getPage(i)).getViewport({ scale: 1 });
    sizes.push({ width: vp.width, height: vp.height });
  }
  return sizes;
}
