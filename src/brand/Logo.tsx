// Isotipo Vecipp. Misma geometría que src-tauri/icons/app-icon.svg, de donde
// salen los íconos de la aplicación (`npx tauri icon src-tauri/icons/app-icon.svg`).

export const BRAND = {
  navy: "#1e3246",
  navyDeep: "#112535",
  green: "#25c15b",
};

export function Isotype({ className }: { className?: string }) {
  return (
    <svg viewBox="14 20 328 290" className={className} aria-hidden="true">
      <path d="M24 64h64l72 176 44-108h56l-72 168h-56Z" fill="#ffffff" />
      <path d="M204 112 240 30l36 82Z" fill={BRAND.green} />
      <path d="M284 128l48 42-48 42Z" fill="#ffffff" />
    </svg>
  );
}

/** El isotipo sobre su cuadro azul, como el ícono de la aplicación. */
export function AppMark({ className = "" }: { className?: string }) {
  return (
    <div className={`flex shrink-0 items-center justify-center rounded-[22%] p-[18%] ${className}`} style={{ background: BRAND.navy }}>
      <Isotype className="h-full w-full" />
    </div>
  );
}
