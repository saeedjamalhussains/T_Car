//! On-disk configuration: which providers to try, in what order, and the
//! credentials for each. Stored as JSON next to the app's data directory.

use crate::providers::Credential;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Providers tried in order until one succeeds.
    pub chain: Vec<String>,
    /// provider id -> credentials
    pub credentials: BTreeMap<String, Credential>,
    /// Seconds before an HTTP provider call is abandoned.
    pub timeout_secs: u64,
    /// Concurrent requests during a bulk run.
    pub concurrency: usize,
    /// Calling code assumed for numbers typed without a `+`, e.g. "1", "91".
    pub default_calling_code: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            chain: vec!["offline".into()],
            credentials: BTreeMap::new(),
            timeout_secs: 15,
            concurrency: 4,
            default_calling_code: "1".into(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        if let Ok(p) = std::env::var("TCAR_CONFIG") {
            return PathBuf::from(p);
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Path::new(&home).join(".config/tcar/config.json")
    }

    pub fn load() -> Config {
        Self::load_from(&Self::path()).unwrap_or_default()
    }

    pub fn load_from(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn credential(&self, id: &str) -> Credential {
        self.credentials.get(id).cloned().unwrap_or_default()
    }
}
