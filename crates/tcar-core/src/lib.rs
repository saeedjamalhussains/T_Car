//! T-Car core -- a modern rewrite of AJ ONeal's `node-tel-carrier` (2014).
//!
//! What changed and why is documented in `docs/REVERSE-ENGINEERING.md`. In
//! short: same idea (phone number -> carrier + SMS/MMS email gateway), but
//! with real number validation, an embedded offline database, a provider
//! chain with fallback, batching, CSV tooling -- and no silent analytics
//! upload.

pub mod config;
pub mod export;
pub mod extract;
pub mod gateways;
pub mod offline;
pub mod number;
pub mod providers;
pub mod types;

pub use config::Config;
pub use number::Number;
pub use providers::{Credential, Provider, ProviderInfo};
pub use types::{BulkItem, Lookup};

use anyhow::{anyhow, Result};
use std::time::Duration;

/// The lookup engine: holds the HTTP client and the configured provider chain.
pub struct Engine {
    http: reqwest::Client,
    config: Config,
}

impl Engine {
    pub fn new(config: Config) -> Result<Engine> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs.max(1)))
            .cookie_store(true)
            .user_agent(concat!("tcar/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Engine { http, config })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn set_config(&mut self, config: Config) -> Result<()> {
        *self = Engine::new(config)?;
        Ok(())
    }

    /// Parse a number using the configured default country for bare input.
    pub fn parse(&self, input: &str) -> Result<Number> {
        Number::parse_with_default(input, &self.config.default_calling_code)
    }

    /// Every country calling code this build knows.
    pub fn countries() -> Vec<number::Country> {
        number::known_countries()
    }

    /// Every provider this build knows about.
    pub fn providers() -> Vec<ProviderInfo> {
        providers::all().iter().map(|p| p.info()).collect()
    }

    fn chain(&self) -> Vec<String> {
        if self.config.chain.is_empty() {
            vec!["offline".into()]
        } else {
            self.config.chain.clone()
        }
    }

    /// Look one number up, trying each provider in the chain until one
    /// answers. Errors from every provider are collected into one message.
    pub async fn lookup(&self, input: &str) -> Result<Lookup> {
        let n = self.parse(input)?;
        let mut errors = Vec::new();
        for id in self.chain() {
            let Some(p) = providers::get(&id) else {
                errors.push(format!("{id}: unknown provider"));
                continue;
            };
            match p.lookup(&self.http, &n, &self.config.credential(&id)).await {
                Ok(l) => return Ok(l),
                Err(e) => errors.push(format!("{id}: {e}")),
            }
        }
        Err(anyhow!("no provider could resolve {}\n  {}", n.national(), errors.join("\n  ")))
    }

    /// Look one number up with a specific provider, ignoring the chain.
    pub async fn lookup_with(&self, provider_id: &str, input: &str) -> Result<Lookup> {
        let n = self.parse(input)?;
        let p = providers::get(provider_id).ok_or_else(|| anyhow!("unknown provider '{provider_id}'"))?;
        p.lookup(&self.http, &n, &self.config.credential(provider_id)).await
    }

    /// Look many numbers up, `concurrency` at a time. Order is preserved and
    /// a failure never aborts the run.
    pub async fn lookup_many(&self, inputs: &[String]) -> Vec<BulkItem> {
        let limit = self.config.concurrency.clamp(1, 32);
        let mut results: Vec<Option<BulkItem>> = vec![None; inputs.len()];

        for chunk in (0..inputs.len()).collect::<Vec<_>>().chunks(limit) {
            let futures = chunk.iter().map(|&i| {
                let input = inputs[i].clone();
                async move {
                    let item = match self.lookup(&input).await {
                        Ok(l) => BulkItem { input, ok: true, lookup: Some(l), error: None },
                        Err(e) => BulkItem { input, ok: false, lookup: None, error: Some(e.to_string()) },
                    };
                    (i, item)
                }
            });
            for (i, item) in futures::future::join_all(futures).await {
                results[i] = Some(item);
            }
        }
        results.into_iter().flatten().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn offline_lookup_works_without_network() {
        let engine = Engine::new(Config::default()).unwrap();
        let l = engine.lookup("801-360-4427").await.expect("offline db resolves a Utah number");
        assert_eq!(l.number, "+18013604427");
        assert_eq!(l.source, "offline-db");
    }

    #[tokio::test]
    async fn international_numbers_parse_but_need_a_provider() {
        let engine = Engine::new(Config::default()).unwrap();
        let n = engine.parse("+91 98765 43210").unwrap();
        assert_eq!(n.e164(), "+919876543210");
        // Nothing but the North American offline table is configured by default.
        assert!(engine.lookup("+91 98765 43210").await.is_err());
    }

    #[tokio::test]
    async fn bulk_preserves_order_and_reports_failures() {
        let engine = Engine::new(Config::default()).unwrap();
        let items = engine
            .lookup_many(&["801-360-4427".into(), "nonsense".into()])
            .await;
        assert_eq!(items.len(), 2);
        assert!(items[0].ok);
        assert!(!items[1].ok && items[1].error.is_some());
    }
}
