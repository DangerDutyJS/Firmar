import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { api, errorMessage } from "../lib/ipc";
import { useFirmarStore } from "../store/useFirmarStore";
import { LEGAL_ENTITY, PRIVACY, TERMS, TERMS_VERSION, type LegalSection } from "./legal";
import { UpdateSettings } from "../features/updates/UpdateChecker";

function Sections({ sections }: { sections: LegalSection[] }) {
  return (
    <div className="space-y-5 text-sm leading-relaxed text-slate-300 select-text">
      {sections.map((s) => (
        <section key={s.title}>
          <h4 className="mb-1.5 font-semibold text-white">{s.title}</h4>
          {s.paragraphs.map((p, i) => (
            <p key={i} className="mb-2">
              {p}
            </p>
          ))}
        </section>
      ))}
    </div>
  );
}

/**
 * Bloquea la app hasta que se aceptan los términos vigentes. Se muestra en el
 * primer uso y cada vez que cambia `TERMS_VERSION`.
 */
export function TermsGate({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<"loading" | "pending" | "accepted">("loading");
  const [tab, setTab] = useState<"terms" | "privacy">("terms");
  const [checked, setChecked] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .legalStatus()
      .then((s) => setState(s.acceptedVersion === TERMS_VERSION ? "accepted" : "pending"))
      .catch(() => setState("pending"));
  }, []);

  if (state === "accepted") return <>{children}</>;
  if (state === "loading") return null;

  const accept = async () => {
    try {
      await api.legalAccept(TERMS_VERSION);
      setState("accepted");
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="flex h-full items-center justify-center bg-ink-950 p-6">
      <div className="flex max-h-full w-full max-w-3xl flex-col gap-4 rounded-2xl border border-ink-800 bg-ink-900 p-6">
        <header>
          <h1 className="text-xl font-semibold text-white">Bienvenido a Firmar Vecipp</h1>
          <p className="mt-1 text-sm text-slate-400">
            Antes de empezar, lee y acepta los términos de uso y la política de privacidad.
          </p>
        </header>
        <div className="rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-100">
          <strong>En resumen:</strong> Firmar Vecipp crea una <em>firma electrónica simple</em> y detecta si un documento cambió
          después de firmarlo. No es una firma digital certificada. Tus firmas y documentos se quedan en tu equipo: no recibimos ningún dato.
        </div>
        <div className="flex gap-1 text-sm">
          {(["terms", "privacy"] as const).map((t) => (
            <button
              key={t}
              type="button"
              onClick={() => setTab(t)}
              className={`rounded-lg px-3 py-1.5 ${tab === t ? "bg-ink-700 text-white" : "text-slate-400 hover:text-slate-200"}`}
            >
              {t === "terms" ? "Términos de uso" : "Política de privacidad"}
            </button>
          ))}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto rounded-xl border border-ink-800 bg-ink-950/60 p-4">
          <Sections sections={tab === "terms" ? TERMS : PRIVACY} />
        </div>
        <label className="flex items-start gap-2 text-sm text-slate-300">
          <input type="checkbox" checked={checked} onChange={(e) => setChecked(e.target.checked)} className="mt-0.5 accent-brand-500" />
          He leído y acepto los términos de uso y la política de privacidad, y entiendo que Firmar Vecipp produce una firma
          electrónica simple, no una firma digital certificada.
        </label>
        {error && <p className="text-sm text-red-400">{error}</p>}
        <button
          type="button"
          disabled={!checked}
          onClick={accept}
          className="self-end rounded-lg bg-brand-500 px-5 py-2 text-sm font-medium text-white hover:bg-brand-600 disabled:cursor-not-allowed disabled:opacity-40"
        >
          Aceptar y continuar
        </button>
      </div>
    </div>
  );
}

type AboutTab = "about" | "terms" | "privacy" | "licenses";

/** «Acerca de»: versión, aviso legal, privacidad y licencias de terceros. */
export function AboutView() {
  const identity = useFirmarStore((s) => s.identity);
  const [tab, setTab] = useState<AboutTab>("about");
  const [version, setVersion] = useState("");
  const [licenses, setLicenses] = useState<string | null>(null);
  const [accepted, setAccepted] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setVersion).catch(() => undefined);
    api.legalStatus().then((s) => setAccepted(s.acceptedAt)).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (tab !== "licenses" || licenses !== null) return;
    fetch("/third-party-licenses.txt")
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error(String(r.status)))))
      .then(setLicenses)
      .catch(() => setLicenses("No se encontró el archivo de licencias. Se genera con «npm run build»."));
  }, [tab, licenses]);

  const TABS: [AboutTab, string][] = [
    ["about", "Acerca de"],
    ["terms", "Términos de uso"],
    ["privacy", "Privacidad"],
    ["licenses", "Licencias"],
  ];

  return (
    <div className="mx-auto flex h-full max-w-4xl flex-col gap-4 p-6">
      <div className="flex gap-1 text-sm">
        {TABS.map(([t, label]) => (
          <button
            key={t}
            type="button"
            onClick={() => setTab(t)}
            className={`rounded-lg px-3 py-1.5 ${tab === t ? "bg-ink-700 text-white" : "text-slate-400 hover:text-slate-200"}`}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto rounded-2xl border border-ink-800 bg-ink-900 p-6">
        {tab === "about" && (
          <div className="space-y-5 text-sm text-slate-300">
            <div className="flex items-center gap-3">
              <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-brand-500 text-xl font-bold text-white">F</div>
              <div>
                <h2 className="text-lg font-semibold text-white">Firmar Vecipp {version && `v${version}`}</h2>
                <p className="text-slate-400">
                  {LEGAL_ENTITY.company} · {LEGAL_ENTITY.country}
                </p>
              </div>
            </div>
            <div className="rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-amber-100">
              <p className="font-semibold">Aviso legal</p>
              <p className="mt-1">
                Firmar Vecipp produce una firma electrónica simple (Ley 527 de 1999 y Decreto 2364 de 2012). No es una firma
                digital certificada por una entidad acreditada ante el ONAC ni verifica la identidad del firmante. Antes
                de usarla en un trámite, confirma que este tipo de firma es suficiente.
              </p>
            </div>
            <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2">
              <dt className="text-slate-500">Responsable</dt>
              <dd>{LEGAL_ENTITY.owner}</dd>
              <dt className="text-slate-500">Contacto</dt>
              <dd className="select-text">{LEGAL_ENTITY.email}</dd>
              <dt className="text-slate-500">Tu identidad</dt>
              <dd className="font-mono select-text">{identity?.fingerprint ?? "—"}</dd>
              <dt className="text-slate-500">Datos en</dt>
              <dd className="font-mono break-all select-text">{identity?.vaultPath ?? "—"}</dd>
              <dt className="text-slate-500">Términos aceptados</dt>
              <dd>
                versión {TERMS_VERSION}
                {accepted && ` · ${new Date(accepted).toLocaleString()}`}
              </dd>
            </dl>
            <UpdateSettings />
            <p className="text-slate-500">
              Firma y verifica sin conexión; solo usa internet para buscar actualizaciones. Criptografía: Ed25519,
              SHA-256, AES-256-GCM y Argon2id.
            </p>
          </div>
        )}
        {tab === "terms" && <Sections sections={TERMS} />}
        {tab === "privacy" && <Sections sections={PRIVACY} />}
        {tab === "licenses" && (
          <pre className="text-xs leading-relaxed whitespace-pre-wrap text-slate-400 select-text">
            {licenses ?? "Cargando…"}
          </pre>
        )}
      </div>
    </div>
  );
}

const WIPE_WORD = "BORRAR";

/** Derecho de supresión: elimina todo lo que Firmar Vecipp guarda en este equipo. */
export function WipeCard() {
  const notify = useFirmarStore((s) => s.notify);
  const setBusy = useFirmarStore((s) => s.setBusy);
  const bootstrap = useFirmarStore((s) => s.bootstrap);
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");

  const wipe = async () => {
    setBusy("Borrando datos…");
    try {
      await api.dataWipe(text);
      useFirmarStore.getState().setDocument(null);
      setOpen(false);
      setText("");
      await bootstrap();
      notify("info", "Se borraron todos tus datos de Firmar Vecipp en este equipo.");
    } catch (e) {
      notify("error", errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="flex flex-col gap-3 rounded-2xl border border-red-500/30 bg-ink-900 p-5 lg:col-span-2">
      <div>
        <h3 className="font-semibold text-white">Borrar todos mis datos</h3>
        <p className="mt-1 text-sm text-slate-400">
          Elimina tus firmas, las copias de los originales, el historial y tus claves del llavero. Los documentos ya
          firmados siguen siendo verificables, pero no podrás re-posicionar su firma. No se puede deshacer: crea antes
          una copia de seguridad si podrías necesitarlos.
        </p>
      </div>
      {open ? (
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span className="text-slate-300">
            Escribe <strong className="font-mono text-red-300">{WIPE_WORD}</strong> para confirmar:
          </span>
          <input
            value={text}
            onChange={(e) => setText(e.target.value)}
            className="w-32 rounded-lg border border-ink-700 bg-ink-800 px-3 py-1.5 font-mono text-white outline-none focus:border-red-500"
          />
          <button
            type="button"
            disabled={text.trim() !== WIPE_WORD}
            onClick={wipe}
            className="rounded-lg bg-red-600 px-4 py-1.5 font-medium text-white hover:bg-red-500 disabled:opacity-40"
          >
            Borrar definitivamente
          </button>
          <button type="button" onClick={() => setOpen(false)} className="px-2 text-slate-400 hover:text-slate-200">
            Cancelar
          </button>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setOpen(true)}
          className="self-start rounded-lg border border-red-500/40 px-4 py-2 text-sm text-red-300 hover:bg-red-500/10"
        >
          Borrar todos mis datos…
        </button>
      )}
    </section>
  );
}
