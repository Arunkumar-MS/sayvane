//! Model catalog, verified downloads, offline import, and removal.
//! A model becomes active only after its SHA-256 matches the manifest.

use crate::ipc_types::{DownloadProgress, ModelInfo};
use parking_lot::Mutex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

const MANIFEST_JSON: &str = include_str!("../../../models/manifest.json");
const SPARE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestModel {
    pub id: String,
    pub label: String,
    pub description: String,
    pub file_name: String,
    pub url: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub multilingual: bool,
    pub can_translate: bool,
    #[serde(default)]
    pub recommended_min_memory_gb: f64,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub manifest_version: u32,
    pub engine_version: String,
    pub models: Vec<ManifestModel>,
}

pub fn manifest() -> &'static Manifest {
    static M: std::sync::OnceLock<Manifest> = std::sync::OnceLock::new();
    M.get_or_init(|| serde_json::from_str(MANIFEST_JSON).expect("models/manifest.json is valid"))
}

pub fn find(id: &str) -> Option<&'static ManifestModel> {
    manifest().models.iter().find(|m| m.id == id)
}

struct Download {
    cancel: Arc<AtomicBool>,
    progress: DownloadProgress,
}

pub struct ModelManager {
    dir: PathBuf,
    downloads: Mutex<HashMap<String, Download>>,
}

impl ModelManager {
    pub fn new(dir: PathBuf) -> Self {
        // Remove partial files from interrupted runs; they are never resumed blindly.
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.path().extension().is_some_and(|x| x == "partial") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        Self { dir, downloads: Mutex::new(HashMap::new()) }
    }

    fn final_path(&self, m: &ManifestModel) -> PathBuf {
        self.dir.join(&m.file_name)
    }

    fn marker_path(&self, m: &ManifestModel) -> PathBuf {
        self.dir.join(format!("{}.verified", m.file_name))
    }

    /// Installed means the file exists with the manifest size and a verification marker
    /// recording the manifest digest, so large models are not re-hashed on every launch.
    pub fn is_installed(&self, id: &str) -> bool {
        let Some(m) = find(id) else { return false };
        let size_ok = std::fs::metadata(self.final_path(m)).map(|md| md.len() == m.size_bytes).unwrap_or(false);
        let marker_ok = std::fs::read_to_string(self.marker_path(m)).map(|s| s.trim() == m.sha256).unwrap_or(false);
        size_ok && marker_ok
    }

    pub fn path_if_installed(&self, id: &str) -> Option<PathBuf> {
        self.is_installed(id).then(|| self.final_path(find(id).expect("checked")))
    }

    pub fn any_installed(&self) -> bool {
        manifest().models.iter().any(|m| self.is_installed(&m.id))
    }

    pub fn infos(&self) -> Vec<ModelInfo> {
        let dl = self.downloads.lock();
        manifest().models.iter().map(|m| self.info_locked(m, &dl)).collect()
    }

    pub fn info(&self, id: &str) -> Option<ModelInfo> {
        let dl = self.downloads.lock();
        find(id).map(|m| self.info_locked(m, &dl))
    }

    fn info_locked(&self, m: &ManifestModel, dl: &HashMap<String, Download>) -> ModelInfo {
        ModelInfo {
            id: m.id.clone(),
            label: m.label.clone(),
            description: m.description.clone(),
            size_bytes: m.size_bytes,
            multilingual: m.multilingual,
            can_translate: m.can_translate,
            installed: self.is_installed(&m.id),
            is_default: m.is_default,
            recommended_min_memory_gb: m.recommended_min_memory_gb,
            download: dl.get(&m.id).map(|d| d.progress.clone()),
        }
    }

    pub fn cancel(&self, id: &str) {
        if let Some(d) = self.downloads.lock().get(id) {
            d.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub fn clear_failed(&self, id: &str) {
        let mut dl = self.downloads.lock();
        if dl.get(id).is_some_and(|d| d.progress.state == "failed") {
            dl.remove(id);
        }
    }

    fn check_space(&self, needed: u64) -> Result<(), String> {
        match fs4::available_space(&self.dir) {
            Ok(free) if free < needed + SPARE_BYTES => Err(format!(
                "Not enough disk space: {} MB free, {} MB needed.",
                free / 1_000_000,
                (needed + SPARE_BYTES) / 1_000_000
            )),
            _ => Ok(()),
        }
    }

    /// Downloads, verifies, and atomically activates a model. `on_progress` is throttled.
    pub async fn download(
        &self,
        client: &reqwest::Client,
        id: &str,
        on_progress: &(dyn Fn(ModelInfo) + Send + Sync),
    ) -> Result<(), String> {
        let m = find(id).ok_or("Unknown model.")?;
        if self.is_installed(id) {
            return Ok(());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut dl = self.downloads.lock();
            if dl.get(id).is_some_and(|d| d.progress.state != "failed") {
                return Err("This model is already downloading.".into());
            }
            dl.insert(
                id.into(),
                Download {
                    cancel: cancel.clone(),
                    progress: DownloadProgress {
                        received_bytes: 0,
                        total_bytes: m.size_bytes,
                        state: "downloading".into(),
                        error: None,
                    },
                },
            );
        }
        let result = self.download_inner(client, m, &cancel, on_progress).await;
        match &result {
            Ok(()) => {
                self.downloads.lock().remove(id);
            }
            Err(e) => {
                let mut dl = self.downloads.lock();
                if cancel.load(Ordering::SeqCst) {
                    dl.remove(id);
                } else if let Some(d) = dl.get_mut(id) {
                    d.progress.state = "failed".into();
                    d.progress.error = Some(e.clone());
                }
            }
        }
        if let Some(info) = self.info(id) {
            on_progress(info);
        }
        result
    }

    async fn download_inner(
        &self,
        client: &reqwest::Client,
        m: &ManifestModel,
        cancel: &AtomicBool,
        on_progress: &(dyn Fn(ModelInfo) + Send + Sync),
    ) -> Result<(), String> {
        use futures_util::StreamExt;
        self.check_space(m.size_bytes)?;
        let partial = self.dir.join(format!("{}.partial", m.file_name));
        let resp = client
            .get(&m.url)
            .send()
            .await
            .map_err(|e| format!("Download failed: {}", e.without_url()))?;
        if !resp.status().is_success() {
            return Err(format!("Download failed: HTTP {}", resp.status().as_u16()));
        }
        let mut file = std::fs::File::create(&partial).map_err(|e| format!("Could not write the model: {e}"))?;
        let mut hasher = Sha256::new();
        let mut received: u64 = 0;
        let mut last_emit = Instant::now();
        let mut stream = resp.bytes_stream();
        let cleanup = |p: &Path| {
            let _ = std::fs::remove_file(p);
        };
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::SeqCst) {
                drop(file);
                cleanup(&partial);
                return Err("Download canceled.".into());
            }
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    drop(file);
                    cleanup(&partial);
                    return Err(format!("Download interrupted: {}", e.without_url()));
                }
            };
            received += chunk.len() as u64;
            if received > m.size_bytes {
                drop(file);
                cleanup(&partial);
                return Err("The download is larger than expected and was discarded.".into());
            }
            hasher.update(&chunk);
            if let Err(e) = file.write_all(&chunk) {
                drop(file);
                cleanup(&partial);
                return Err(format!("Could not write the model: {e}"));
            }
            if last_emit.elapsed().as_millis() >= 250 {
                last_emit = Instant::now();
                if let Some(d) = self.downloads.lock().get_mut(&m.id) {
                    d.progress.received_bytes = received;
                }
                if let Some(info) = self.info(&m.id) {
                    on_progress(info);
                }
            }
        }
        if let Some(d) = self.downloads.lock().get_mut(&m.id) {
            d.progress.received_bytes = received;
            d.progress.state = "verifying".into();
        }
        if let Some(info) = self.info(&m.id) {
            on_progress(info);
        }
        file.sync_all().map_err(|e| format!("Could not write the model: {e}"))?;
        drop(file);
        let digest = hex(&hasher.finalize());
        if received != m.size_bytes || digest != m.sha256 {
            cleanup(&partial);
            return Err("The downloaded model failed verification and was discarded.".into());
        }
        self.activate(m, &partial)
    }

    fn activate(&self, m: &ManifestModel, verified_file: &Path) -> Result<(), String> {
        let dest = self.final_path(m);
        std::fs::rename(verified_file, &dest).map_err(|e| format!("Could not install the model: {e}"))?;
        crate::paths::atomic_write(&self.marker_path(m), m.sha256.as_bytes())
            .map_err(|e| format!("Could not install the model: {e}"))
    }

    /// Imports an existing model file for offline installation after verifying it.
    pub fn import_file(&self, id: &str, src: &Path) -> Result<(), String> {
        let m = find(id).ok_or("Unknown model.")?;
        let len = std::fs::metadata(src).map_err(|e| format!("The file could not be read: {e}"))?.len();
        if len != m.size_bytes {
            return Err(format!("This file is not the {} model (size does not match).", m.label));
        }
        self.check_space(len)?;
        let partial = self.dir.join(format!("{}.partial", m.file_name));
        let mut input = std::fs::File::open(src).map_err(|e| format!("The file could not be read: {e}"))?;
        let mut out = std::fs::File::create(&partial).map_err(|e| format!("Could not write the model: {e}"))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = input.read(&mut buf).map_err(|e| format!("The file could not be read: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(|e| format!("Could not write the model: {e}"))?;
        }
        out.sync_all().map_err(|e| e.to_string())?;
        drop(out);
        if hex(&hasher.finalize()) != m.sha256 {
            let _ = std::fs::remove_file(&partial);
            return Err(format!("This file does not match the verified {} model.", m.label));
        }
        self.activate(m, &partial)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let m = find(id).ok_or("Unknown model.")?;
        let _ = std::fs::remove_file(self.marker_path(m));
        match std::fs::remove_file(self.final_path(m)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("The model could not be removed: {e}")),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_has_real_digests_and_one_default() {
        let m = manifest();
        assert_eq!(m.manifest_version, 1);
        assert_eq!(m.models.iter().filter(|x| x.is_default).count(), 1);
        assert_eq!(m.models.iter().find(|x| x.is_default).unwrap().id, "base");
        for x in &m.models {
            assert_eq!(x.sha256.len(), 64, "{}", x.id);
            assert!(x.url.starts_with("https://"));
            assert!(x.size_bytes > 10_000_000);
        }
        assert!(!find("tiny.en").unwrap().can_translate);
    }

    #[test]
    fn import_rejects_wrong_file_and_keeps_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mm = ModelManager::new(dir.path().to_path_buf());
        let bogus = dir.path().join("bogus.bin");
        std::fs::write(&bogus, b"nope").unwrap();
        assert!(mm.import_file("base", &bogus).is_err());
        assert!(!mm.is_installed("base"));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    /// Network check that the real model URL resolves through its CDN redirect.
    /// Run with `cargo test -- --ignored model_url_follows_redirect`.
    #[test]
    #[ignore]
    fn model_url_follows_redirect() {
        let m = find("tiny").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let no_redirect = crate::providers::http_client().get(&m.url).send().await.unwrap();
            assert!(no_redirect.status().is_redirection(), "host redirects ({})", no_redirect.status());
            let r = crate::providers::download_client()
                .get(&m.url)
                .header("Range", "bytes=0-3")
                .send()
                .await
                .unwrap();
            assert!(r.status().is_success(), "status {}", r.status());
            let body = r.bytes().await.unwrap();
            assert_eq!(body.len(), 4);
        });
    }

    #[test]
    fn a_file_without_marker_is_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let mm = ModelManager::new(dir.path().to_path_buf());
        let m = find("tiny").unwrap();
        let f = std::fs::File::create(dir.path().join(&m.file_name)).unwrap();
        f.set_len(m.size_bytes).unwrap();
        assert!(!mm.is_installed("tiny"));
        std::fs::write(dir.path().join(format!("{}.verified", m.file_name)), "0".repeat(64)).unwrap();
        assert!(!mm.is_installed("tiny"), "marker must match the manifest digest");
    }
}
