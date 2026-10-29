import { useCallback, useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api, errorMessage, type ContactView, type MyIdentity } from "../../lib/ipc";
import { useFirmarStore } from "../../store/useFirmarStore";

const CARD_EXTENSION = "firmarid";

/** Tu nombre público, tu tarjeta de identidad y tus contactos de confianza. */
export function IdentityView() {
  const notify = useFirmarStore((s) => s.notify);
  const [me, setMe] = useState<MyIdentity | null>(null);
  const [name, setName] = useState("");
  const [contacts, setContacts] = useState<ContactView[]>([]);
  const [editing, setEditing] = useState<string | null>(null);
  const [editName, setEditName] = useState("");

  const load = useCallback(async () => {
    try {
      const [identity, list] = await Promise.all([api.identityGet(), api.contactsList()]);
      setMe(identity);
      setName(identity.name ?? "");
      setContacts(list);
    } catch (e) {
      notify("error", errorMessage(e));
    }
  }, [notify]);

  useEffect(() => {
    load();
  }, [load]);

  const fail = (e: unknown) => notify("error", errorMessage(e));

  const saveName = async () => {
    try {
      const updated = await api.identitySetName(name.trim() || null);
      setMe(updated);
      useFirmarStore.getState().setMyName(updated.name);
      notify("success", name.trim() ? "Tu nombre aparecerá en el sello de los documentos que firmes." : "Nombre quitado.");
    } catch (e) {
      fail(e);
    }
  };

  const exportCard = async () => {
    const safe = (me?.name ?? "mi-identidad").replace(/[^\p{L}\p{N} _-]/gu, "").trim() || "mi-identidad";
    const path = await save({
      defaultPath: `${safe}.${CARD_EXTENSION}`,
      filters: [{ name: "Tarjeta de identidad de Firmar Vecipp", extensions: [CARD_EXTENSION] }],
    });
    if (!path) return;
    try {
      await api.identityExportCard(path);
      notify("success", "Tarjeta guardada. Envíala a quienes reciban tus documentos.", [
        { label: "Mostrar en carpeta", run: () => api.revealPath(path).catch(fail) },
      ]);
    } catch (e) {
      fail(e);
    }
  };

  const importCard = async () => {
    const selected = await open({
      multiple: true,
      filters: [{ name: "Tarjeta de identidad de Firmar Vecipp", extensions: [CARD_EXTENSION] }],
    });
    const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
    for (const path of paths) {
      try {
        const c = await api.contactImport(path);
        notify("success", `${c.name} se añadió a tus contactos de confianza.`);
      } catch (e) {
        fail(e);
      }
    }
    if (paths.length) load();
  };

  const rename = async (fingerprint: string) => {
    try {
      await api.contactRename(fingerprint, editName);
      setEditing(null);
      load();
    } catch (e) {
      fail(e);
    }
  };

  const remove = async (c: ContactView) => {
    try {
      await api.contactRemove(c.fingerprint);
      load();
    } catch (e) {
      fail(e);
    }
  };

  const nameChanged = (me?.name ?? "") !== name.trim();

  return (
    <div className="mx-auto flex h-full max-w-5xl flex-col gap-6 overflow-y-auto p-6">
      <header>
        <h2 className="text-lg font-semibold text-white">Identidad</h2>
        <p className="mt-1 max-w-3xl text-sm text-slate-400">
          Tu nombre va dentro del sello de cada documento que firmas. Para que otras personas puedan comprobar que de
          verdad fuiste tú, compárteles tu tarjeta de identidad. Si ellas te envían la suya, impórtala y sus documentos
          aparecerán como <strong className="text-emerald-300">contacto verificado</strong>.
        </p>
      </header>

      <div className="grid gap-6 lg:grid-cols-2">
        <section className="flex flex-col gap-4 rounded-2xl border border-ink-800 bg-ink-900 p-5">
          <h3 className="font-semibold text-white">Mi identidad</h3>
          <label className="flex flex-col gap-1 text-sm text-slate-400">
            Nombre que verán quienes reciban tus documentos
            <div className="flex gap-2">
              <input
                value={name}
                onChange={(e) => setName(e.target.value)}
                maxLength={80}
                placeholder="Nombre y apellidos"
                className="flex-1 rounded-lg border border-ink-700 bg-ink-800 px-3 py-2 text-white outline-none focus:border-brand-500"
              />
              <button
                type="button"
                onClick={saveName}
                disabled={!nameChanged}
                className="rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white hover:bg-brand-600 disabled:opacity-40"
              >
                Guardar
              </button>
            </div>
          </label>
          <div>
            <p className="text-sm text-slate-400">Tu código de seguridad</p>
            <p className="mt-1 font-mono text-2xl tracking-wider text-white select-text">{me?.fingerprint ?? "…"}</p>
            <p className="mt-1 text-xs text-slate-500">
              Si alguien quiere asegurarse de que la tarjeta es tuya, puede comparar este código contigo por teléfono o en
              persona.
            </p>
          </div>
          <button
            type="button"
            onClick={exportCard}
            disabled={!me?.name}
            title={me?.name ? undefined : "Guarda primero tu nombre"}
            className="self-start rounded-lg border border-ink-700 px-4 py-2 text-sm text-slate-200 hover:bg-ink-800 disabled:opacity-40"
          >
            Exportar mi tarjeta de identidad…
          </button>
        </section>

        <section className="flex min-h-0 flex-col gap-3 rounded-2xl border border-ink-800 bg-ink-900 p-5">
          <div className="flex items-center justify-between">
            <h3 className="font-semibold text-white">Contactos de confianza</h3>
            <button
              type="button"
              onClick={importCard}
              className="rounded-lg border border-ink-700 px-3 py-1.5 text-sm text-slate-200 hover:bg-ink-800"
            >
              + Importar tarjeta
            </button>
          </div>
          {contacts.length === 0 ? (
            <p className="text-sm text-slate-500">
              Aún no tienes contactos. Pide a las personas que te envían documentos su archivo <code>.{CARD_EXTENSION}</code>.
            </p>
          ) : (
            <ul className="divide-y divide-ink-800 overflow-y-auto">
              {contacts.map((c) => (
                <li key={c.fingerprint} className="group flex items-center gap-3 py-2 text-sm">
                  <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-300">
                    ✓
                  </span>
                  <div className="min-w-0 flex-1">
                    {editing === c.fingerprint ? (
                      <input
                        autoFocus
                        value={editName}
                        onChange={(e) => setEditName(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") rename(c.fingerprint);
                          if (e.key === "Escape") setEditing(null);
                        }}
                        onBlur={() => setEditing(null)}
                        className="w-full rounded border border-ink-700 bg-ink-800 px-2 py-1 text-white outline-none focus:border-brand-500"
                      />
                    ) : (
                      <p className="truncate font-medium text-slate-100">{c.name}</p>
                    )}
                    <p className="font-mono text-xs text-slate-500">{c.fingerprint}</p>
                  </div>
                  <div className="flex shrink-0 gap-3 text-xs opacity-0 group-hover:opacity-100">
                    <button
                      type="button"
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => {
                        setEditing(c.fingerprint);
                        setEditName(c.name);
                      }}
                      className="text-slate-400 hover:text-slate-200"
                    >
                      Renombrar
                    </button>
                    <button type="button" onClick={() => remove(c)} className="text-slate-500 hover:text-red-400">
                      Quitar
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </div>
  );
}
