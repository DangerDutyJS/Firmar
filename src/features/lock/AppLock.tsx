import { useCallback, useEffect, useRef, useState } from "react";
import { create } from "zustand";
import { api, errorMessage, type LockView } from "../../lib/ipc";
import { useFirmarStore } from "../../store/useFirmarStore";

interface LockState {
  view: LockView | null;
  refresh: () => Promise<LockView | null>;
  lockNow: () => Promise<void>;
}

export const useLock = create<LockState>((set) => ({
  view: null,
  refresh: async () => {
    try {
      const view = await api.lockStatus();
      set({ view });
      return view;
    } catch {
      return null;
    }
  },
  lockNow: async () => {
    await api.lockNow();
    set((s) => (s.view ? { view: { ...s.view, locked: s.view.enabled } } : s));
  },
}));

const ACTIVITY_EVENTS = ["pointerdown", "keydown", "wheel", "pointermove"] as const;

/**
 * Muestra la pantalla de desbloqueo mientras el backend esté bloqueado y
 * vuelve a bloquear tras el tiempo de inactividad configurado.
 */
export function LockGate({ children }: { children: React.ReactNode }) {
  const view = useLock((s) => s.view);
  const refresh = useLock((s) => s.refresh);
  const lockNow = useLock((s) => s.lockNow);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Bloqueo por inactividad.
  const minutes = view?.enabled && !view.locked ? view.autoLockMinutes : 0;
  useEffect(() => {
    if (!minutes) return;
    let timer: ReturnType<typeof setTimeout>;
    const arm = () => {
      clearTimeout(timer);
      timer = setTimeout(() => lockNow(), minutes * 60_000);
    };
    arm();
    ACTIVITY_EVENTS.forEach((ev) => window.addEventListener(ev, arm, { passive: true }));
    return () => {
      clearTimeout(timer);
      ACTIVITY_EVENTS.forEach((ev) => window.removeEventListener(ev, arm));
    };
  }, [minutes, lockNow]);

  if (!view) return null;
  if (view.locked) return <LockScreen view={view} />;
  return <>{children}</>;
}

function LockScreen({ view }: { view: LockView }) {
  const refresh = useLock((s) => s.refresh);
  const [pin, setPin] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const triedHello = useRef(false);

  const unlockHello = useCallback(async () => {
    setError(null);
    setBusy(true);
    try {
      await api.lockUnlockHello();
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  // Con Windows Hello activado, se ofrece directamente al abrir.
  useEffect(() => {
    if (view.hello && view.helloAvailable && !triedHello.current) {
      triedHello.current = true;
      unlockHello();
    }
  }, [view.hello, view.helloAvailable, unlockHello]);

  const unlockPin = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await api.lockUnlockPin(pin);
      setPin("");
      await refresh();
    } catch (err) {
      setError(errorMessage(err));
      setPin("");
      refresh();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full items-center justify-center bg-ink-950 p-6">
      <form onSubmit={unlockPin} className="flex w-full max-w-sm flex-col items-center gap-4 rounded-2xl border border-ink-800 bg-ink-900 p-8">
        <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-brand-500 text-2xl text-white">🔒</div>
        <div className="text-center">
          <h1 className="text-lg font-semibold text-white">Firmar Vecipp está bloqueado</h1>
          <p className="mt-1 text-sm text-slate-400">Introduce tu PIN para continuar.</p>
        </div>
        <input
          type="password"
          inputMode="numeric"
          autoFocus
          value={pin}
          onChange={(e) => setPin(e.target.value)}
          placeholder="PIN"
          disabled={busy}
          className="w-full rounded-lg border border-ink-700 bg-ink-800 px-3 py-2.5 text-center text-lg tracking-[0.4em] text-white outline-none focus:border-brand-500"
        />
        {error && <p className="text-center text-sm text-red-400">{error}</p>}
        <button
          type="submit"
          disabled={busy || pin.length === 0}
          className="w-full rounded-lg bg-brand-500 px-4 py-2.5 text-sm font-medium text-white hover:bg-brand-600 disabled:opacity-40"
        >
          Desbloquear
        </button>
        {view.hello && view.helloAvailable && (
          <button type="button" onClick={unlockHello} disabled={busy} className="text-sm text-brand-400 hover:underline disabled:opacity-40">
            Usar Windows Hello
          </button>
        )}
      </form>
    </div>
  );
}

const AUTO_LOCK_OPTIONS = [
  { value: 0, label: "Solo al abrir la app" },
  { value: 5, label: "Tras 5 minutos sin uso" },
  { value: 15, label: "Tras 15 minutos sin uso" },
  { value: 30, label: "Tras 30 minutos sin uso" },
  { value: 60, label: "Tras 1 hora sin uso" },
];

/** Ajustes de bloqueo para la sección «Copia de seguridad y seguridad». */
export function LockSettings() {
  const notify = useFirmarStore((s) => s.notify);
  const view = useLock((s) => s.view);
  const refresh = useLock((s) => s.refresh);
  const lockNow = useLock((s) => s.lockNow);
  const [pin, setPin] = useState("");
  const [confirm, setConfirm] = useState("");
  const [changing, setChanging] = useState(false);

  const fail = (e: unknown) => notify("error", errorMessage(e));
  const ready = pin.length >= 4 && pin === confirm;

  const savePin = async () => {
    try {
      await api.lockSetPin(pin);
      setPin("");
      setConfirm("");
      setChanging(false);
      await refresh();
      notify("success", "Bloqueo activado. Firmar Vecipp pedirá el PIN cada vez que se abra.");
    } catch (e) {
      fail(e);
    }
  };

  const setOptions = async (hello: boolean, minutes: number) => {
    try {
      await api.lockSetOptions(hello, minutes);
      await refresh();
    } catch (e) {
      fail(e);
    }
  };

  const disable = async () => {
    try {
      await api.lockDisable();
      await refresh();
      notify("info", "Bloqueo desactivado.");
    } catch (e) {
      fail(e);
    }
  };

  if (!view) return null;

  return (
    <section className="flex flex-col gap-3 rounded-2xl border border-ink-800 bg-ink-900 p-5 lg:col-span-2">
      <div>
        <h3 className="font-semibold text-white">Bloqueo de la app</h3>
        <p className="mt-1 text-sm text-slate-400">
          Evita que otra persona firme con tu firma si usa tu computador con tu sesión de Windows abierta.
        </p>
      </div>

      {(!view.enabled || changing) && (
        <div className="flex flex-wrap items-end gap-2 text-sm">
          <label className="flex flex-col gap-1 text-slate-400">
            {view.enabled ? "PIN nuevo" : "PIN (mínimo 4 caracteres)"}
            <input
              type="password"
              inputMode="numeric"
              value={pin}
              onChange={(e) => setPin(e.target.value)}
              className="w-40 rounded-lg border border-ink-700 bg-ink-800 px-3 py-1.5 text-white outline-none focus:border-brand-500"
            />
          </label>
          <label className="flex flex-col gap-1 text-slate-400">
            Repite el PIN
            <input
              type="password"
              inputMode="numeric"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
              className="w-40 rounded-lg border border-ink-700 bg-ink-800 px-3 py-1.5 text-white outline-none focus:border-brand-500"
            />
          </label>
          <button
            type="button"
            disabled={!ready}
            onClick={savePin}
            className="rounded-lg bg-brand-500 px-4 py-1.5 font-medium text-white hover:bg-brand-600 disabled:opacity-40"
          >
            {view.enabled ? "Cambiar PIN" : "Activar bloqueo"}
          </button>
          {changing && (
            <button type="button" onClick={() => setChanging(false)} className="px-2 py-1.5 text-slate-400 hover:text-slate-200">
              Cancelar
            </button>
          )}
        </div>
      )}

      {view.enabled && (
        <div className="flex flex-col gap-3 text-sm">
          <label className="flex items-center gap-2 text-slate-300">
            <input
              type="checkbox"
              checked={view.hello}
              disabled={!view.helloAvailable}
              onChange={(e) => setOptions(e.target.checked, view.autoLockMinutes)}
              className="accent-brand-500"
            />
            Desbloquear con Windows Hello (huella, rostro o PIN de Windows)
            {!view.helloAvailable && <span className="text-xs text-slate-500">· no está configurado en este equipo</span>}
          </label>
          <label className="flex items-center gap-2 text-slate-400">
            Bloquear
            <select
              value={view.autoLockMinutes}
              onChange={(e) => setOptions(view.hello, Number(e.target.value))}
              className="rounded-lg border border-ink-700 bg-ink-800 px-2 py-1 text-slate-200 outline-none focus:border-brand-500"
            >
              {AUTO_LOCK_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>
          <div className="flex gap-3">
            <button type="button" onClick={() => lockNow()} className="rounded-lg border border-ink-700 px-3 py-1.5 text-slate-200 hover:bg-ink-800">
              Bloquear ahora
            </button>
            {!changing && (
              <button type="button" onClick={() => setChanging(true)} className="px-2 text-slate-400 hover:text-slate-200">
                Cambiar PIN
              </button>
            )}
            <button type="button" onClick={disable} className="px-2 text-slate-500 hover:text-red-400">
              Desactivar bloqueo
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
