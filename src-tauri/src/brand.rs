//! Build-time product identity from branding/brand.json. Display text may change;
//! the application ID and storage namespace must stay stable after release.

use crate::ipc_types::Brand;
use std::sync::OnceLock;

const BRAND_JSON: &str = include_str!("../../branding/brand.json");

pub fn brand() -> &'static Brand {
    static B: OnceLock<Brand> = OnceLock::new();
    B.get_or_init(|| serde_json::from_str(BRAND_JSON).expect("branding/brand.json is valid"))
}

pub fn name() -> &'static str {
    &brand().display_name
}

/// Service name for OS credential vault entries. Keyed by the stable namespace so a
/// display-name change never strands saved keys.
pub fn credential_service() -> String {
    let b = brand();
    let ns = if b.storage_namespace.is_empty() { &b.application_id } else { &b.storage_namespace };
    format!("{ns}.providers")
}
