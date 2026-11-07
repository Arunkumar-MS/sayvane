// In-memory stand-in for the Rust core, used only by `pnpm dev:mock` (browser preview)
// and screenshots. Every command in commands.ts is served here with realistic data.
// URL params: ?os=windows  ?setup=1 (missing access + model)  ?empty=1 (no history)
//             ?state=idle|hover|starting|recording|handsfree|transcribing|polishing|result|result-warning|result-error
//             ?page=home|history|settings
import type {
  AppEvent,
  AppSnapshot,
  AudioInput,
  ConnectionResult,
  CredentialStatus,
  EventEnvelope,
  HistoryEntry,
  ImportJob,
  ModelInfo,
  ModelSummary,
  PermissionStatus,
  ProviderProfile,
  RecordingMode,
  SessionView,
  Settings,
  SettingsPatch,
  ShortcutStatus,
  Stats,
} from "./types";
import { EVENT_CHANNEL } from "./types";

const params = new URLSearchParams(typeof location !== "undefined" ? location.search : "");
const os = params.get("os") === "windows" ? "windows" : "macos";
const setupMode = params.get("setup") === "1";
const emptyMode = params.get("empty") === "1";

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

// ---------- seed ----------

function iso(msAgo: number): string {
  return new Date(Date.now() - msAgo).toISOString();
}

function atLocal(daysAgo: number, hour: number, minute: number): string {
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  d.setHours(hour, minute, 0, 0);
  return d.toISOString();
}

let idCounter = 100;
const nextId = () => `d_${(idCounter++).toString(36)}${Math.random().toString(36).slice(2, 6)}`;

function entry(p: Partial<HistoryEntry> & Pick<HistoryEntry, "createdAtUtc" | "finalText">): HistoryEntry {
  return {
    id: nextId(),
    source: "dictation",
    audioState: "saved",
    durationMs: 9000,
    originalText: p.finalText,
    polishedText: null,
    detectedLanguage: "en",
    modelId: "base",
    destinationAppName: null,
    processingState: "done",
    polishState: "off",
    insertionState: "confirmed",
    errorCode: null,
    errorMessage: null,
    hasSegments: false,
    canRetry: true,
    retryUnavailableReason: null,
    retryKind: "retry",
    importName: null,
    ...p,
  };
}

function seedHistory(): HistoryEntry[] {
  if (emptyMode) return [];
  const today = new Date().getHours();
  // Keep "today" entries inside today even early in the morning.
  const t = (minsAgo: number) => (today * 60 > minsAgo + 5 ? iso(minsAgo * MIN) : atLocal(0, 0, 5));
  return [
    entry({
      createdAtUtc: t(12),
      destinationAppName: "Slack",
      durationMs: 14_200,
      originalText:
        "hey so I looked at the onboarding numbers and um I think the drop off is mostly on the permissions step can we pair on it tomorrow morning",
      polishedText:
        "Hey — I looked at the onboarding numbers, and I think the drop-off is mostly on the permissions step. Can we pair on it tomorrow morning?",
      finalText:
        "Hey — I looked at the onboarding numbers, and I think the drop-off is mostly on the permissions step. Can we pair on it tomorrow morning?",
      polishState: "applied",
      insertionState: "confirmed",
    }),
    entry({
      createdAtUtc: t(48),
      destinationAppName: "Mail",
      durationMs: 21_800,
      finalText:
        "Thanks for sending the draft over. The structure works well. I'd tighten the second section and move the pricing table closer to the top so it's the first thing people see.",
      insertionState: "sentUnconfirmed",
    }),
    entry({
      createdAtUtc: t(125),
      source: "note",
      destinationAppName: null,
      durationMs: 32_400,
      finalText:
        "Ideas for the weekend: repot the fig tree, finish the second half of the novel, and call Grandma about the trip in December. Also check whether the bike shop is open on Sunday.",
      insertionState: "notRequested",
    }),
    entry({
      createdAtUtc: t(190),
      destinationAppName: "Notes",
      durationMs: 18_600,
      originalText: "",
      finalText: "",
      processingState: "failed",
      insertionState: "savedOnly",
      errorCode: "worker_crashed",
      errorMessage: "The local speech engine stopped unexpectedly. Your audio is saved — Recover will transcribe it again.",
      retryKind: "recover",
    }),
    entry({
      createdAtUtc: t(240),
      destinationAppName: "Linear",
      durationMs: 3_100,
      originalText: "",
      finalText: "",
      processingState: "canceled",
      insertionState: "canceled",
      retryKind: "recover",
    }),
    entry({
      createdAtUtc: atLocal(1, 16, 42),
      source: "import",
      importName: "Team sync — Nov 6.m4a",
      destinationAppName: null,
      durationMs: 4 * MIN + 12_000,
      hasSegments: true,
      insertionState: "notRequested",
      finalText:
        "Okay, let's get started. First item is the release checklist. We're on track for the beta next Thursday, pending the notarization fix. Second, design wants another pass on the empty states in History. Third, we agreed to keep AI polishing off by default and make the privacy explainer more visible in Settings. Action items: Priya owns notarization, Sam takes the empty states, and I'll write the privacy copy.",
    }),
    entry({
      createdAtUtc: atLocal(1, 11, 5),
      destinationAppName: "Visual Studio Code",
      durationMs: 7_900,
      finalText: "Refactor the retry handler so it keeps the last successful transcript until the new attempt finishes.",
      polishState: "fallback",
      insertionState: "confirmed",
    }),
    entry({
      createdAtUtc: atLocal(1, 9, 30),
      destinationAppName: "Safari",
      durationMs: 6_400,
      finalText: "Book a table for four at seven thirty on Friday, somewhere quiet near the river.",
      insertionState: "destinationChanged",
    }),
    entry({
      createdAtUtc: atLocal(3, 14, 20),
      destinationAppName: "Messages",
      durationMs: 2_300,
      originalText: "",
      finalText: "",
      processingState: "noSpeech",
      insertionState: "savedOnly",
      errorMessage: "No speech was detected. Check that the right microphone is selected.",
    }),
    entry({
      createdAtUtc: atLocal(4, 10, 15),
      destinationAppName: "Notion",
      durationMs: 26_000,
      originalText:
        "so for the Q1 plan the three goals are ship the windows build get crash free sessions above ninety nine percent and cut cold start to under two seconds",
      polishedText:
        "Q1 plan — three goals:\n1. Ship the Windows build.\n2. Get crash-free sessions above 99%.\n3. Cut cold start to under two seconds.",
      finalText:
        "Q1 plan — three goals:\n1. Ship the Windows build.\n2. Get crash-free sessions above 99%.\n3. Cut cold start to under two seconds.",
      polishState: "applied",
    }),
    entry({
      createdAtUtc: atLocal(6, 19, 45),
      source: "note",
      audioState: "removedByRetention",
      durationMs: 11_000,
      finalText: "Remember to renew the passport before March. The appointment form needs a recent photo.",
      insertionState: "notRequested",
      canRetry: false,
      retryUnavailableReason: "Audio was removed by your 7-day retention setting.",
      retryKind: null,
    }),
    entry({
      createdAtUtc: atLocal(12, 8, 50),
      destinationAppName: "Terminal",
      durationMs: 5_200,
      finalText: "git log since last Tuesday for the bar window",
      insertionState: "ineligible",
    }),
  ];
}

function seedModels(): ModelInfo[] {
  const base = !setupMode;
  return [
    { id: "tiny", label: "Fastest", description: "Lowest resource use. Good for quick notes on older machines.", sizeBytes: 77_691_713, multilingual: true, canTranslate: true, installed: false, isDefault: false, recommendedMinMemoryGb: 2, download: null },
    { id: "base", label: "Balanced", description: "The recommended default — accurate and quick on every supported computer.", sizeBytes: 147_951_465, multilingual: true, canTranslate: true, installed: base, isDefault: true, recommendedMinMemoryGb: 4, download: null },
    { id: "small", label: "More accurate", description: "Noticeably better with accents and jargon. Uses more memory.", sizeBytes: 487_601_967, multilingual: true, canTranslate: true, installed: false, isDefault: false, recommendedMinMemoryGb: 8, download: null },
    { id: "large-v3-turbo", label: "High accuracy", description: "Best quality for long recordings and imports. Needs a recent computer.", sizeBytes: 1_624_555_275, multilingual: true, canTranslate: false, installed: false, isDefault: false, recommendedMinMemoryGb: 16, download: null },
    { id: "tiny.en", label: "English fast", description: "English only. Very light and responsive.", sizeBytes: 77_704_715, multilingual: false, canTranslate: false, installed: false, isDefault: false, recommendedMinMemoryGb: 2, download: null },
  ];
}

const defaultShortcuts =
  os === "macos"
    ? [
        { action: "pushToTalk" as const, accelerator: "Control+Alt+Space" },
        { action: "toggleHandsFree" as const, accelerator: "Control+Alt+Shift+Space" },
        { action: "pasteLast" as const, accelerator: "Control+Command+V" },
        { action: "copyLast" as const, accelerator: "Control+Command+C" },
        { action: "cancel" as const, accelerator: "Escape" },
      ]
    : [
        { action: "pushToTalk" as const, accelerator: "Control+Alt+Space" },
        { action: "toggleHandsFree" as const, accelerator: "Control+Alt+Shift+Space" },
        { action: "pasteLast" as const, accelerator: "Control+Alt+V" },
        { action: "copyLast" as const, accelerator: "Control+Alt+C" },
        { action: "cancel" as const, accelerator: "Escape" },
      ];

const idleSession: SessionView = {
  phase: "idle",
  sessionId: null,
  generation: 0,
  mode: null,
  elapsedMs: null,
  destinationLabel: null,
  cancelPending: false,
  result: null,
  busyReason: null,
};

interface MockState {
  snapshot: AppSnapshot;
  history: HistoryEntry[];
  models: ModelInfo[];
  profiles: ProviderProfile[];
  creds: Record<string, CredentialStatus>;
}

const permissions: PermissionStatus =
  os === "macos"
    ? setupMode
      ? { microphone: "notDetermined", accessibility: "denied", inputMonitoring: "granted" }
      : { microphone: "granted", accessibility: "granted", inputMonitoring: "granted" }
    : { microphone: setupMode ? "denied" : "granted", accessibility: "notApplicable", inputMonitoring: "notApplicable" };

const state: MockState = {
  history: seedHistory(),
  models: seedModels(),
  profiles: [
    {
      id: "p_openrouter",
      label: "OpenRouter",
      kind: "openrouter",
      baseUrl: "https://openrouter.ai/api/v1",
      chatPath: "chat/completions",
      modelsPath: "models",
      modelId: "meta-llama/llama-3.3-70b-instruct",
      authMode: "bearer",
      headers: {},
      secretHeaderNames: [],
      timeoutMs: 15000,
    },
  ],
  creds: {
    p_openrouter: { profileId: "p_openrouter", bearerSaved: true, savedSecretHeaders: [], needsReconnect: false },
  },
  snapshot: {
    brand: {
      displayName: "Sayvane",
      productSlug: "sayvane",
      publisher: "Sayvane",
      applicationId: "app.sayvane.desktop",
      tagline: "Speak where you type",
      accentColor: "#E8573A",
      supportUrl: null,
      privacyUrl: null,
    },
    platform: { os, arch: os === "macos" ? "aarch64" : "x86_64", osVersion: os === "macos" ? "15.1" : "11 24H2" },
    session: { ...idleSession },
    settings: {
      schemaVersion: 1,
      modelId: "base",
      language: null,
      task: "transcribe",
      vocabularyHints: "",
      microphoneId: null,
      advanced: { temperature: 0, beamSize: null, wordTimestamps: false, noContext: true, noSpeechThreshold: 0.6, threads: 0 },
      polishEnabled: false,
      activeProfileId: "p_openrouter",
      shortcuts: defaultShortcuts,
      fnGestureEnabled: os === "macos",
      retention: "keep",
      launchAtLogin: true,
      reducedMotion: "system",
      soundsEnabled: true,
    },
    permissions,
    engine: setupMode
      ? { state: "noModel", modelId: null, backend: null, message: null }
      : { state: "ready", modelId: "base", backend: os === "macos" ? "metal" : "cpu", message: null },
    shortcuts: defaultShortcuts.map((s) => ({ ...s, registered: true, error: null })),
    storage: {
      dataDir: os === "macos" ? "~/Library/Application Support/app.sayvane.desktop" : "%APPDATA%\\app.sayvane.desktop",
      ok: true,
      error: null,
    },
    historyCount: 0,
    imports: [],
    eventSeq: 1,
    settingsWarning: null,
  },
};
state.snapshot.historyCount = state.history.length;

// ---------- event bus ----------

type Listener = (payload: unknown) => void;
const listeners = new Map<string, Set<Listener>>();

export function subscribeMock(channel: string, fn: Listener): () => void {
  if (!listeners.has(channel)) listeners.set(channel, new Set());
  listeners.get(channel)!.add(fn);
  return () => listeners.get(channel)?.delete(fn);
}

function emit(event: AppEvent) {
  state.snapshot.eventSeq += 1;
  const env: EventEnvelope = { schemaVersion: 1, seq: state.snapshot.eventSeq, event };
  listeners.get(EVENT_CHANNEL)?.forEach((fn) => fn(env));
}

function setSession(patch: Partial<SessionView>) {
  state.snapshot.session = { ...state.snapshot.session, ...patch };
  emit({ kind: "sessionStateChanged", session: state.snapshot.session });
}

// ---------- fake session ----------

let timers: number[] = [];
let levelTimer: number | null = null;
let elapsedTimer: number | null = null;
let recordStart = 0;

function clearTimers() {
  timers.forEach((t) => clearTimeout(t));
  timers = [];
  if (levelTimer !== null) clearInterval(levelTimer);
  if (elapsedTimer !== null) clearInterval(elapsedTimer);
  levelTimer = elapsedTimer = null;
}

function later(ms: number, fn: () => void) {
  timers.push(window.setTimeout(fn, ms));
}

function startLevels(sessionId: string) {
  let phase = 0;
  levelTimer = window.setInterval(() => {
    phase += 1;
    // Syllable-ish envelope with pauses between words.
    const syll = Math.max(0, Math.sin(phase / 2.3)) * (0.35 + 0.65 * Math.max(0, Math.sin(phase / 11)));
    const level = Math.min(1, 0.04 + syll * (0.55 + Math.random() * 0.45));
    emit({ kind: "audioLevel", sessionId, level });
  }, 60);
}

const sampleNotes = [
  "Quick reminder to send the revised proposal to Jordan before Thursday and ask about the venue deposit.",
  "Pick up coffee beans, oat milk, and a birthday card for Alex on the way home.",
  "The new onboarding flow feels much calmer. Let's keep the checklist but hide it once everything is set up.",
];

function startDictation(mode: RecordingMode) {
  const s = state.snapshot.session;
  if (s.phase !== "idle" && s.phase !== "result") throw "Another dictation is already in progress.";
  clearTimers();
  const sessionId = nextId();
  setSession({
    ...idleSession,
    phase: "starting",
    sessionId,
    generation: s.generation + 1,
    mode,
    destinationLabel: mode === "note" ? "History" : "Notes",
  });
  later(450, () => {
    recordStart = Date.now();
    setSession({ phase: "recording", elapsedMs: 0 });
    startLevels(sessionId);
    elapsedTimer = window.setInterval(() => setSession({ elapsedMs: Date.now() - recordStart }), 1000);
  });
}

function finishDictation() {
  const s = state.snapshot.session;
  if (s.phase !== "recording") return;
  const durationMs = Date.now() - recordStart;
  clearTimers();
  setSession({ phase: "savingAudio", elapsedMs: null });
  later(250, () => setSession({ phase: "transcribing" }));
  const polish = state.snapshot.settings.polishEnabled;
  const t1 = 1600;
  if (polish) later(t1, () => setSession({ phase: "polishing" }));
  const t2 = polish ? t1 + 1300 : t1;
  later(t2, () => setSession({ phase: "savingTranscript" }));
  later(t2 + 250, () => {
    const text = sampleNotes[Math.floor(Math.random() * sampleNotes.length)];
    const isNote = s.mode === "note";
    const e = entry({
      createdAtUtc: new Date().toISOString(),
      source: isNote ? "note" : "dictation",
      durationMs,
      destinationAppName: isNote ? null : "Notes",
      finalText: text,
      polishState: polish ? "applied" : "off",
      polishedText: polish ? text : null,
      insertionState: isNote ? "notRequested" : "confirmed",
    });
    state.history.unshift(e);
    state.snapshot.historyCount = state.history.length;
    emit({ kind: "historyEntryUpdated", entry: e });
    setSession({
      phase: "result",
      result: { tone: "success", message: isNote ? "Saved to History" : "Inserted in Notes", dictationId: e.id, canCopy: true },
    });
    later(2600, () => setSession({ ...idleSession, generation: state.snapshot.session.generation }));
  });
}

function cancelSession() {
  clearTimers();
  setSession({ ...idleSession, generation: state.snapshot.session.generation });
}

// Bar screenshots: seed a session state from ?state=
function applyStateParam() {
  const p = params.get("state");
  if (!p) return;
  const base: SessionView = { ...idleSession, sessionId: "s_demo", generation: 1, destinationLabel: "Notes", mode: "hold" };
  switch (p) {
    case "starting":
      state.snapshot.session = { ...base, phase: "starting" };
      break;
    case "recording":
    case "handsfree":
      state.snapshot.session = { ...base, phase: "recording", mode: p === "handsfree" ? "handsFree" : "hold", elapsedMs: 83_000 };
      recordStart = Date.now() - 83_000;
      setTimeout(() => startLevels("s_demo"), 50);
      if (p === "handsfree") elapsedTimer = window.setInterval(() => setSession({ elapsedMs: Date.now() - recordStart }), 1000);
      break;
    case "transcribing":
      state.snapshot.session = { ...base, phase: "transcribing" };
      break;
    case "polishing":
      state.snapshot.session = { ...base, phase: "polishing" };
      break;
    case "result":
      state.snapshot.session = { ...base, phase: "result", result: { tone: "success", message: "Inserted in Notes", dictationId: state.history[0]?.id ?? null, canCopy: true } };
      break;
    case "result-warning":
      state.snapshot.session = { ...base, phase: "result", result: { tone: "warning", message: "AI unavailable; original text used", dictationId: state.history[0]?.id ?? null, canCopy: true } };
      break;
    case "result-error":
      state.snapshot.session = { ...base, phase: "result", result: { tone: "error", message: "Microphone access is off", dictationId: null, canCopy: false } };
      break;
  }
}
applyStateParam();

// ---------- helpers ----------

function wordCount(t: string) {
  return t.trim() ? t.trim().split(/\s+/).length : 0;
}

function computeStats(): Stats {
  const weekAgo = Date.now() - 7 * DAY;
  const valid = state.history.filter((e) => e.finalText.trim());
  const wordsThisWeek = valid.filter((e) => Date.parse(e.createdAtUtc) >= weekAgo).reduce((a, e) => a + wordCount(e.finalText), 0);
  const spoken = valid.filter((e) => e.source !== "import");
  const audioMs = spoken.reduce((a, e) => a + e.durationMs, 0);
  const words = spoken.reduce((a, e) => a + wordCount(e.originalText || e.finalText), 0);
  const speakingWpm = audioMs >= 5000 ? Math.round(words / (audioMs / MIN)) : null;
  // Mock inflates usage a little so the tile looks lived-in.
  const minutesSaved = Math.max(0, Math.round((words * 9) / 40 - (audioMs * 9) / MIN));
  const days = new Set(valid.map((e) => new Date(e.createdAtUtc).toDateString()));
  let streak = 0;
  const d = new Date();
  if (!days.has(d.toDateString())) d.setDate(d.getDate() - 1);
  while (days.has(d.toDateString())) {
    streak++;
    d.setDate(d.getDate() - 1);
  }
  return { wordsThisWeek, speakingWpm, minutesSaved, streakDays: streak, totalEntries: state.history.length };
}

const delay = <T>(v: T, ms = 60) => new Promise<T>((r) => setTimeout(() => r(v), ms));

const downloads = new Map<string, number>();

function installModel(id: string) {
  const m = state.models.find((x) => x.id === id);
  if (!m || m.installed || downloads.has(id)) return;
  m.download = { receivedBytes: 0, totalBytes: m.sizeBytes, state: "downloading", error: null };
  emit({ kind: "modelDownloadProgress", model: { ...m } });
  const t = window.setInterval(() => {
    if (!m.download) return;
    m.download = { ...m.download, receivedBytes: Math.min(m.sizeBytes, m.download.receivedBytes + m.sizeBytes / 40) };
    if (m.download.receivedBytes >= m.sizeBytes) {
      clearInterval(t);
      downloads.delete(id);
      m.download = { ...m.download, state: "verifying" };
      emit({ kind: "modelDownloadProgress", model: { ...m } });
      setTimeout(() => {
        m.download = null;
        m.installed = true;
        emit({ kind: "modelDownloadProgress", model: { ...m } });
        if (state.snapshot.engine.state === "noModel" || state.snapshot.settings.modelId === id) {
          state.snapshot.engine = { state: "ready", modelId: id, backend: "metal", message: null };
          emit({ kind: "engineChanged", engine: state.snapshot.engine });
        }
      }, 700);
      return;
    }
    emit({ kind: "modelDownloadProgress", model: { ...m } });
  }, 150);
  downloads.set(id, t);
}

function updateSettings(patch: SettingsPatch): Settings {
  state.snapshot.settings = { ...state.snapshot.settings, ...patch };
  emit({ kind: "settingsChanged", settings: state.snapshot.settings });
  return state.snapshot.settings;
}

let importTimer: number | null = null;
function runImports() {
  if (importTimer !== null) return;
  const step = () => {
    const jobs = state.snapshot.imports;
    const cur = jobs.find((j) => j.state === "copying" || j.state === "transcribing") ?? jobs.find((j) => j.state === "queued");
    if (!cur) {
      importTimer = null;
      return;
    }
    if (cur.state === "queued") cur.state = "copying";
    else if (cur.state === "copying") cur.state = "transcribing";
    else {
      cur.state = "done";
      const e = entry({
        createdAtUtc: new Date().toISOString(),
        source: "import",
        importName: cur.fileName,
        durationMs: 95_000,
        hasSegments: true,
        insertionState: "notRequested",
        finalText: "This is the transcript of the imported recording. It was transcribed locally and is ready to export as subtitles.",
      });
      cur.dictationId = e.id;
      state.history.unshift(e);
      emit({ kind: "historyEntryUpdated", entry: e });
    }
    state.snapshot.imports = jobs.map((j) => ({ ...j }));
    emit({ kind: "importsChanged", imports: state.snapshot.imports });
    importTimer = window.setTimeout(step, 1400);
  };
  importTimer = window.setTimeout(step, 400);
}

// ---------- command router ----------

export async function handleMockCommand(cmd: string, a: Record<string, unknown>): Promise<unknown> {
  const id = a.id as string;
  switch (cmd) {
    case "get_snapshot":
      return delay(structuredClone(state.snapshot), 30);
    case "start_dictation":
      startDictation(a.mode as RecordingMode);
      return delay(undefined, 10);
    case "finish_dictation":
      finishDictation();
      return delay(undefined, 10);
    case "cancel_session":
      cancelSession();
      return delay(undefined, 10);

    case "list_history": {
      const q = ((a.query as string | null) ?? "").trim().toLowerCase();
      const list = q
        ? state.history.filter(
            (e) =>
              e.finalText.toLowerCase().includes(q) ||
              e.originalText.toLowerCase().includes(q) ||
              (e.destinationAppName ?? "").toLowerCase().includes(q) ||
              (e.importName ?? "").toLowerCase().includes(q),
          )
        : state.history;
      return delay(structuredClone(list));
    }
    case "get_stats":
      return delay(computeStats());
    case "recover_entry": {
      const e = state.history.find((x) => x.id === id);
      if (!e) throw "Entry not found";
      e.processingState = "transcribing";
      emit({ kind: "historyEntryUpdated", entry: { ...e } });
      later(1500, () => {
        e.processingState = "done";
        e.errorMessage = null;
        e.errorCode = null;
        e.retryKind = "retry";
        if (!e.finalText) e.originalText = e.finalText = "Draft the release notes and send them to the team before the beta goes out.";
        emit({ kind: "historyEntryUpdated", entry: { ...e } });
      });
      return delay(undefined);
    }
    case "delete_entry":
      state.history = state.history.filter((x) => x.id !== id);
      state.snapshot.historyCount = state.history.length;
      emit({ kind: "historyEntryDeleted", id });
      return delay(undefined);
    case "copy_entry":
    case "copy_last":
    case "paste_last":
      return delay(undefined);
    case "import_audio": {
      const jobs: ImportJob[] = [
        { jobId: nextId(), fileName: "Interview — Maya.mp3", state: "queued", dictationId: null, error: null },
        { jobId: nextId(), fileName: "Voice memo 14.m4a", state: "queued", dictationId: null, error: null },
      ];
      state.snapshot.imports = [...state.snapshot.imports.filter((j) => j.state !== "done" && j.state !== "canceled"), ...jobs];
      emit({ kind: "importsChanged", imports: state.snapshot.imports });
      runImports();
      return delay(jobs.length);
    }
    case "cancel_import": {
      const cur = state.snapshot.imports.find((j) => j.state === "copying" || j.state === "transcribing");
      if (cur) cur.state = "canceled";
      state.snapshot.imports = state.snapshot.imports.map((j) => ({ ...j }));
      emit({ kind: "importsChanged", imports: state.snapshot.imports });
      return delay(undefined);
    }
    case "export_entry":
      return delay(true, 300);
    case "delete_all_data":
      if (a.confirmPhrase !== "DELETE") throw "Type DELETE to confirm.";
      state.history = [];
      state.snapshot.historyCount = 0;
      emit({ kind: "historyCleared" });
      return delay(undefined);

    case "list_models":
      return delay(structuredClone(state.models));
    case "install_model":
      installModel(id);
      return delay(undefined);
    case "cancel_model_install": {
      const t = downloads.get(id);
      if (t !== undefined) clearInterval(t);
      downloads.delete(id);
      const m = state.models.find((x) => x.id === id);
      if (m) {
        m.download = null;
        emit({ kind: "modelDownloadProgress", model: { ...m } });
      }
      return delay(undefined);
    }
    case "remove_model": {
      const m = state.models.find((x) => x.id === id);
      if (m) {
        m.installed = false;
        emit({ kind: "modelDownloadProgress", model: { ...m } });
      }
      return delay(undefined);
    }
    case "import_model_file":
      return delay(false, 200);
    case "list_inputs":
      return delay<AudioInput[]>([
        { id: "builtin", name: os === "macos" ? "MacBook Pro Microphone" : "Microphone Array (Realtek Audio)", isDefault: true },
        { id: "usb", name: "Shure MV7", isDefault: false },
        { id: "airpods", name: "AirPods Pro", isDefault: false },
      ]);

    case "update_settings":
      return delay(updateSettings(a.patch as SettingsPatch), 120);
    case "set_polish_enabled":
      return delay(updateSettings({ polishEnabled: a.enabled as boolean }));
    case "set_shortcut": {
      const action = a.action as ShortcutStatus["action"];
      const acc = a.accelerator as string;
      const conflict = acc === "Control+Alt+Delete" || acc === "Command+Space";
      state.snapshot.shortcuts = state.snapshot.shortcuts.map((s) =>
        s.action === action
          ? { action, accelerator: acc, registered: !conflict, error: conflict ? "Another app or the system already uses this shortcut." : null }
          : s,
      );
      state.snapshot.settings.shortcuts = state.snapshot.settings.shortcuts.map((s) => (s.action === action ? { action, accelerator: acc } : s));
      emit({ kind: "shortcutsChanged", shortcuts: state.snapshot.shortcuts });
      return delay(state.snapshot.shortcuts, 150);
    }
    case "suspend_shortcuts":
      return delay(undefined, 10);

    case "request_permission":
    case "open_permission_settings": {
      const kind = a.kind as keyof PermissionStatus;
      later(900, () => {
        state.snapshot.permissions = { ...state.snapshot.permissions, [kind]: "granted" };
        emit({ kind: "permissionsChanged", permissions: state.snapshot.permissions });
      });
      return delay(undefined);
    }
    case "refresh_permissions":
      return delay(undefined);

    case "list_profiles":
      return delay(structuredClone(state.profiles));
    case "save_profile": {
      const p = a.profile as ProviderProfile;
      const prof = { ...p, id: p.id || `p_${nextId()}` };
      const i = state.profiles.findIndex((x) => x.id === prof.id);
      const prev = i >= 0 ? state.profiles[i] : null;
      if (i >= 0) state.profiles[i] = prof;
      else state.profiles.push(prof);
      const c = state.creds[prof.id] ?? { profileId: prof.id, bearerSaved: false, savedSecretHeaders: [], needsReconnect: false };
      if (prev && new URL(prev.baseUrl).origin !== new URL(prof.baseUrl).origin && c.bearerSaved) c.needsReconnect = true;
      state.creds[prof.id] = c;
      return delay(structuredClone(prof), 150);
    }
    case "delete_profile":
      state.profiles = state.profiles.filter((p) => p.id !== id);
      delete state.creds[id];
      if (state.snapshot.settings.activeProfileId === id) updateSettings({ activeProfileId: null, polishEnabled: false });
      return delay(undefined);
    case "set_active_profile":
      return delay(updateSettings({ activeProfileId: a.id as string | null }));
    case "credential_status": {
      const pid = a.profileId as string;
      return delay(state.creds[pid] ?? { profileId: pid, bearerSaved: false, savedSecretHeaders: [], needsReconnect: false });
    }
    case "save_credential": {
      const pid = a.profileId as string;
      const c = state.creds[pid] ?? { profileId: pid, bearerSaved: false, savedSecretHeaders: [], needsReconnect: false };
      if (a.name === null) c.bearerSaved = true;
      else if (!c.savedSecretHeaders.includes(a.name as string)) c.savedSecretHeaders = [...c.savedSecretHeaders, a.name as string];
      c.needsReconnect = false;
      state.creds[pid] = c;
      return delay({ ...c }, 200);
    }
    case "delete_credential": {
      const pid = a.profileId as string;
      const c = state.creds[pid] ?? { profileId: pid, bearerSaved: false, savedSecretHeaders: [], needsReconnect: false };
      if (a.name === null) c.bearerSaved = false;
      else c.savedSecretHeaders = c.savedSecretHeaders.filter((n) => n !== a.name);
      state.creds[pid] = c;
      return delay({ ...c });
    }
    case "refresh_provider_models":
      return delay<ModelSummary[]>(
        [
          { id: "meta-llama/llama-3.3-70b-instruct", name: "Llama 3.3 70B Instruct", contextLength: 131072 },
          { id: "mistralai/mistral-small-3.2-24b-instruct", name: "Mistral Small 3.2", contextLength: 128000 },
          { id: "qwen/qwen3-32b", name: "Qwen3 32B", contextLength: 40960 },
          { id: "google/gemma-3-27b-it", name: "Gemma 3 27B", contextLength: 131072 },
        ],
        700,
      );
    case "test_connection": {
      const pid = a.profileId as string;
      const ok = state.creds[pid]?.bearerSaved || state.profiles.find((p) => p.id === pid)?.authMode === "none";
      return delay<ConnectionResult>(
        ok ? { ok: true, message: "Connected. The model replied to a test message.", latencyMs: 412 } : { ok: false, message: "The server rejected the request (401). Check the API key.", latencyMs: null },
        900,
      );
    }

    case "open_main":
      emit({ kind: "navigate", page: a.page as "home" });
      return delay(undefined);
    case "reset_bar_position":
    case "reset_permissions":
      return delay("Access was reset. Allow the microphone, then turn on Sayvane under Accessibility.", 200);
    case "show_bar_menu":
    case "set_bar_hit_region":
    case "reveal_data_dir":
    case "open_url":
      return delay(undefined);
  }
  throw new Error(`Mock: unknown command ${cmd}`);
}
