import { create } from "zustand";
import {
  api,
  errorMessage,
  type DocumentInspection,
  type DocxLayout,
  type IdentityInfo,
  type ProfileView,
  type SignaturePlacement,
} from "../lib/ipc";
import { EMPTY_CAPTION, type CaptionSettings } from "../features/signer/caption";

export type View = "signer" | "batch" | "history" | "vault" | "security" | "about";

export interface NoticeAction {
  label: string;
  run: () => void;
}

export interface Notice {
  id: number;
  tone: "success" | "error" | "info";
  text: string;
  actions?: NoticeAction[];
}

/** Última posición y ancho usados, para colocar la siguiente firma en el mismo sitio. */
export interface RememberedPlacement {
  x: number;
  y: number;
  width: number;
}

const REMEMBER_KEY = "firmar.lastPlacement";
const CAPTION_KEY = "firmar.caption";
const HISTORY_LIMIT = 100;

function loadCaption(): CaptionSettings {
  try {
    const v = JSON.parse(localStorage.getItem(CAPTION_KEY) ?? "null");
    return v && typeof v === "object" ? { ...EMPTY_CAPTION, ...v } : EMPTY_CAPTION;
  } catch {
    return EMPTY_CAPTION;
  }
}

function saveCaption(v: CaptionSettings) {
  try {
    localStorage.setItem(CAPTION_KEY, JSON.stringify(v));
  } catch {
    // Sin almacenamiento local: el texto no se recuerda entre sesiones.
  }
}

function loadRemembered(): RememberedPlacement | null {
  try {
    const raw = localStorage.getItem(REMEMBER_KEY);
    const v = raw ? JSON.parse(raw) : null;
    return v && [v.x, v.y, v.width].every((n) => typeof n === "number" && n >= 0 && n <= 1) ? v : null;
  } catch {
    return null;
  }
}

function saveRemembered(v: RememberedPlacement) {
  try {
    localStorage.setItem(REMEMBER_KEY, JSON.stringify(v));
  } catch {
    // Sin almacenamiento local: simplemente no se recuerda.
  }
}

/** Documento cargado en el firmador. */
export interface OpenDocument {
  inspection: DocumentInspection;
  /** Bytes que muestra el visor: el original si estamos re-posicionando. */
  bytes: Uint8Array;
  /** Solo para .docx: tamaño de la primera y la última página. */
  docxLayout: DocxLayout | null;
  /** `resign` cuando el documento ya es nuestro y editamos su firma. */
  mode: "sign" | "resign" | "readonly";
}

export interface PlacedSignature extends SignaturePlacement {
  key: string;
}

interface FirmarState {
  view: View;
  identity: IdentityInfo | null;
  profiles: ProfileView[];
  activeProfileId: string | null;
  document: OpenDocument | null;
  placements: PlacedSignature[];
  busy: string | null;
  notices: Notice[];
  remembered: RememberedPlacement | null;
  caption: CaptionSettings;
  /** Deshacer / rehacer de las firmas colocadas en el documento abierto. */
  past: PlacedSignature[][];
  future: PlacedSignature[][];
  /** Archivo por abrir en el firmador (Explorador, otra ventana…). */
  pendingOpen: string | null;
  /** Archivos elegidos para firmar en lote. */
  batchQueue: string[];

  setView: (view: View) => void;
  bootstrap: () => Promise<void>;
  refreshProfiles: () => Promise<void>;
  saveProfile: (name: string, pngDataUrl: string) => Promise<void>;
  deleteProfile: (id: string) => Promise<void>;
  setActiveProfile: (id: string) => void;

  setDocument: (doc: OpenDocument | null, placements?: SignaturePlacement[]) => void;
  addPlacement: (p: SignaturePlacement) => void;
  updatePlacement: (key: string, patch: Partial<SignaturePlacement>) => void;
  removePlacement: (key: string) => void;
  /** Guarda el estado actual para poder deshacer el cambio que empieza. */
  checkpoint: () => void;
  undo: () => void;
  redo: () => void;
  /** Aplica el texto a todas las firmas colocadas y lo recuerda. */
  setCaption: (settings: CaptionSettings, caption: string | null, resize: (p: PlacedSignature) => Partial<SignaturePlacement>) => void;
  requestOpen: (path: string | null) => void;
  setBatchQueue: (paths: string[]) => void;

  setBusy: (label: string | null) => void;
  notify: (tone: Notice["tone"], text: string, actions?: NoticeAction[]) => void;
  dismiss: (id: number) => void;
}

let seq = 0;
let rememberTimer: ReturnType<typeof setTimeout> | undefined;
/** Guarda con retardo: durante un arrastre llegan decenas de cambios por segundo. */
function remember(p: SignaturePlacement) {
  clearTimeout(rememberTimer);
  rememberTimer = setTimeout(() => saveRemembered({ x: p.x, y: p.y, width: p.width }), 300);
}
const nextKey = () => `p${++seq}`;

export const useFirmarStore = create<FirmarState>((set, get) => ({
  view: "signer",
  identity: null,
  profiles: [],
  activeProfileId: null,
  document: null,
  placements: [],
  busy: null,
  notices: [],
  remembered: loadRemembered(),
  caption: loadCaption(),
  past: [],
  future: [],
  pendingOpen: null,
  batchQueue: [],

  setView: (view) => set({ view }),

  bootstrap: async () => {
    try {
      const identity = await api.identityInfo();
      set({ identity });
      await get().refreshProfiles();
      if (get().profiles.length === 0) set({ view: "vault" });
    } catch (e) {
      get().notify("error", errorMessage(e));
    }
  },

  refreshProfiles: async () => {
    const profiles = await api.vaultList();
    const current = get().activeProfileId;
    set({
      profiles,
      activeProfileId: profiles.some((p) => p.id === current) ? current : (profiles[0]?.id ?? null),
    });
  },

  saveProfile: async (name, pngDataUrl) => {
    const profile = await api.vaultSave(name, pngDataUrl);
    await get().refreshProfiles();
    set({ activeProfileId: profile.id });
  },

  deleteProfile: async (id) => {
    await api.vaultDelete(id);
    await get().refreshProfiles();
  },

  setActiveProfile: (id) => set({ activeProfileId: id }),

  setDocument: (document, placements = []) =>
    set({ document, placements: placements.map((p) => ({ ...p, key: nextKey() })), past: [], future: [] }),

  addPlacement: (p) => {
    get().checkpoint();
    remember(p);
    set((s) => ({ placements: [...s.placements, { ...p, key: nextKey() }], remembered: { x: p.x, y: p.y, width: p.width } }));
  },

  updatePlacement: (key, patch) =>
    set((s) => {
      const placements = s.placements.map((p) => (p.key === key ? { ...p, ...patch } : p));
      const changed = placements.find((p) => p.key === key);
      if (!changed) return { placements };
      remember(changed);
      return { placements, remembered: { x: changed.x, y: changed.y, width: changed.width } };
    }),

  removePlacement: (key) => {
    get().checkpoint();
    set((s) => ({ placements: s.placements.filter((p) => p.key !== key) }));
  },

  checkpoint: () => set((s) => ({ past: [...s.past, s.placements].slice(-HISTORY_LIMIT), future: [] })),

  undo: () =>
    set((s) => {
      const previous = s.past[s.past.length - 1];
      if (!previous) return {};
      return { placements: previous, past: s.past.slice(0, -1), future: [s.placements, ...s.future] };
    }),

  redo: () =>
    set((s) => {
      const [next, ...rest] = s.future;
      if (!next) return {};
      return { placements: next, past: [...s.past, s.placements], future: rest };
    }),

  setCaption: (settings, caption, resize) => {
    saveCaption(settings);
    const changed = get().placements.some((p) => (p.caption ?? null) !== caption);
    if (changed) get().checkpoint();
    set((s) => ({
      caption: settings,
      placements: changed ? s.placements.map((p) => ({ ...p, ...resize(p), caption })) : s.placements,
    }));
  },

  requestOpen: (pendingOpen) => set({ pendingOpen, view: pendingOpen ? "signer" : get().view }),

  setBatchQueue: (paths) =>
    set((s) => ({ batchQueue: Array.from(new Set(paths)), view: paths.length ? "batch" : s.view })),

  setBusy: (busy) => set({ busy }),

  notify: (tone, text, actions) => {
    const id = Date.now() + Math.random();
    set((s) => ({ notices: [...s.notices, { id, tone, text, actions }] }));
    setTimeout(() => get().dismiss(id), actions?.length ? 12000 : tone === "error" ? 8000 : 4500);
  },

  dismiss: (id) => set((s) => ({ notices: s.notices.filter((n) => n.id !== id) })),
}));

export const activeProfile = (s: FirmarState) => s.profiles.find((p) => p.id === s.activeProfileId) ?? null;
