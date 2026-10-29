import { useEffect, useRef, useState } from "react";
import { useFirmarStore } from "../../store/useFirmarStore";
import {
  CAPTION_MAX_LINE_CHARS,
  CAPTION_MAX_LINES,
  CAPTION_TOKENS,
  SUGGESTED_LINES,
  buildCaption,
  usesName,
  type CaptionAlign,
  type CaptionSettings,
} from "./caption";
import { resizeForCaption } from "./placement";

/** Texto y alineación que llevarán las firmas, ya con los campos sustituidos. */
export function useStamp(): { caption: string | null; captionAlign: CaptionAlign } {
  const settings = useFirmarStore((s) => s.caption);
  const name = useFirmarStore((s) => s.myName);
  return { caption: buildCaption(settings, name), captionAlign: settings.align };
}

const ALIGNS: { value: CaptionAlign; label: string; icon: string }[] = [
  { value: "left", label: "Izquierda", icon: "M4 6h16M4 10h10M4 14h16M4 18h10" },
  { value: "center", label: "Centro", icon: "M4 6h16M7 10h10M4 14h16M7 18h10" },
  { value: "right", label: "Derecha", icon: "M4 6h16M10 10h10M4 14h16M10 18h10" },
];

/**
 * Botón «Texto» con su editor: líneas libres con campos ({nombre}, {fecha}…),
 * reordenables, y alineación. `inline`: el editor se abre debajo, a todo el
 * ancho, en vez de flotar (para columnas laterales donde se recortaría).
 */
export function CaptionPanel({ inline = false, summary }: { inline?: boolean; summary?: string }) {
  const saved = useFirmarStore((s) => s.caption);
  const myName = useFirmarStore((s) => s.myName);
  const setCaption = useFirmarStore((s) => s.setCaption);
  const setView = useFirmarStore((s) => s.setView);
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<CaptionSettings>(saved);
  const [focused, setFocused] = useState(0);
  const panelRef = useRef<HTMLDivElement>(null);
  const inputs = useRef<(HTMLInputElement | null)[]>([]);

  useEffect(() => {
    if (!open) return;
    setDraft(saved.lines.length ? saved : { ...saved, lines: [...SUGGESTED_LINES] });
    const close = (e: MouseEvent) => {
      if (!panelRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open, saved]);

  const preview = buildCaption(draft, myName);
  const active = buildCaption(saved, myName) !== null;

  const apply = (settings: CaptionSettings) => {
    const caption = buildCaption(settings, myName);
    setCaption(settings, { caption, captionAlign: settings.align }, (p) => resizeForCaption(p, caption));
    setOpen(false);
  };

  const setLines = (lines: string[]) => setDraft((d) => ({ ...d, lines }));
  const updateLine = (i: number, value: string) => setLines(draft.lines.map((l, j) => (j === i ? value : l)));
  const removeLine = (i: number) => {
    setLines(draft.lines.filter((_, j) => j !== i));
    setFocused(Math.max(0, i - 1));
  };
  const moveLine = (i: number, delta: number) => {
    const j = i + delta;
    if (j < 0 || j >= draft.lines.length) return;
    const lines = [...draft.lines];
    [lines[i], lines[j]] = [lines[j], lines[i]];
    setLines(lines);
    setFocused(j);
  };
  const addLine = () => {
    if (draft.lines.length >= CAPTION_MAX_LINES) return;
    setLines([...draft.lines, ""]);
    setFocused(draft.lines.length);
    requestAnimationFrame(() => inputs.current[draft.lines.length]?.focus());
  };

  /** Inserta un campo donde está el cursor de la línea activa. */
  const insertToken = (token: string) => {
    if (draft.lines.length === 0) {
      setLines([token]);
      return;
    }
    const i = Math.min(focused, draft.lines.length - 1);
    const input = inputs.current[i];
    const line = draft.lines[i];
    const start = input?.selectionStart ?? line.length;
    const end = input?.selectionEnd ?? line.length;
    const next = (line.slice(0, start) + token + line.slice(end)).slice(0, CAPTION_MAX_LINE_CHARS);
    updateLine(i, next);
    requestAnimationFrame(() => {
      input?.focus();
      input?.setSelectionRange(start + token.length, start + token.length);
    });
  };

  return (
    <div className={inline ? "w-full" : "relative"} ref={panelRef}>
      <div className="flex items-center gap-3">
        {summary !== undefined && <span className="min-w-0 flex-1 text-sm text-slate-400">{summary}</span>}
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className={`shrink-0 rounded-lg border px-3 py-2 text-sm whitespace-nowrap ${active ? "border-brand-500 text-brand-400" : "border-ink-700 text-slate-200 hover:bg-ink-800"}`}
          title="Añadir nombre, fecha o texto bajo la firma"
        >
          Aa Texto{active ? " ✓" : ""}
        </button>
      </div>
      {open && (
        <div
          className={`space-y-3 rounded-xl border border-ink-700 bg-ink-900 p-4 text-sm shadow-2xl ${
            inline ? "mt-3 w-full" : "absolute top-full right-0 z-30 mt-2 w-[26rem]"
          }`}
        >
          <div className="flex items-center justify-between">
            <p className="font-medium text-white">Texto del sello</p>
            <div className="flex rounded-lg border border-ink-700">
              {ALIGNS.map((a) => (
                <button
                  key={a.value}
                  type="button"
                  title={`Alinear a la ${a.label.toLowerCase()}`}
                  aria-label={a.label}
                  onClick={() => setDraft((d) => ({ ...d, align: a.value }))}
                  className={`px-2 py-1.5 ${draft.align === a.value ? "bg-ink-700 text-white" : "text-slate-400 hover:text-slate-200"}`}
                >
                  <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
                    <path d={a.icon} />
                  </svg>
                </button>
              ))}
            </div>
          </div>

          <ol className="space-y-2">
            {draft.lines.map((line, i) => (
              <li key={i} className="flex items-center gap-1.5">
                <span className="w-4 text-right text-xs text-slate-600">{i + 1}</span>
                <input
                  ref={(el) => {
                    inputs.current[i] = el;
                  }}
                  value={line}
                  maxLength={CAPTION_MAX_LINE_CHARS}
                  onFocus={() => setFocused(i)}
                  onChange={(e) => updateLine(i, e.target.value)}
                  placeholder="Escribe o inserta un campo"
                  className={`min-w-0 flex-1 rounded-lg border bg-ink-800 px-2.5 py-1.5 text-white outline-none focus:border-brand-500 ${focused === i ? "border-brand-500/50" : "border-ink-700"}`}
                />
                <button type="button" aria-label="Subir línea" title="Subir" disabled={i === 0} onClick={() => moveLine(i, -1)} className="px-1 text-slate-400 hover:text-white disabled:opacity-20">
                  ↑
                </button>
                <button type="button" aria-label="Bajar línea" title="Bajar" disabled={i === draft.lines.length - 1} onClick={() => moveLine(i, 1)} className="px-1 text-slate-400 hover:text-white disabled:opacity-20">
                  ↓
                </button>
                <button type="button" aria-label="Quitar línea" title="Quitar" onClick={() => removeLine(i)} className="px-1 text-slate-500 hover:text-red-400">
                  ✕
                </button>
              </li>
            ))}
          </ol>

          <div className="flex flex-wrap items-center gap-1.5">
            <button
              type="button"
              onClick={addLine}
              disabled={draft.lines.length >= CAPTION_MAX_LINES}
              className="rounded-md border border-dashed border-ink-600 px-2.5 py-1 text-xs text-slate-300 hover:border-brand-500 disabled:opacity-30"
            >
              + Línea
            </button>
            <span className="ml-1 text-xs text-slate-500">Insertar:</span>
            {CAPTION_TOKENS.map((t) => (
              <button
                key={t.token}
                type="button"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => insertToken(t.token)}
                className="rounded-md bg-ink-800 px-2 py-1 text-xs text-brand-400 hover:bg-ink-700"
              >
                {t.label}
              </button>
            ))}
          </div>

          {usesName(draft) && !myName && (
            <p className="text-xs text-amber-300">
              {"{nombre}"} quedará vacío hasta que pongas tu nombre en{" "}
              <button type="button" onClick={() => setView("identity")} className="underline">
                Identidad
              </button>
              .
            </p>
          )}

          <div
            className="rounded-lg bg-white px-3 py-2 text-xs leading-5 whitespace-pre-line text-slate-800"
            style={{ fontFamily: "Arial, sans-serif", textAlign: draft.align }}
          >
            {preview ?? <span className="text-slate-400">Sin texto: solo se estampará la firma</span>}
          </div>
          <p className="text-xs text-slate-500">
            Se aplica a todas las firmas de este documento y a las siguientes. Máximo {CAPTION_MAX_LINES} líneas.
          </p>
          <div className="flex items-center justify-between gap-2">
            <button
              type="button"
              onClick={() => apply({ ...draft, lines: [] })}
              className="text-xs text-slate-500 hover:text-red-400"
            >
              Quitar todo el texto
            </button>
            <div className="flex gap-2">
              <button type="button" onClick={() => setOpen(false)} className="px-3 py-1.5 text-slate-400 hover:text-slate-200">
                Cancelar
              </button>
              <button
                type="button"
                onClick={() => apply({ ...draft, lines: draft.lines.filter((l) => l.trim()) })}
                className="rounded-lg bg-brand-500 px-3 py-1.5 font-medium text-white hover:bg-brand-600"
              >
                Aplicar
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
