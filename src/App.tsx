import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppMark } from "./brand/Logo";
import { Splash } from "./brand/Splash";
import { api } from "./lib/ipc";
import { BatchView } from "./features/batch/BatchView";
import { HistoryView } from "./features/history/HistoryView";
import { AboutView, TermsGate } from "./legal/LegalViews";
import { UpdateChecker } from "./features/updates/UpdateChecker";
import { IdentityView } from "./features/identity/IdentityView";
import { LockGate } from "./features/lock/AppLock";
import { SecurityView } from "./features/security/SecurityView";
import { SignerView } from "./features/signer/SignerView";
import { VaultView } from "./features/vault/VaultView";
import { useFirmarStore, type View } from "./store/useFirmarStore";

const NAV: { view: View; label: string; icon: string }[] = [
  { view: "signer", label: "Firmar", icon: "M4 20h16M6 16l9.5-9.5a2.1 2.1 0 0 1 3 3L9 19H6v-3Z" },
  { view: "batch", label: "Firmar varios", icon: "M8 4h10v14M5 7h10v13H5z" },
  { view: "history", label: "Historial", icon: "M12 7v5l3 2M3 12a9 9 0 1 0 3-6.7M3 4v4h4" },
  { view: "identity", label: "Identidad", icon: "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm-7 8a7 7 0 0 1 14 0" },
  { view: "vault", label: "Mis firmas", icon: "M5 11h14v9H5zM8 11V8a4 4 0 1 1 8 0v3" },
  { view: "security", label: "Seguridad", icon: "M12 3 5 6v5c0 4.5 3 8.5 7 10 4-1.5 7-5.5 7-10V6l-7-3Z" },
  { view: "about", label: "Acerca de", icon: "M12 8h.01M11 12h1v4h1M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Z" },
];

export default function App() {
  return (
    <>
      <TermsGate>
        <LockGate>
          <Shell />
        </LockGate>
        <UpdateChecker />
      </TermsGate>
      <Splash />
    </>
  );
}

function Shell() {
  const view = useFirmarStore((s) => s.view);
  const setView = useFirmarStore((s) => s.setView);
  const identity = useFirmarStore((s) => s.identity);
  const bootstrap = useFirmarStore((s) => s.bootstrap);
  const busy = useFirmarStore((s) => s.busy);

  useEffect(() => {
    bootstrap();
  }, [bootstrap]);

  // F11: pantalla completa sin bordes (la app arranca maximizada).
  useEffect(() => {
    const onKey = async (e: KeyboardEvent) => {
      if (e.key !== "F11") return;
      e.preventDefault();
      const win = getCurrentWindow();
      await win.setFullscreen(!(await win.isFullscreen()));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // «Firmar con Firmar» desde el Explorador: al arrancar, o reenviado por una
  // segunda instancia cuando la app ya estaba abierta.
  useEffect(() => {
    const { requestOpen } = useFirmarStore.getState();
    api.takeLaunchFile().then((path) => path && requestOpen(path));
    const unlisten = listen<string>("open-file", (e) => requestOpen(e.payload));
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <div className="flex h-full">
      <nav className="flex w-56 shrink-0 flex-col border-r border-ink-800 bg-ink-900 p-4">
        <div className="mb-8 flex items-center gap-2 px-2">
          <AppMark className="h-8 w-8" />
          <span className="text-lg font-semibold tracking-tight text-white">
            Firmar <span className="font-normal text-slate-400">Vecipp</span>
          </span>
        </div>
        <ul className="flex flex-col gap-1">
          {NAV.map((item) => (
            <li key={item.view}>
              <button
                type="button"
                onClick={() => setView(item.view)}
                className={`flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm ${view === item.view ? "bg-ink-800 text-white" : "text-slate-400 hover:bg-ink-800/60 hover:text-slate-200"}`}
              >
                <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
                  <path d={item.icon} />
                </svg>
                {item.label}
              </button>
            </li>
          ))}
        </ul>
        <div className="mt-auto space-y-1 px-2 text-xs text-slate-500">
          <p className="flex items-center gap-1.5">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" /> Tus datos no salen del equipo
          </p>
          {identity && (
            <p title={identity.vaultPath}>
              Identidad <span className="font-mono text-slate-400">{identity.fingerprint}</span>
            </p>
          )}
        </div>
      </nav>

      <main className="relative min-w-0 flex-1">
        {view === "signer" && <SignerView />}
        {view === "batch" && <BatchView />}
        {view === "history" && <HistoryView />}
        {view === "identity" && <IdentityView />}
        {view === "vault" && <VaultView />}
        {view === "security" && <SecurityView />}
        {view === "about" && <AboutView />}
        {busy && (
          <div className="absolute inset-0 z-30 flex items-center justify-center bg-ink-950/60 backdrop-blur-sm">
            <div className="flex items-center gap-3 rounded-xl border border-ink-700 bg-ink-900 px-5 py-3 text-sm text-slate-200">
              <span className="h-4 w-4 animate-spin rounded-full border-2 border-brand-400 border-t-transparent" />
              {busy}
            </div>
          </div>
        )}
      </main>

      <Notices />
    </div>
  );
}

function Notices() {
  const notices = useFirmarStore((s) => s.notices);
  const dismiss = useFirmarStore((s) => s.dismiss);
  const tone = { success: "border-emerald-500/50", error: "border-red-500/60", info: "border-ink-700" };
  return (
    <div className="pointer-events-none fixed right-5 bottom-5 z-40 flex w-96 flex-col gap-2">
      {notices.map((n) => (
        <div
          key={n.id}
          role="status"
          className={`pointer-events-auto rounded-xl border bg-ink-900 px-4 py-3 text-sm text-slate-200 shadow-2xl ${tone[n.tone]}`}
        >
          <div className="flex items-start gap-3">
            <p className="flex-1 select-text">{n.text}</p>
            <button type="button" aria-label="Cerrar" onClick={() => dismiss(n.id)} className="text-slate-500 hover:text-slate-200">
              ✕
            </button>
          </div>
          {n.actions && n.actions.length > 0 && (
            <div className="mt-2 flex gap-2">
              {n.actions.map((a) => (
                <button
                  key={a.label}
                  type="button"
                  onClick={() => {
                    a.run();
                    dismiss(n.id);
                  }}
                  className="rounded-md bg-ink-800 px-2.5 py-1 text-xs font-medium text-slate-100 hover:bg-ink-700"
                >
                  {a.label}
                </button>
              ))}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
