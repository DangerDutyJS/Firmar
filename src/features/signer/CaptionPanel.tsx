import { useEffect, useRef, useState } from "react";
import { useFirmarStore } from "../../store/useFirmarStore";
import { buildCaption, type CaptionSettings } from "./caption";
import { resizeForCaption } from "./placement";

/** Botón «Texto» con su panel: nombre, fecha y texto libre bajo la firma. */
export function CaptionPanel() {
  const saved = useFirmarStore((s) => s.caption);
  const setCaption = useFirmarStore((s) => s.setCaption);
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<CaptionSettings>(saved);
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    setDraft(saved);
    const close = (e: MouseEvent) => {
      if (!panelRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open, saved]);

  const preview = buildCaption(draft);
  const active = buildCaption(saved) !== null;

  const apply = () => {
    const caption = buildCaption(draft);
    setCaption(draft, caption, (p) => resizeForCaption(p, caption));
    setOpen(false);
  };

  const field = (patch: Partial<CaptionSettings>) => setDraft((d) => ({ ...d, ...patch }));

  return (
    <div className="relative" ref={panelRef}>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className={`rounded-lg border px-3 py-2 text-sm ${active ? "border-brand-500 text-brand-400" : "border-ink-700 text-slate-200 hover:bg-ink-800"}`}
        title="Añadir nombre, fecha o texto bajo la firma"
      >
        Aa Texto{active ? " ✓" : ""}
      </button>
      {open && (
        <div className="absolute top-full right-0 z-30 mt-2 w-80 space-y-3 rounded-xl border border-ink-700 bg-ink-900 p-4 text-sm shadow-2xl">
          <p className="font-medium text-white">Texto bajo la firma</p>
          <label className="flex items-center gap-2 text-slate-300">
            <input type="checkbox" checked={draft.includeName} onChange={(e) => field({ includeName: e.target.checked })} className="accent-brand-500" />
            Mi nombre
          </label>
          {draft.includeName && (
            <input
              value={draft.name}
              onChange={(e) => field({ name: e.target.value })}
              maxLength={80}
              placeholder="Nombre y apellidos"
              className="w-full rounded-lg border border-ink-700 bg-ink-800 px-3 py-1.5 text-white outline-none focus:border-brand-500"
            />
          )}
          <label className="flex items-center gap-2 text-slate-300">
            <input type="checkbox" checked={draft.includeDate} onChange={(e) => field({ includeDate: e.target.checked })} className="accent-brand-500" />
            Fecha de firma
          </label>
          <input
            value={draft.custom}
            onChange={(e) => field({ custom: e.target.value })}
            maxLength={80}
            placeholder="Otro texto (cargo, C.I., «Aprobado»…)"
            className="w-full rounded-lg border border-ink-700 bg-ink-800 px-3 py-1.5 text-white outline-none focus:border-brand-500"
          />
          <div className="rounded-lg bg-white px-3 py-2 text-center text-xs leading-5 whitespace-pre-line text-slate-800" style={{ fontFamily: "Arial, sans-serif" }}>
            {preview ?? <span className="text-slate-400">Sin texto</span>}
          </div>
          <p className="text-xs text-slate-500">Se aplica a todas las firmas de este documento y a las siguientes.</p>
          <div className="flex justify-end gap-2">
            <button type="button" onClick={() => setOpen(false)} className="px-3 py-1.5 text-slate-400 hover:text-slate-200">
              Cancelar
            </button>
            <button type="button" onClick={apply} className="rounded-lg bg-brand-500 px-3 py-1.5 font-medium text-white hover:bg-brand-600">
              Aplicar
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
