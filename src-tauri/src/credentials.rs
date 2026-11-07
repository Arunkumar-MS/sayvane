//! OS credential vault access (macOS Keychain, Windows Credential Manager).
//! Secrets never enter SQLite, settings files, logs, or IPC responses.

use std::collections::HashMap;
use std::sync::Mutex;

pub trait SecretStore: Send + Sync {
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

pub fn bearer_account(profile_id: &str) -> String {
    format!("{profile_id}:bearer")
}

pub fn header_account(profile_id: &str, header: &str) -> String {
    format!("{profile_id}:header:{}", header.to_ascii_lowercase())
}

pub struct OsVault {
    service: String,
}

impl OsVault {
    pub fn new() -> Self {
        Self { service: crate::brand::credential_service() }
    }
}

impl Default for OsVault {
    fn default() -> Self {
        Self::new()
    }
}

fn vault_err(e: keyring::Error) -> String {
    format!("The system credential store reported an error: {e}")
}

impl SecretStore for OsVault {
    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        keyring::Entry::new(&self.service, account).map_err(vault_err)?.set_password(value).map_err(vault_err)
    }

    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match keyring::Entry::new(&self.service, account).map_err(vault_err)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(vault_err(e)),
        }
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match keyring::Entry::new(&self.service, account).map_err(vault_err)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(vault_err(e)),
        }
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryVault(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryVault {
    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        self.0.lock().unwrap().insert(account.into(), value.into());
        Ok(())
    }
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().get(account).cloned())
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(account);
        Ok(())
    }
}
