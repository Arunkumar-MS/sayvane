// Typed wrappers for every Tauri command exposed by src-tauri/src/commands.rs.
// The UI never owns device handles, database paths, or secrets: it only asks.
import { invoke } from "@tauri-apps/api/core";
import type {
  AppSnapshot,
  AudioInput,
  ConnectionResult,
  CredentialStatus,
  ExportFormat,
  HistoryEntry,
  ModelInfo,
  ModelSummary,
  Page,
  PermissionKind,
  ProviderProfile,
  RecordingMode,
  Settings,
  SettingsPatch,
  ShortcutAction,
  ShortcutStatus,
  Stats,
  TextVariant,
} from "./types";

export const api = {
  // Snapshot / session
  getSnapshot: () => invoke<AppSnapshot>("get_snapshot"),
  startDictation: (mode: RecordingMode) => invoke<void>("start_dictation", { mode }),
  finishDictation: () => invoke<void>("finish_dictation"),
  cancelSession: () => invoke<void>("cancel_session"),

  // History
  listHistory: (query: string | null) => invoke<HistoryEntry[]>("list_history", { query }),
  getStats: () => invoke<Stats>("get_stats"),
  recoverEntry: (id: string) => invoke<void>("recover_entry", { id }),
  deleteEntry: (id: string) => invoke<void>("delete_entry", { id }),
  copyEntry: (id: string, variant: TextVariant) => invoke<void>("copy_entry", { id, variant }),
  copyLast: () => invoke<void>("copy_last"),
  pasteLast: () => invoke<void>("paste_last"),
  /** Opens the OS file picker in the core and queues the chosen files. */
  importAudio: () => invoke<number>("import_audio"),
  cancelImport: () => invoke<void>("cancel_import"),
  /** Opens the OS save dialog in the core. Returns false if the user canceled. */
  exportEntry: (id: string, format: ExportFormat, variant: TextVariant) =>
    invoke<boolean>("export_entry", { id, format, variant }),
  deleteAllData: (confirmPhrase: string) => invoke<void>("delete_all_data", { confirmPhrase }),

  // Models & audio
  listModels: () => invoke<ModelInfo[]>("list_models"),
  installModel: (id: string) => invoke<void>("install_model", { id }),
  cancelModelInstall: (id: string) => invoke<void>("cancel_model_install", { id }),
  removeModel: (id: string) => invoke<void>("remove_model", { id }),
  /** Opens a file picker in the core for an existing compatible model file. */
  importModelFile: (id: string) => invoke<boolean>("import_model_file", { id }),
  listInputs: () => invoke<AudioInput[]>("list_inputs"),

  // Settings
  updateSettings: (patch: SettingsPatch) => invoke<Settings>("update_settings", { patch }),
  setPolishEnabled: (enabled: boolean) => invoke<Settings>("set_polish_enabled", { enabled }),
  setShortcut: (action: ShortcutAction, accelerator: string) =>
    invoke<ShortcutStatus[]>("set_shortcut", { action, accelerator }),
  /** Suspend global shortcuts while the user records a replacement in Settings. */
  suspendShortcuts: (suspended: boolean) => invoke<void>("suspend_shortcuts", { suspended }),

  // Permissions
  requestPermission: (kind: PermissionKind) => invoke<void>("request_permission", { kind }),
  openPermissionSettings: (kind: PermissionKind) => invoke<void>("open_permission_settings", { kind }),
  refreshPermissions: () => invoke<void>("refresh_permissions"),

  // Providers
  listProfiles: () => invoke<ProviderProfile[]>("list_profiles"),
  saveProfile: (profile: ProviderProfile) => invoke<ProviderProfile>("save_profile", { profile }),
  deleteProfile: (id: string) => invoke<void>("delete_profile", { id }),
  setActiveProfile: (id: string | null) => invoke<Settings>("set_active_profile", { id }),
  credentialStatus: (profileId: string) => invoke<CredentialStatus>("credential_status", { profileId }),
  /** name = null saves the bearer key; otherwise a secret header value. */
  saveCredential: (profileId: string, name: string | null, value: string) =>
    invoke<CredentialStatus>("save_credential", { profileId, name, value }),
  deleteCredential: (profileId: string, name: string | null) =>
    invoke<CredentialStatus>("delete_credential", { profileId, name }),
  refreshProviderModels: (profileId: string) => invoke<ModelSummary[]>("refresh_provider_models", { profileId }),
  testConnection: (profileId: string) => invoke<ConnectionResult>("test_connection", { profileId }),

  // Windows & misc
  openMain: (page: Page) => invoke<void>("open_main", { page }),
  resetBarPosition: () => invoke<void>("reset_bar_position"),
  showBarMenu: () => invoke<void>("show_bar_menu"),
  /** Reports the capsule's rect (CSS px, window-relative) so clicks outside it pass through. */
  setBarHitRegion: (x: number, y: number, width: number, height: number) =>
    invoke<void>("set_bar_hit_region", { x, y, width, height }),
  revealDataDir: () => invoke<void>("reveal_data_dir"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
};

export type Api = typeof api;
