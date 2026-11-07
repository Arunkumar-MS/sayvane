//! Narrow IPC commands. The UI asks; the core owns devices, files, and secrets.
//! Mirrors `ui/src/ipc/commands.ts`.

use crate::core::CoreRef;
use crate::credentials::{bearer_account, header_account, SecretStore};
use crate::ipc_types::*;
use crate::shell;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

type R<T> = Result<T, String>;

// ---------- snapshot / session ----------

#[tauri::command]
pub fn get_snapshot(core: State<CoreRef>) -> AppSnapshot {
    core.snapshot()
}

#[tauri::command]
pub fn start_dictation(core: State<CoreRef>, mode: RecordingMode) {
    core.start_dictation(mode);
}

#[tauri::command]
pub fn finish_dictation(core: State<CoreRef>) {
    core.finish_dictation();
}

#[tauri::command]
pub fn cancel_session(core: State<CoreRef>) {
    core.cancel_session();
}

// ---------- history ----------

#[tauri::command]
pub fn list_history(core: State<CoreRef>, query: Option<String>) -> R<Vec<HistoryEntry>> {
    let rows = core.db()?.list(query.as_deref())?;
    Ok(rows.iter().map(|r| core.entry_for(r)).collect())
}

#[tauri::command]
pub fn get_stats(core: State<CoreRef>) -> R<Stats> {
    let rows = core.db()?.list(None)?;
    Ok(crate::stats::compute_now(&rows))
}

#[tauri::command]
pub fn recover_entry(core: State<CoreRef>, id: String) -> R<()> {
    core.recover_entry(&id)
}

#[tauri::command]
pub fn delete_entry(core: State<CoreRef>, id: String) -> R<()> {
    core.delete_entry(&id)
}

#[tauri::command]
pub fn copy_entry(core: State<CoreRef>, id: String, variant: TextVariant) -> R<()> {
    core.copy_entry(&id, variant)
}

#[tauri::command]
pub fn copy_last(core: State<CoreRef>) -> R<()> {
    core.copy_last()
}

#[tauri::command]
pub fn paste_last(core: State<CoreRef>) -> R<()> {
    core.paste_last()
}

#[tauri::command]
pub async fn import_audio(app: AppHandle, core: State<'_, CoreRef>) -> R<usize> {
    let picked = app
        .dialog()
        .file()
        .set_title("Import audio")
        .add_filter("Audio", crate::audio::decode::SUPPORTED_EXTENSIONS)
        .blocking_pick_files();
    let Some(files) = picked else { return Ok(0) };
    let paths: Vec<_> = files.into_iter().filter_map(|f| f.into_path().ok()).collect();
    core.queue_imports(paths)
}

#[tauri::command]
pub fn cancel_import(core: State<CoreRef>) {
    core.cancel_background();
}

#[tauri::command]
pub async fn export_entry(
    app: AppHandle,
    core: State<'_, CoreRef>,
    id: String,
    format: ExportFormat,
    variant: TextVariant,
) -> R<bool> {
    // Validate before asking for a destination.
    let row = core.db()?.get(&id)?.ok_or("That entry no longer exists.")?;
    crate::export::render(&row, format, variant)?;
    let ext = crate::export::extension(format);
    let picked = app
        .dialog()
        .file()
        .set_title("Export transcript")
        .set_file_name(core.suggested_export_name(&id, format))
        .add_filter(ext.to_uppercase(), &[ext])
        .blocking_save_file();
    let Some(dest) = picked.and_then(|p| p.into_path().ok()) else { return Ok(false) };
    core.export_entry(&id, format, variant, &dest)?;
    Ok(true)
}

#[tauri::command]
pub fn delete_all_data(core: State<CoreRef>, confirm_phrase: String) -> R<()> {
    core.delete_all(&confirm_phrase)
}

// ---------- models & audio ----------

#[tauri::command]
pub fn list_models(core: State<CoreRef>) -> Vec<ModelInfo> {
    core.models.infos()
}

#[tauri::command]
pub fn install_model(core: State<CoreRef>, id: String) -> R<()> {
    crate::speech::models::find(&id).ok_or("Unknown model.")?;
    core.models.clear_failed(&id);
    let c = core.inner().clone();
    tauri::async_runtime::spawn(async move {
        let emit = |m: ModelInfo| c.emit(AppEvent::ModelDownloadProgress { model: m });
        let result = c.models.download(&c.download_http, &id, &emit).await;
        if result.is_ok() {
            c.refresh_engine_status();
            if c.settings().model_id == id {
                c.warm_in_background();
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_model_install(core: State<CoreRef>, id: String) {
    core.models.cancel(&id);
}

#[tauri::command]
pub fn remove_model(core: State<CoreRef>, id: String) -> R<()> {
    if !core.session.lock().is_idle() || core.background.lock().is_some() {
        return Err("Wait for the current job to finish.".into());
    }
    let path = core.models.path_if_installed(&id);
    if path.is_some() && core.speech.loaded_model() == path {
        // Release the file (Windows locks open files) before deleting.
        core.speech.shutdown();
    }
    core.models.remove(&id)?;
    core.refresh_engine_status();
    if let Some(m) = core.models.info(&id) {
        core.emit(AppEvent::ModelDownloadProgress { model: m });
    }
    Ok(())
}

#[tauri::command]
pub async fn import_model_file(app: AppHandle, core: State<'_, CoreRef>, id: String) -> R<bool> {
    let m = crate::speech::models::find(&id).ok_or("Unknown model.")?;
    let picked = app
        .dialog()
        .file()
        .set_title(format!("Choose the {} model file ({})", m.label, m.file_name))
        .add_filter("Model", &["bin"])
        .blocking_pick_file();
    let Some(src) = picked.and_then(|p| p.into_path().ok()) else { return Ok(false) };
    let c = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || c.models.import_file(&id, &src).map(|_| c))
        .await
        .map_err(|e| e.to_string())?
        .map(|c| {
            c.refresh_engine_status();
            c.warm_in_background();
            for m in c.models.infos() {
                c.emit(AppEvent::ModelDownloadProgress { model: m });
            }
            true
        })
}

#[tauri::command]
pub fn list_inputs() -> Vec<AudioInput> {
    crate::audio::list_inputs()
}

// ---------- settings ----------

#[tauri::command]
pub fn update_settings(app: AppHandle, core: State<CoreRef>, patch: SettingsPatch) -> R<Settings> {
    if patch.polish_enabled == Some(true) {
        shell::set_polish(core.inner(), true)?;
    }
    let launch = patch.launch_at_login;
    let model_changed = patch.model_id.is_some();
    let shortcuts_changed = patch.shortcuts.is_some() || patch.fn_gesture_enabled.is_some();
    if let Some(m) = &patch.model_id {
        crate::speech::models::find(m).ok_or("Unknown model.")?;
    }
    let s = core.update_settings(patch)?;
    if let Some(on) = launch {
        use tauri_plugin_autostart::ManagerExt;
        let al = app.autolaunch();
        let r = if on { al.enable() } else { al.disable() };
        if let Err(e) = r {
            return Err(format!("Launch at login could not be changed: {e}"));
        }
    }
    if model_changed {
        core.warm_in_background();
    }
    if shortcuts_changed {
        shell::register_shortcuts(&core);
    }
    if s.retention != RetentionPolicy::Keep {
        core.apply_retention();
    }
    Ok(s)
}

#[tauri::command]
pub fn set_polish_enabled(core: State<CoreRef>, enabled: bool) -> R<Settings> {
    shell::set_polish(core.inner(), enabled)
}

#[tauri::command]
pub fn set_shortcut(core: State<CoreRef>, action: ShortcutAction, accelerator: String) -> R<Vec<ShortcutStatus>> {
    shell::set_shortcut(&core, action, &accelerator)
}

#[tauri::command]
pub fn suspend_shortcuts(core: State<CoreRef>, suspended: bool) {
    core.shortcuts_suspended.store(suspended, Ordering::SeqCst);
    shell::register_shortcuts(&core);
}

// ---------- permissions ----------

#[tauri::command]
pub fn request_permission(core: State<CoreRef>, kind: PermissionKind) {
    core.desktop.request_permission(kind);
    let c = core.inner().clone();
    std::thread::spawn(move || {
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            c.refresh_permissions();
        }
    });
}

#[tauri::command]
pub fn open_permission_settings(core: State<CoreRef>, kind: PermissionKind) {
    core.desktop.open_permission_settings(kind);
}

#[tauri::command]
pub fn refresh_permissions(core: State<CoreRef>) {
    core.refresh_permissions();
}

// ---------- providers ----------

fn profile(core: &CoreRef, id: &str) -> R<ProviderProfile> {
    core.profiles.lock().get(id).cloned().ok_or_else(|| "That provider no longer exists.".into())
}

fn emit_provider(core: &CoreRef) {
    // Provider changes surface through settings (active profile) and command results.
    core.emit(AppEvent::SettingsChanged { settings: core.settings() });
}

#[tauri::command]
pub fn list_profiles(core: State<CoreRef>) -> Vec<ProviderProfile> {
    core.profiles.lock().data.profiles.clone()
}

#[tauri::command]
pub fn save_profile(core: State<CoreRef>, profile: ProviderProfile) -> R<ProviderProfile> {
    let saved = core.profiles.lock().upsert(profile)?;
    if core.settings().active_profile_id.is_none() {
        core.update_settings(SettingsPatch { active_profile_id: Some(Some(saved.id.clone())), ..Default::default() })?;
    }
    emit_provider(&core);
    Ok(saved)
}

#[tauri::command]
pub fn delete_profile(core: State<CoreRef>, id: String) -> R<()> {
    core.profiles.lock().remove(&id, &core.vault)?;
    if core.settings().active_profile_id.as_deref() == Some(&id) {
        core.update_settings(SettingsPatch {
            active_profile_id: Some(None),
            polish_enabled: Some(false),
            ..Default::default()
        })?;
    }
    emit_provider(&core);
    Ok(())
}

#[tauri::command]
pub fn set_active_profile(core: State<CoreRef>, id: Option<String>) -> R<Settings> {
    if let Some(id) = &id {
        profile(&core, id)?;
    }
    let mut patch = SettingsPatch { active_profile_id: Some(id.clone()), ..Default::default() };
    if id.is_none() {
        patch.polish_enabled = Some(false);
    }
    core.update_settings(patch)
}

#[tauri::command]
pub fn credential_status(core: State<CoreRef>, profile_id: String) -> R<CredentialStatus> {
    let p = profile(&core, &profile_id)?;
    Ok(core.profiles.lock().status(&p, &core.vault))
}

#[tauri::command]
pub fn save_credential(core: State<CoreRef>, profile_id: String, name: Option<String>, value: String) -> R<CredentialStatus> {
    let p = profile(&core, &profile_id)?;
    let value = value.trim();
    if value.is_empty() {
        return Err("Enter a value to save.".into());
    }
    let account = match &name {
        None => bearer_account(&p.id),
        Some(h) if p.secret_header_names.iter().any(|x| x.eq_ignore_ascii_case(h)) => header_account(&p.id, h),
        Some(_) => return Err("Add the header name to this provider first.".into()),
    };
    core.vault.set(&account, value)?;
    // Saving a key is the deliberate step that binds it to this endpoint's origin.
    core.profiles.lock().bind_origin(&p)?;
    Ok(core.profiles.lock().status(&p, &core.vault))
}

#[tauri::command]
pub fn delete_credential(core: State<CoreRef>, profile_id: String, name: Option<String>) -> R<CredentialStatus> {
    let p = profile(&core, &profile_id)?;
    let account = match &name {
        None => bearer_account(&p.id),
        Some(h) => header_account(&p.id, h),
    };
    core.vault.delete(&account)?;
    Ok(core.profiles.lock().status(&p, &core.vault))
}

#[tauri::command]
pub async fn refresh_provider_models(core: State<'_, CoreRef>, profile_id: String) -> R<Vec<ModelSummary>> {
    let p = profile(&core, &profile_id)?;
    let store = core.profiles.lock().clone();
    crate::providers::list_models(&core.http, &store, &core.vault, &p)
        .await
        .map_err(|e| crate::providers::describe(&e))
}

#[tauri::command]
pub async fn test_connection(core: State<'_, CoreRef>, profile_id: String) -> R<ConnectionResult> {
    let p = profile(&core, &profile_id)?;
    let store = core.profiles.lock().clone();
    Ok(crate::providers::test_connection(&core.http, &store, &core.vault, &p).await)
}

// ---------- windows & misc ----------

#[tauri::command]
pub fn open_main(app: AppHandle, page: Page) {
    shell::show_main(&app, page);
}

#[tauri::command]
pub fn reset_bar_position(app: AppHandle) {
    shell::reset_bar_position(&app);
}

#[tauri::command]
pub fn show_bar_menu(app: AppHandle) -> R<()> {
    shell::show_bar_menu(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_bar_hit_region(app: AppHandle, x: f64, y: f64, width: f64, height: f64) {
    shell::set_bar_hit_region(&app, x, y, width, height);
}

#[tauri::command]
pub fn reveal_data_dir(app: AppHandle, core: State<CoreRef>) -> R<()> {
    app.opener().open_path(core.paths.root.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> R<()> {
    let parsed = url::Url::parse(&url).map_err(|_| "Invalid link.".to_string())?;
    let allowed = parsed.scheme() == "https"
        || (parsed.scheme() == "x-apple.systempreferences")
        || parsed.scheme() == "ms-settings";
    if !allowed {
        return Err("Only secure links can be opened.".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}
