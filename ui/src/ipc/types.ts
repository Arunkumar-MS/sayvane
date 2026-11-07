// IPC contract between the React UI and the Rust core.
// Every type here has a serde twin in src-tauri/src/ipc_types.rs (camelCase fields,
// tagged unions use `kind`). Change both sides together.

export type Os = "macos" | "windows" | "other";
export type Arch = "aarch64" | "x86_64" | "other";

export interface Brand {
  displayName: string;
  productSlug: string;
  publisher: string;
  applicationId: string;
  tagline: string;
  accentColor: string;
  supportUrl: string | null;
  privacyUrl: string | null;
}

// ---------- Session ----------

export type RecordingMode = "hold" | "handsFree" | "note";

export type SessionPhase =
  | "idle"
  | "starting"
  | "recording"
  | "savingAudio"
  | "transcribing"
  | "polishing"
  | "savingTranscript"
  | "delivering"
  | "result";

export type ResultTone = "success" | "info" | "warning" | "error";

export interface SessionResult {
  tone: ResultTone;
  /** Short readable message, e.g. "Inserted in Notes" or "AI unavailable; original text used". */
  message: string;
  dictationId: string | null;
  /** True when the transcript can be copied from the bar. */
  canCopy: boolean;
}

export interface SessionView {
  phase: SessionPhase;
  sessionId: string | null;
  generation: number;
  mode: RecordingMode | null;
  /** Milliseconds since capture became active; null when not recording. */
  elapsedMs: number | null;
  /** Destination app name captured at start, or "History" for notes. */
  destinationLabel: string | null;
  cancelPending: boolean;
  result: SessionResult | null;
  /** Human message when an operation was refused because work is in progress. */
  busyReason: string | null;
}

// ---------- History ----------

export type DictationSource = "dictation" | "note" | "import";
export type AudioState = "capturing" | "saved" | "interrupted" | "missing" | "removedByRetention" | "none";
export type ProcessingState =
  | "recording"
  | "queued"
  | "transcribing"
  | "polishing"
  | "done"
  | "noSpeech"
  | "failed"
  | "canceled"
  | "interrupted";
export type PolishState = "off" | "skipped" | "applied" | "fallback" | "pending";
export type InsertionState =
  | "notRequested"
  | "confirmed"
  | "sentUnconfirmed"
  | "savedOnly"
  | "destinationChanged"
  | "ineligible"
  | "canceled";

export interface Segment {
  id: number;
  start: number; // seconds
  end: number;
  text: string;
}

export interface HistoryEntry {
  id: string;
  createdAtUtc: string; // ISO-8601
  source: DictationSource;
  audioState: AudioState;
  durationMs: number;
  originalText: string;
  polishedText: string | null;
  finalText: string;
  detectedLanguage: string | null;
  modelId: string;
  destinationAppName: string | null;
  processingState: ProcessingState;
  polishState: PolishState;
  insertionState: InsertionState;
  errorCode: string | null;
  errorMessage: string | null;
  hasSegments: boolean;
  /** Derived by the core: whether Recover/Retry may run, and why not. */
  canRetry: boolean;
  retryUnavailableReason: string | null;
  /** "recover" when there is audio but no usable transcript, "retry" otherwise. */
  retryKind: "recover" | "retry" | null;
  importName: string | null;
}

export interface Stats {
  wordsThisWeek: number;
  /** Words per minute, null when under 5 s of valid audio. */
  speakingWpm: number | null;
  /** Estimated minutes saved vs typing at 40 wpm. */
  minutesSaved: number;
  streakDays: number;
  totalEntries: number;
}

export type TextVariant = "final" | "original" | "polished";
export type ExportFormat = "txt" | "json" | "srt" | "vtt" | "tsv";

export interface ImportJob {
  jobId: string;
  fileName: string;
  state: "queued" | "copying" | "transcribing" | "done" | "failed" | "canceled";
  dictationId: string | null;
  error: string | null;
}

// ---------- Models / engine ----------

export interface ModelInfo {
  id: string; // e.g. "base"
  label: string; // "Balanced"
  description: string;
  sizeBytes: number;
  multilingual: boolean;
  canTranslate: boolean;
  installed: boolean;
  isDefault: boolean;
  recommendedMinMemoryGb: number;
  /** Present while downloading. */
  download: { receivedBytes: number; totalBytes: number; state: "downloading" | "verifying" | "failed"; error: string | null } | null;
}

export interface EngineStatus {
  state: "noModel" | "cold" | "loading" | "ready" | "busy" | "error";
  modelId: string | null;
  backend: "metal" | "cpu" | null;
  message: string | null;
}

// ---------- Settings ----------

export type RetentionPolicy = "keep" | "afterTranscription" | "days7" | "days30";
export type ShortcutAction = "pushToTalk" | "toggleHandsFree" | "pasteLast" | "copyLast" | "cancel";

export interface ShortcutBinding {
  action: ShortcutAction;
  /** Tauri accelerator string, e.g. "Control+Alt+Space". "Fn" for the macOS Fn gesture. */
  accelerator: string;
}

export interface ShortcutStatus {
  action: ShortcutAction;
  accelerator: string;
  registered: boolean;
  error: string | null;
}

export interface AdvancedRecognition {
  temperature: number;
  beamSize: number | null; // null = greedy
  wordTimestamps: boolean;
  noContext: boolean;
  noSpeechThreshold: number;
  threads: number; // 0 = auto
}

export interface Settings {
  schemaVersion: number;
  modelId: string;
  language: string | null; // null = auto
  task: "transcribe" | "translate";
  vocabularyHints: string;
  microphoneId: string | null; // null = system default
  advanced: AdvancedRecognition;
  polishEnabled: boolean;
  activeProfileId: string | null;
  shortcuts: ShortcutBinding[];
  fnGestureEnabled: boolean; // macOS only
  retention: RetentionPolicy;
  launchAtLogin: boolean;
  reducedMotion: "system" | "reduce" | "full";
  soundsEnabled: boolean;
}

export type SettingsPatch = Partial<Omit<Settings, "schemaVersion">>;

export interface AudioInput {
  id: string;
  name: string;
  isDefault: boolean;
}

// ---------- Permissions ----------

export type PermissionKind = "microphone" | "accessibility" | "inputMonitoring";
export type PermissionState = "granted" | "denied" | "notDetermined" | "notApplicable" | "unknown";

export interface PermissionStatus {
  microphone: PermissionState;
  accessibility: PermissionState;
  inputMonitoring: PermissionState;
}

// ---------- Providers ----------

export type ProviderKind = "openrouter" | "compatible" | "organization" | "local";
export type AuthMode = "bearer" | "headers" | "none";

export interface ProviderProfile {
  id: string;
  label: string;
  kind: ProviderKind;
  baseUrl: string;
  chatPath: string;
  modelsPath: string | null;
  modelId: string;
  authMode: AuthMode;
  headers: Record<string, string>;
  /** Names of secret headers that are stored in the OS vault. Values never cross IPC. */
  secretHeaderNames: string[];
  timeoutMs: number;
}

export interface CredentialStatus {
  profileId: string;
  bearerSaved: boolean;
  savedSecretHeaders: string[];
  /** True when the endpoint origin changed since the credential was saved. */
  needsReconnect: boolean;
}

export interface ModelSummary {
  id: string;
  name: string | null;
  contextLength: number | null;
}

export interface ConnectionResult {
  ok: boolean;
  message: string;
  latencyMs: number | null;
}

// ---------- Snapshot & events ----------

export interface Platform {
  os: Os;
  arch: Arch;
  osVersion: string;
}

export interface StorageInfo {
  dataDir: string;
  ok: boolean;
  error: string | null;
}

export interface AppSnapshot {
  brand: Brand;
  platform: Platform;
  session: SessionView;
  settings: Settings;
  permissions: PermissionStatus;
  engine: EngineStatus;
  shortcuts: ShortcutStatus[];
  storage: StorageInfo;
  historyCount: number;
  imports: ImportJob[];
  eventSeq: number;
  /** Set when settings were recovered from a malformed file. */
  settingsWarning: string | null;
}

export type AppEvent =
  | { kind: "sessionStateChanged"; session: SessionView }
  | { kind: "audioLevel"; sessionId: string; level: number }
  | { kind: "historyEntryUpdated"; entry: HistoryEntry }
  | { kind: "historyEntryDeleted"; id: string }
  | { kind: "historyCleared" }
  | { kind: "permissionsChanged"; permissions: PermissionStatus }
  | { kind: "engineChanged"; engine: EngineStatus }
  | { kind: "modelDownloadProgress"; model: ModelInfo }
  | { kind: "settingsChanged"; settings: Settings }
  | { kind: "shortcutsChanged"; shortcuts: ShortcutStatus[] }
  | { kind: "importsChanged"; imports: ImportJob[] }
  | { kind: "storageError"; message: string }
  | { kind: "navigate"; page: Page };

export interface EventEnvelope {
  schemaVersion: 1;
  seq: number;
  event: AppEvent;
}

export type Page = "home" | "history" | "settings";

/** Name of the single Tauri event channel carrying EventEnvelope payloads. */
export const EVENT_CHANNEL = "app://event";
