//! Managed storage layout under the platform application-support directory.
//! The directory is keyed by the stable application identifier, never the display name.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub recordings: PathBuf,
    pub capture: PathBuf,
    pub work: PathBuf,
    pub db: PathBuf,
    pub settings: PathBuf,
    pub models: PathBuf,
    pub logs: PathBuf,
}

impl Paths {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        let p = Self {
            recordings: root.join("recordings"),
            capture: root.join("recordings").join(".capture"),
            work: root.join("work"),
            db: root.join("db"),
            settings: root.join("settings"),
            models: root.join("models"),
            logs: root.join("logs"),
            root,
        };
        for dir in [&p.root, &p.recordings, &p.capture, &p.work, &p.db, &p.settings, &p.models, &p.logs] {
            fs::create_dir_all(dir)?;
            restrict_to_user(dir);
        }
        Ok(p)
    }

    pub fn database_file(&self) -> PathBuf {
        self.db.join("history.sqlite3")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.settings.join("settings.json")
    }

    pub fn profiles_file(&self) -> PathBuf {
        self.settings.join("providers.json")
    }

    pub fn overlay_file(&self) -> PathBuf {
        self.settings.join("overlay.json")
    }

    /// Resolves a database-stored relative recording path, refusing anything that
    /// escapes the managed recordings directory.
    pub fn resolve_recording(&self, relative: &str) -> Option<PathBuf> {
        let rel = Path::new(relative);
        if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            return None;
        }
        Some(self.recordings.join(rel))
    }
}

#[cfg(unix)]
pub fn restrict_to_user(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = if path.is_dir() { 0o700 } else { 0o600 };
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
pub fn restrict_to_user(_path: &Path) {
    // Windows: %APPDATA% is already ACL-restricted to the user profile.
}

/// Writes through a sibling temp file, fsyncs, then renames over the destination.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other("no parent directory"))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        uuid::Uuid::new_v4()
    ));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    restrict_to_user(&tmp);
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Makes a user-supplied export or import name safe for any filesystem while keeping Unicode.
pub fn sanitize_file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    let mut out: String = trimmed.chars().take(80).collect();
    if out.is_empty() {
        out = "transcript".into();
    }
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1",
        "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&out.to_uppercase().as_str()) {
        out.push('_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_paths_cannot_escape() {
        let p = Paths::new(tempfile::tempdir().unwrap().keep()).unwrap();
        assert!(p.resolve_recording("abc.wav").is_some());
        assert!(p.resolve_recording("../db/history.sqlite3").is_none());
        assert!(p.resolve_recording("/etc/passwd").is_none());
    }

    #[test]
    fn sanitizes_names_but_keeps_unicode() {
        assert_eq!(sanitize_file_stem("Réunion: plan/v2?"), "Réunion- plan-v2-");
        assert_eq!(sanitize_file_stem("..."), "transcript");
        assert_eq!(sanitize_file_stem("con"), "con_");
        assert_eq!(sanitize_file_stem("会議メモ"), "会議メモ");
    }

    #[test]
    fn atomic_write_replaces_content() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.json");
        atomic_write(&f, b"one").unwrap();
        atomic_write(&f, b"two").unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"two");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
