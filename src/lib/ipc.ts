import { invoke } from "@tauri-apps/api/core";

/** Espejo de los tipos serializados por `src-tauri/src/domain`. */
export type DocumentFormat = "pdf" | "png" | "jpeg" | "docx" | "binary";

export interface SignaturePlacement {
  page: number;
  x: number;
  y: number;
  width: number;
  height: number;
  /** Texto bajo la firma (nombre, fecha…), hasta 5 líneas. */
  caption?: string | null;
  captionAlign?: "left" | "center" | "right" | null;
}

export interface HistoryEntry {
  documentId: string;
  fileName: string;
  outputPath: string;
  format: DocumentFormat;
  signedAt: string;
  signatures: number;
}

export type PageTarget = "first" | "last" | "all";

export interface PlacementTemplate {
  pages: PageTarget;
  x: number;
  y: number;
  width: number;
  caption: string | null;
  captionAlign: "left" | "center" | "right" | null;
}

export interface BatchItem {
  path: string;
  outputPath: string | null;
  error: string | null;
}

export interface BatchProgress {
  done: number;
  total: number;
  current: string;
}

export interface FirmarMetadataPayload {
  version: number;
  documentId: string;
  profileId: string;
  profileName: string;
  sourceFileName: string;
  format: DocumentFormat;
  placements: SignaturePlacement[];
  signedAt: string;
  originalSha256: string;
}

interface SignedState {
  owned: boolean;
  signerFingerprint: string;
  /** Nombre que el firmante declaró en el sello (no verificado). */
  signerName: string | null;
  /** Nombre del contacto de confianza con esa clave, si lo es. */
  contactName: string | null;
  metadata: FirmarMetadataPayload | null;
}

export type SignatureStatus =
  | { state: "unsigned" }
  | ({ state: "valid" } & SignedState)
  | ({ state: "modifiedAfterSigning" } & SignedState)
  | { state: "tampered"; reason: string };

export interface DocumentInspection {
  path: string;
  fileName: string;
  format: DocumentFormat;
  size: number;
  status: SignatureStatus;
}

export interface SignedDocument {
  documentId: string;
  outputPath: string;
  sidecarPath: string | null;
  format: DocumentFormat;
  contentSha256: string;
  signedAt: string;
}

export interface ProfileView {
  id: string;
  name: string;
  createdAt: string;
  width: number;
  height: number;
  dataUrl: string;
}

/** Página de Word en puntos. En DOCX solo se firman la primera y la última. */
export interface DocxPageSetup {
  width: number;
  height: number;
  marginTop: number;
  marginRight: number;
  marginBottom: number;
  marginLeft: number;
}

export interface DocxLayout {
  /** `[primera, última]`, o solo `[primera]` si el documento tiene una página. */
  pages: DocxPageSetup[];
  pagesHint: number | null;
}

export interface IdentityInfo {
  fingerprint: string;
  vaultPath: string;
}

export interface BackupSummary {
  fingerprint: string;
  profiles: number;
  documents: number;
}

export interface MyIdentity {
  name: string | null;
  fingerprint: string;
}

export interface ContactView {
  name: string;
  fingerprint: string;
  addedAt: string;
}

export interface LockView {
  enabled: boolean;
  locked: boolean;
  hello: boolean;
  autoLockMinutes: number;
  retryInSeconds: number;
  helloAvailable: boolean;
}

export interface LegalStatus {
  acceptedVersion: string | null;
  acceptedAt: string | null;
}

export interface FirmarError {
  kind: string;
  message: string;
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as FirmarError).message);
  return String(e);
}

export const api = {
  identityInfo: () => invoke<IdentityInfo>("identity_info"),

  vaultList: () => invoke<ProfileView[]>("vault_list"),
  vaultSave: (name: string, pngBase64: string) => invoke<ProfileView>("vault_save", { name, pngBase64 }),
  vaultDelete: (id: string) => invoke<void>("vault_delete", { id }),

  documentRead: (path: string) => invoke<ArrayBuffer>("document_read", { path }),
  documentOriginal: (path: string) => invoke<ArrayBuffer>("document_original", { path }),
  docxLayout: (path: string) => invoke<DocxLayout>("docx_layout", { path }),
  documentInspect: (path: string) => invoke<DocumentInspection>("document_inspect", { path }),
  /** `outputPath = null`: se guarda junto al original como `nombre.firmado.ext`. */
  documentSign: (inputPath: string, outputPath: string | null, profileId: string, placements: SignaturePlacement[]) =>
    invoke<SignedDocument>("document_sign", { inputPath, outputPath, profileId, placements }),
  /** `outputPath = null`: se actualiza el mismo archivo firmado. */
  documentResign: (signedPath: string, outputPath: string | null, profileId: string, placements: SignaturePlacement[]) =>
    invoke<SignedDocument>("document_resign", { signedPath, outputPath, profileId, placements }),

  documentSignBatch: (paths: string[], profileId: string, template: PlacementTemplate) =>
    invoke<BatchItem[]>("document_sign_batch", { paths, profileId, template }),

  historyList: () => invoke<HistoryEntry[]>("history_list"),
  historyForget: (documentId: string) => invoke<void>("history_forget", { documentId }),
  takeLaunchFile: () => invoke<string | null>("take_launch_file"),

  identityGet: () => invoke<MyIdentity>("identity_get"),
  identitySetName: (name: string | null) => invoke<MyIdentity>("identity_set_name", { name }),
  identityExportCard: (path: string) => invoke<void>("identity_export_card", { path }),
  contactsList: () => invoke<ContactView[]>("contacts_list"),
  contactImport: (path: string) => invoke<ContactView>("contact_import", { path }),
  contactRename: (fingerprint: string, name: string) => invoke<void>("contact_rename", { fingerprint, name }),
  contactRemove: (fingerprint: string) => invoke<void>("contact_remove", { fingerprint }),

  lockStatus: () => invoke<LockView>("lock_status"),
  lockSetPin: (pin: string) => invoke<void>("lock_set_pin", { pin }),
  lockDisable: () => invoke<void>("lock_disable"),
  lockSetOptions: (hello: boolean, autoLockMinutes: number) => invoke<void>("lock_set_options", { hello, autoLockMinutes }),
  lockUnlockPin: (pin: string) => invoke<void>("lock_unlock_pin", { pin }),
  lockUnlockHello: () => invoke<void>("lock_unlock_hello"),
  lockNow: () => invoke<void>("lock_now"),

  legalStatus: () => invoke<LegalStatus>("legal_status"),
  legalAccept: (version: string) => invoke<LegalStatus>("legal_accept", { version }),
  /** Borra firmas, originales, historial y claves. `confirmation` debe ser "BORRAR". */
  dataWipe: (confirmation: string) => invoke<void>("data_wipe", { confirmation }),

  backupExport: (path: string, password: string) => invoke<void>("backup_export", { path, password }),
  backupInspect: (path: string, password: string) => invoke<BackupSummary>("backup_inspect", { path, password }),
  backupRestore: (path: string, password: string) => invoke<IdentityInfo>("backup_restore", { path, password }),

  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
};
