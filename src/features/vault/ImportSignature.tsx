import { useEffect, useRef, useState } from "react";
import { loadImage, removeBackground, removeEdgeFrames, trimToInk } from "./imageProcessing";

interface Props {
  onResult: (pngDataUrl: string | null) => void;
}

const INK_OPTIONS: { label: string; value: [number, number, number] | null }[] = [
  { label: "Original", value: null },
  { label: "Negro", value: [17, 24, 39] },
  { label: "Azul", value: [29, 53, 160] },
];

export function ImportSignature({ onResult }: Props) {
  const [image, setImage] = useState<HTMLImageElement | null>(null);
  const [sensitivity, setSensitivity] = useState(55);
  const [ink, setInk] = useState(0);
  const [stripFrames, setStripFrames] = useState(true);
  const [preview, setPreview] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!image) return;
    const handle = requestAnimationFrame(() => {
      const cleaned = removeBackground(image, { sensitivity, inkColor: INK_OPTIONS[ink].value });
      if (stripFrames) removeEdgeFrames(cleaned);
      const url = trimToInk(cleaned, 10)?.toDataURL("image/png") ?? null;
      setPreview(url);
      onResult(url);
    });
    return () => cancelAnimationFrame(handle);
  }, [image, sensitivity, ink, stripFrames, onResult]);

  const pick = async (file: File | undefined) => {
    if (!file) return;
    setError(null);
    try {
      setImage(await loadImage(file));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  if (!image) {
    return (
      <label
        className="flex h-full cursor-pointer flex-col items-center justify-center gap-3 rounded-xl border-2 border-dashed border-ink-700 bg-ink-900/40 text-slate-400 transition hover:border-brand-500 hover:text-slate-200"
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          pick(e.dataTransfer.files[0]);
        }}
      >
        <svg viewBox="0 0 24 24" className="h-10 w-10" fill="none" stroke="currentColor" strokeWidth={1.5}>
          <path d="M4 16v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2M12 4v12m0-12-4 4m4-4 4 4" />
        </svg>
        <span className="text-sm">Arrastra una foto o escaneo de tu firma, o haz clic para elegirla</span>
        <span className="text-xs text-slate-500">PNG o JPG · el fondo se elimina en tu equipo</span>
        {error && <span className="text-xs text-red-400">{error}</span>}
        <input
          ref={inputRef}
          type="file"
          accept="image/png,image/jpeg,image/webp"
          className="hidden"
          onChange={(e) => pick(e.target.files?.[0])}
        />
      </label>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4">
      <div className="checkerboard flex min-h-0 flex-1 items-center justify-center rounded-xl p-6">
        {preview ? (
          <img src={preview} alt="Vista previa" className="max-h-full max-w-full object-contain" />
        ) : (
          <span className="text-sm text-slate-500">No se detectó tinta: sube la sensibilidad</span>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-6 text-sm">
        <label className="flex items-center gap-3">
          <span className="text-slate-400">Sensibilidad</span>
          <input
            type="range"
            min={0}
            max={100}
            value={sensitivity}
            onChange={(e) => setSensitivity(Number(e.target.value))}
            className="accent-brand-500"
          />
        </label>
        <div className="flex items-center gap-2">
          <span className="text-slate-400">Tinta</span>
          {INK_OPTIONS.map((o, i) => (
            <button
              key={o.label}
              type="button"
              onClick={() => setInk(i)}
              className={`rounded-md px-2.5 py-1 text-xs ${i === ink ? "bg-brand-500 text-white" : "bg-ink-800 text-slate-300 hover:bg-ink-700"}`}
            >
              {o.label}
            </button>
          ))}
        </div>
        <label className="flex items-center gap-2 text-slate-400" title="Elimina recuadros y líneas pegadas al borde de la imagen">
          <input
            type="checkbox"
            checked={stripFrames}
            onChange={(e) => setStripFrames(e.target.checked)}
            className="accent-brand-500"
          />
          Quitar marcos y bordes
        </label>
        <button
          type="button"
          onClick={() => {
            setImage(null);
            setPreview(null);
            onResult(null);
          }}
          className="ml-auto text-xs text-slate-400 hover:text-slate-200"
        >
          Elegir otra imagen
        </button>
      </div>
    </div>
  );
}
