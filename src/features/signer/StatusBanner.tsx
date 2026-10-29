import type { DocumentInspection } from "../../lib/ipc";

const date = (iso: string) =>
  new Date(iso).toLocaleString(undefined, { day: "numeric", month: "long", year: "numeric", hour: "2-digit", minute: "2-digit" });

/** Estado del documento en lenguaje llano; lo técnico queda en «Ver detalles». */
export function StatusBanner({ inspection }: { inspection: DocumentInspection }) {
  const { status } = inspection;

  if (status.state === "unsigned") return null;

  if (status.state === "tampered") {
    return (
      <Banner tone="danger" icon="✕" title="Este documento fue modificado después de firmarlo">
        <p>La firma ya no es válida. No lo uses como documento firmado.</p>
        <Details rows={[["Motivo", status.reason]]} />
      </Banner>
    );
  }

  const meta = status.metadata;
  const modified = status.state === "modifiedAfterSigning";
  const when = meta ? ` el ${date(meta.signedAt)}` : "";

  // Quién firmó: tú, un contacto de confianza (verificado) o un nombre declarado (sin verificar).
  const who = status.owned
    ? "ti"
    : status.contactName
      ? status.contactName
      : status.signerName
        ? `«${status.signerName}»`
        : "otra persona";

  const title = modified
    ? `Firmado por ${who}, pero otro programa le añadió cambios después`
    : `Firmado por ${who}${status.owned ? when : ""}`;

  const body = modified
    ? "Lo firmado sigue intacto, pero el archivo tiene contenido añadido después de la firma. Revisa quién lo editó."
    : status.owned
      ? "Nadie lo ha modificado desde que lo firmaste."
      : "La firma es auténtica y el documento no ha cambiado desde que se firmó.";

  const rows: [string, string][] = [["Huella del firmante", status.signerFingerprint]];
  if (status.signerName) rows.unshift(["Nombre en el sello", status.signerName]);
  if (meta) {
    rows.push(
      ["Firma usada", meta.profileName],
      ["Firmas en el documento", String(meta.placements.length || "sin estampado visual")],
      ["Archivo original", meta.sourceFileName],
      ["Huella del original (SHA-256)", meta.originalSha256],
    );
  }

  return (
    <Banner tone={modified ? "warning" : "success"} icon={modified ? "!" : "✓"} title={title}>
      <p>{body}</p>
      {!status.owned && <IdentityNote contactName={status.contactName} signerName={status.signerName} />}
      <Details rows={rows} />
    </Banner>
  );
}

/** Aclara si la identidad del firmante está verificada por tus contactos. */
function IdentityNote({ contactName, signerName }: { contactName: string | null; signerName: string | null }) {
  if (contactName) {
    return (
      <p className="mt-1 flex flex-wrap items-center gap-2">
        <span className="rounded-full bg-emerald-500/20 px-2 py-0.5 text-xs font-medium text-emerald-200">
          ✓ Contacto verificado
        </span>
        {signerName && signerName !== contactName && (
          <span className="text-xs opacity-80">En el sello se identificó como «{signerName}».</span>
        )}
      </p>
    );
  }
  return (
    <p className="mt-1 text-xs opacity-90">
      <span className="rounded-full bg-amber-500/20 px-2 py-0.5 font-medium text-amber-200">Identidad no verificada</span>{" "}
      {signerName ? "Cualquiera puede escribir un nombre." : "El firmante no puso su nombre."} Para confirmar quién es,
      pídele su tarjeta de identidad e impórtala en «Identidad».
    </p>
  );
}

const TONES = {
  success: { box: "border-emerald-500/40 bg-emerald-500/10 text-emerald-100", icon: "bg-emerald-500 text-white" },
  warning: { box: "border-amber-500/40 bg-amber-500/10 text-amber-100", icon: "bg-amber-500 text-ink-950" },
  danger: { box: "border-red-500/40 bg-red-500/10 text-red-100", icon: "bg-red-500 text-white" },
};

function Banner({ tone, icon, title, children }: { tone: keyof typeof TONES; icon: string; title: string; children: React.ReactNode }) {
  return (
    <div className={`flex gap-3 rounded-xl border px-4 py-3 text-sm ${TONES[tone].box}`}>
      <span className={`mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-xs font-bold ${TONES[tone].icon}`}>
        {icon}
      </span>
      <div className="min-w-0 flex-1 space-y-1">
        <p className="font-semibold">{title}</p>
        <div className="opacity-90">{children}</div>
      </div>
    </div>
  );
}

function Details({ rows }: { rows: [string, string][] }) {
  return (
    <details className="pt-1 text-xs">
      <summary className="cursor-pointer opacity-70 select-none hover:opacity-100">Ver detalles</summary>
      <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
        {rows.map(([k, v]) => (
          <div key={k} className="contents">
            <dt className="opacity-70">{k}</dt>
            <dd className="font-mono break-all select-text">{v}</dd>
          </div>
        ))}
      </dl>
    </details>
  );
}
