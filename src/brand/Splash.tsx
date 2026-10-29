import { useEffect, useState } from "react";
import { BRAND, Isotype } from "./Logo";

const VISIBLE_MS = 1600;
const FADE_MS = 400;

/** Pantalla de carga al abrir la app: se muestra un momento y se desvanece. */
export function Splash() {
  const [phase, setPhase] = useState<"in" | "out" | "gone">("in");

  useEffect(() => {
    let fade: number | undefined;
    const show = window.setTimeout(() => {
      setPhase("out");
      fade = window.setTimeout(() => setPhase("gone"), FADE_MS);
    }, VISIBLE_MS);
    return () => {
      window.clearTimeout(show);
      window.clearTimeout(fade);
    };
  }, []);

  if (phase === "gone") return null;

  return (
    <div
      role="status"
      aria-label="Cargando Firmar Vecipp"
      className="fixed inset-0 z-[100] flex flex-col items-center justify-center transition-opacity ease-out"
      style={{
        background: `radial-gradient(circle at 50% 40%, ${BRAND.navy}, ${BRAND.navyDeep} 70%)`,
        opacity: phase === "out" ? 0 : 1,
        transitionDuration: `${FADE_MS}ms`,
      }}
    >
      <div className="splash-rise flex flex-col items-center">
        <Isotype className="h-28 w-28" />
        <span className="mt-5 font-[Nunito] text-2xl font-extrabold tracking-[0.2em] text-white">vecipp</span>
        <span className="mt-2 h-px w-44 bg-white/80" />
        <span className="-mt-1 font-['Great_Vibes'] text-8xl leading-tight text-white">firma</span>
      </div>

      <div className="absolute bottom-16 flex flex-col items-center gap-3">
        <div className="h-1 w-48 overflow-hidden rounded-full bg-white/10">
          <div className="splash-bar h-full w-1/3 rounded-full" style={{ background: BRAND.green }} />
        </div>
        <span className="text-sm tracking-wide text-slate-300">Cargando...</span>
      </div>
    </div>
  );
}
