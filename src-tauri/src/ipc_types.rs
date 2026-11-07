//! Serde twins of `ui/src/ipc/types.ts`. Field names are camelCase on the wire and
//! tagged unions use `kind`. Change both files together.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Brand {
    pub display_name: String,
    pub product_slug: String,
    pub publisher: String,
    pub application_id: String,
    #[serde(default)]
    pub storage_namespace: String,
    pub tagline: String,
    pub accent_color: String,
    #[serde(default)]
    pub icon_source: String,
    pub support_url: Option<String>,
    pub privacy_url: Option<String>,
}

// ---------- Session ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingMode {
    Hold,
    HandsFree,
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionPhase {
    Idle,
    Starting,
    Recording,
    SavingAudio,
    Transcribing,
    Polishing,
    SavingTranscript,
    Delivering,
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultTone {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResult {
    pub tone: ResultTone,
    pub message: String,
    pub dictation_id: Option<String>,
    pub can_copy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub phase: SessionPhase,
    pub session_id: Option<String>,
    pub generation: u64,
    pub mode: Option<RecordingMode>,
    pub elapsed_ms: Option<u64>,
    pub destination_label: Option<String>,
    pub cancel_pending: bool,
    pub result: Option<SessionResult>,
    pub busy_reason: Option<String>,
}

impl Default for SessionView {
    fn default() -> Self {
        Self {
            phase: SessionPhase::Idle,
            session_id: None,
            generation: 0,
            mode: None,
            elapsed_ms: None,
            destination_label: None,
            cancel_pending: false,
            result: None,
            busy_reason: None,
        }
    }
}

// ---------- History ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DictationSource {
    Dictation,
    Note,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioState {
    Capturing,
    Saved,
    Interrupted,
    Missing,
    RemovedByRetention,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessingState {
    Recording,
    Queued,
    Transcribing,
    Polishing,
    Done,
    NoSpeech,
    Failed,
    Canceled,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PolishState {
    Off,
    Skipped,
    Applied,
    Fallback,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsertionState {
    NotRequested,
    Confirmed,
    SentUnconfirmed,
    SavedOnly,
    DestinationChanged,
    Ineligible,
    Canceled,
}

/// Round-trips the enums above through their camelCase wire names so SQLite stores
/// the same identifiers the UI sees.
pub fn enum_to_str<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

pub fn enum_from_str<T: for<'de> Deserialize<'de>>(s: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub p: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub id: i64,
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<Word>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RetryKind {
    Recover,
    Retry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub created_at_utc: String,
    pub source: DictationSource,
    pub audio_state: AudioState,
    pub duration_ms: i64,
    pub original_text: String,
    pub polished_text: Option<String>,
    pub final_text: String,
    pub detected_language: Option<String>,
    pub model_id: String,
    pub destination_app_name: Option<String>,
    pub processing_state: ProcessingState,
    pub polish_state: PolishState,
    pub insertion_state: InsertionState,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub has_segments: bool,
    pub can_retry: bool,
    pub retry_unavailable_reason: Option<String>,
    pub retry_kind: Option<RetryKind>,
    pub import_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub words_this_week: u64,
    pub speaking_wpm: Option<f64>,
    pub minutes_saved: f64,
    pub streak_days: u32,
    pub total_entries: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextVariant {
    Final,
    Original,
    Polished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Txt,
    Json,
    Srt,
    Vtt,
    Tsv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportState {
    Queued,
    Copying,
    Transcribing,
    Done,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportJob {
    pub job_id: String,
    pub file_name: String,
    pub state: ImportState,
    pub dictation_id: Option<String>,
    pub error: Option<String>,
}

// ---------- Models / engine ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub state: String, // downloading | verifying | failed
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    pub description: String,
    pub size_bytes: u64,
    pub multilingual: bool,
    pub can_translate: bool,
    pub installed: bool,
    pub is_default: bool,
    pub recommended_min_memory_gb: f64,
    pub download: Option<DownloadProgress>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineState {
    NoModel,
    Cold,
    Loading,
    Ready,
    Busy,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub state: EngineState,
    pub model_id: Option<String>,
    pub backend: Option<String>,
    pub message: Option<String>,
}

// ---------- Settings ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RetentionPolicy {
    Keep,
    AfterTranscription,
    Days7,
    Days30,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutAction {
    PushToTalk,
    ToggleHandsFree,
    PasteLast,
    CopyLast,
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutBinding {
    pub action: ShortcutAction,
    pub accelerator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    pub action: ShortcutAction,
    pub accelerator: String,
    pub registered: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedRecognition {
    pub temperature: f64,
    pub beam_size: Option<u32>,
    pub word_timestamps: bool,
    pub no_context: bool,
    pub no_speech_threshold: f64,
    pub threads: u32,
}

impl Default for AdvancedRecognition {
    fn default() -> Self {
        Self { temperature: 0.0, beam_size: None, word_timestamps: false, no_context: true, no_speech_threshold: 0.6, threads: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Task {
    Transcribe,
    Translate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReducedMotion {
    System,
    Reduce,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub schema_version: u32,
    pub model_id: String,
    pub language: Option<String>,
    pub task: Task,
    pub vocabulary_hints: String,
    pub microphone_id: Option<String>,
    pub advanced: AdvancedRecognition,
    pub polish_enabled: bool,
    pub active_profile_id: Option<String>,
    pub shortcuts: Vec<ShortcutBinding>,
    pub fn_gesture_enabled: bool,
    pub retention: RetentionPolicy,
    pub launch_at_login: bool,
    pub reduced_motion: ReducedMotion,
    pub sounds_enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub model_id: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub language: Option<Option<String>>,
    pub task: Option<Task>,
    pub vocabulary_hints: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub microphone_id: Option<Option<String>>,
    pub advanced: Option<AdvancedRecognition>,
    pub polish_enabled: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    pub active_profile_id: Option<Option<String>>,
    pub shortcuts: Option<Vec<ShortcutBinding>>,
    pub fn_gesture_enabled: Option<bool>,
    pub retention: Option<RetentionPolicy>,
    pub launch_at_login: Option<bool>,
    pub reduced_motion: Option<ReducedMotion>,
    pub sounds_enabled: Option<bool>,
}

/// Distinguishes a missing field (no change) from an explicit `null` (clear the value).
fn double_option<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInput {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

// ---------- Permissions ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionKind {
    Microphone,
    Accessibility,
    InputMonitoring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionState {
    Granted,
    Denied,
    NotDetermined,
    NotApplicable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub microphone: PermissionState,
    pub accessibility: PermissionState,
    pub input_monitoring: PermissionState,
}

// ---------- Providers ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Openrouter,
    Compatible,
    Organization,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthMode {
    Bearer,
    Headers,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub label: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub chat_path: String,
    pub models_path: Option<String>,
    pub model_id: String,
    pub auth_mode: AuthMode,
    pub headers: BTreeMap<String, String>,
    pub secret_header_names: Vec<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub profile_id: String,
    pub bearer_saved: bool,
    pub saved_secret_headers: Vec<String>,
    pub needs_reconnect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    pub id: String,
    pub name: Option<String>,
    pub context_length: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionResult {
    pub ok: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
}

// ---------- Snapshot & events ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Platform {
    pub os: String,
    pub arch: String,
    pub os_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub data_dir: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub brand: Brand,
    pub platform: Platform,
    pub session: SessionView,
    pub settings: Settings,
    pub permissions: PermissionStatus,
    pub engine: EngineStatus,
    pub shortcuts: Vec<ShortcutStatus>,
    pub storage: StorageInfo,
    pub history_count: u64,
    pub imports: Vec<ImportJob>,
    pub event_seq: u64,
    pub settings_warning: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Page {
    Home,
    History,
    Settings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AppEvent {
    #[serde(rename_all = "camelCase")]
    SessionStateChanged { session: SessionView },
    #[serde(rename_all = "camelCase")]
    AudioLevel { session_id: String, level: f32 },
    #[serde(rename_all = "camelCase")]
    HistoryEntryUpdated { entry: HistoryEntry },
    #[serde(rename_all = "camelCase")]
    HistoryEntryDeleted { id: String },
    HistoryCleared,
    #[serde(rename_all = "camelCase")]
    PermissionsChanged { permissions: PermissionStatus },
    #[serde(rename_all = "camelCase")]
    EngineChanged { engine: EngineStatus },
    #[serde(rename_all = "camelCase")]
    ModelDownloadProgress { model: ModelInfo },
    #[serde(rename_all = "camelCase")]
    SettingsChanged { settings: Settings },
    #[serde(rename_all = "camelCase")]
    ShortcutsChanged { shortcuts: Vec<ShortcutStatus> },
    #[serde(rename_all = "camelCase")]
    ImportsChanged { imports: Vec<ImportJob> },
    #[serde(rename_all = "camelCase")]
    StorageError { message: String },
    #[serde(rename_all = "camelCase")]
    Navigate { page: Page },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    pub schema_version: u32,
    pub seq: u64,
    pub event: AppEvent,
}

pub const EVENT_CHANNEL: &str = "app://event";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_use_ui_wire_names() {
        assert_eq!(enum_to_str(&InsertionState::SentUnconfirmed), "sentUnconfirmed");
        assert_eq!(enum_to_str(&AudioState::RemovedByRetention), "removedByRetention");
        assert_eq!(enum_from_str::<ProcessingState>("noSpeech"), Some(ProcessingState::NoSpeech));
        assert_eq!(enum_to_str(&ProviderKind::Openrouter), "openrouter");
    }

    #[test]
    fn events_are_kind_tagged() {
        let v = serde_json::to_value(AppEvent::AudioLevel { session_id: "s".into(), level: 0.5 }).unwrap();
        assert_eq!(v["kind"], "audioLevel");
        assert_eq!(v["sessionId"], "s");
        let v = serde_json::to_value(AppEvent::HistoryCleared).unwrap();
        assert_eq!(v["kind"], "historyCleared");
    }

    #[test]
    fn patch_distinguishes_null_from_missing() {
        let p: SettingsPatch = serde_json::from_str(r#"{"language":null}"#).unwrap();
        assert_eq!(p.language, Some(None));
        let p: SettingsPatch = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(p.language, None);
    }
}
