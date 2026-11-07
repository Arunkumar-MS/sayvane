// Tiny external store: seeded from api.getSnapshot(), kept current by core events.
import { useSyncExternalStore } from "react";
import { api } from "../ipc/commands";
import { onAppEvent } from "../ipc/events";
import type {
  AppEvent,
  AppSnapshot,
  EventEnvelope,
  HistoryEntry,
  ModelInfo,
  Page,
  Settings,
  SettingsPatch,
  Stats,
} from "../ipc/types";
import { isSessionActive } from "../lib/status";

export type SettingsTab = "dictation" | "shortcuts" | "polish" | "privacy";
export type ToastTone = "success" | "error" | "info";

export interface Toast {
  id: number;
  message: string;
  tone: ToastTone;
  detail?: string;
}

export interface UiState {
  snapshot: AppSnapshot | null;
  bootError: string | null;
  history: HistoryEntry[];
  historyLoaded: boolean;
  historyQuery: string;
  stats: Stats | null;
  models: ModelInfo[];
  page: Page;
  settingsTab: SettingsTab;
  /** Shown on the AI Polish tab after the header toggle was used without a provider. */
  polishNotice: boolean;
  storageError: string | null;
  toasts: Toast[];
}

type Listener = () => void;

function createStore<T>(initial: T) {
  let state = initial;
  const listeners = new Set<Listener>();
  return {
    get: () => state,
    set(patch: Partial<T> | ((s: T) => Partial<T>)) {
      const p = typeof patch === "function" ? patch(state) : patch;
      state = { ...state, ...p };
      listeners.forEach((l) => l());
    },
    subscribe(l: Listener) {
      listeners.add(l);
      return () => listeners.delete(l);
    },
  };
}

const initialPage = (): Page => {
  const p = new URLSearchParams(location.search).get("page");
  return p === "history" || p === "settings" ? p : "home";
};
const initialTab = (): SettingsTab => {
  const t = new URLSearchParams(location.search).get("tab");
  return t === "shortcuts" || t === "polish" || t === "privacy" ? t : "dictation";
};

export const store = createStore<UiState>({
  snapshot: null,
  bootError: null,
  history: [],
  historyLoaded: false,
  historyQuery: "",
  stats: null,
  models: [],
  page: initialPage(),
  settingsTab: initialTab(),
  polishNotice: false,
  storageError: null,
  toasts: [],
});

/** Live microphone level, kept apart so meters don't re-render the whole app. */
export const levelStore = createStore<{ sessionId: string | null; level: number; at: number }>({
  sessionId: null,
  level: 0,
  at: 0,
});

export function useStore<R>(selector: (s: UiState) => R): R {
  return useSyncExternalStore(store.subscribe, () => selector(store.get()));
}

export function useLevel(): number {
  return useSyncExternalStore(levelStore.subscribe, () => levelStore.get().level);
}

// ---------- toasts ----------

let toastId = 0;
export function toast(message: string, tone: ToastTone = "success", detail?: string) {
  const id = ++toastId;
  store.set((s) => ({ toasts: [...s.toasts.slice(-2), { id, message, tone, detail }] }));
  window.setTimeout(() => dismissToast(id), tone === "error" ? 5000 : 2200);
}
export function dismissToast(id: number) {
  store.set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
}

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return "Something went wrong.";
}

// ---------- data loading ----------

let lastSeq: number | null = null;
let buffered: EventEnvelope[] = [];
let resyncing = false;

export async function refreshSnapshot() {
  const snapshot = await api.getSnapshot();
  lastSeq = snapshot.eventSeq;
  store.set({ snapshot, bootError: null, storageError: snapshot.storage.ok ? store.get().storageError : snapshot.storage.error });
}

let historyReq = 0;
export async function loadHistory(query = store.get().historyQuery) {
  const req = ++historyReq;
  try {
    const list = await api.listHistory(query.trim() ? query.trim() : null);
    if (req === historyReq) store.set({ history: sortHistory(list), historyLoaded: true });
  } catch (e) {
    if (req === historyReq) store.set({ historyLoaded: true, storageError: errorText(e) });
  }
}

export async function refreshStats() {
  try {
    store.set({ stats: await api.getStats() });
  } catch {
    /* stats are optional */
  }
}

export async function loadModels() {
  try {
    store.set({ models: await api.listModels() });
  } catch (e) {
    toast("Couldn't load speech models", "error", errorText(e));
  }
}

let statsTimer: number | undefined;
function refreshStatsSoon() {
  window.clearTimeout(statsTimer);
  statsTimer = window.setTimeout(refreshStats, 400);
}

async function resync() {
  if (resyncing) return;
  resyncing = true;
  try {
    await refreshSnapshot();
    await Promise.all([loadHistory(), loadModels(), refreshStats()]);
  } finally {
    resyncing = false;
  }
}

function sortHistory(list: HistoryEntry[]): HistoryEntry[] {
  return [...list].sort((a, b) => Date.parse(b.createdAtUtc) - Date.parse(a.createdAtUtc));
}

export function matchesQuery(e: HistoryEntry, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [e.finalText, e.originalText, e.polishedText ?? "", e.destinationAppName ?? "", e.importName ?? ""].some((t) =>
    t.toLowerCase().includes(q),
  );
}

function patchSnapshot(patch: Partial<AppSnapshot>) {
  const s = store.get().snapshot;
  if (s) store.set({ snapshot: { ...s, ...patch } });
}

export function applyEvent(event: AppEvent) {
  const st = store.get();
  switch (event.kind) {
    case "sessionStateChanged":
      patchSnapshot({ session: event.session });
      if (event.session.phase !== "recording") levelStore.set({ level: 0 });
      break;
    case "audioLevel":
      levelStore.set({ sessionId: event.sessionId, level: event.level, at: performance.now() });
      break;
    case "historyEntryUpdated": {
      const exists = st.history.some((h) => h.id === event.entry.id);
      if (exists) {
        store.set({ history: st.history.map((h) => (h.id === event.entry.id ? event.entry : h)) });
      } else {
        if (matchesQuery(event.entry, st.historyQuery)) store.set({ history: sortHistory([event.entry, ...st.history]) });
        if (st.snapshot) patchSnapshot({ historyCount: st.snapshot.historyCount + 1 });
      }
      refreshStatsSoon();
      break;
    }
    case "historyEntryDeleted":
      store.set({ history: st.history.filter((h) => h.id !== event.id) });
      if (st.snapshot) patchSnapshot({ historyCount: Math.max(0, st.snapshot.historyCount - 1) });
      refreshStatsSoon();
      break;
    case "historyCleared":
      store.set({ history: [] });
      patchSnapshot({ historyCount: 0 });
      refreshStatsSoon();
      break;
    case "permissionsChanged":
      patchSnapshot({ permissions: event.permissions });
      break;
    case "engineChanged":
      patchSnapshot({ engine: event.engine });
      break;
    case "modelDownloadProgress": {
      const has = st.models.some((m) => m.id === event.model.id);
      store.set({ models: has ? st.models.map((m) => (m.id === event.model.id ? event.model : m)) : [...st.models, event.model] });
      if (event.model.download?.state === "failed") toast(`${event.model.label} download failed`, "error", event.model.download.error ?? undefined);
      break;
    }
    case "settingsChanged":
      patchSnapshot({ settings: event.settings });
      break;
    case "shortcutsChanged":
      patchSnapshot({ shortcuts: event.shortcuts });
      break;
    case "importsChanged":
      patchSnapshot({ imports: event.imports });
      break;
    case "storageError":
      store.set({ storageError: event.message });
      break;
    case "navigate":
      store.set({ page: event.page });
      break;
  }
}

function onEnvelope(env: EventEnvelope) {
  if (lastSeq === null) {
    buffered.push(env);
    return;
  }
  if (env.seq <= lastSeq) return; // duplicate or already covered by snapshot
  if (env.seq !== lastSeq + 1) {
    // Missed something: the snapshot is the source of truth.
    void resync();
    return;
  }
  lastSeq = env.seq;
  applyEvent(env.event);
}

let booted: Promise<void> | null = null;

/** Subscribe first, then fetch, so no event between the two is lost. */
export function boot(opts: { history: boolean } = { history: true }): Promise<void> {
  if (booted) return booted;
  booted = (async () => {
    try {
      await onAppEvent(onEnvelope);
      await refreshSnapshot();
      const pending = buffered;
      buffered = [];
      pending.sort((a, b) => a.seq - b.seq).forEach(onEnvelope);
      if (opts.history) await Promise.all([loadHistory(), refreshStats(), loadModels()]);
    } catch (e) {
      store.set({ bootError: errorText(e) });
    }
  })();
  return booted;
}

// ---------- actions ----------

export function navigate(page: Page, tab?: SettingsTab) {
  store.set({ page, ...(tab ? { settingsTab: tab } : {}), ...(page !== "settings" ? { polishNotice: false } : {}) });
}

export async function saveSettings(patch: SettingsPatch): Promise<Settings | null> {
  try {
    const settings = await api.updateSettings(patch);
    patchSnapshot({ settings });
    const snap = store.get().snapshot;
    toast("Saved", "success", snap && isSessionActive(snap.session) ? "Applies to your next dictation" : undefined);
    return settings;
  } catch (e) {
    toast("Couldn't save setting", "error", errorText(e));
    return null;
  }
}

export async function setPolishEnabled(enabled: boolean) {
  const snap = store.get().snapshot;
  if (!snap) return;
  if (enabled && !snap.settings.activeProfileId) {
    store.set({ page: "settings", settingsTab: "polish", polishNotice: true });
    return;
  }
  try {
    const settings = await api.setPolishEnabled(enabled);
    patchSnapshot({ settings });
    toast(enabled ? "AI polish on" : "AI polish off", "success", enabled ? undefined : "Transcripts stay on this computer");
  } catch (e) {
    toast("Couldn't change AI polish", "error", errorText(e));
  }
}

/** Runs an api call and reports failures as a toast; returns undefined on failure. */
export async function attempt<T>(label: string, fn: () => Promise<T>): Promise<T | undefined> {
  try {
    return await fn();
  } catch (e) {
    toast(label, "error", errorText(e));
    return undefined;
  }
}
