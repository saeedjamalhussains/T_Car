//! Lookup providers.
//!
//! Two sources of truth, and nothing else:
//!
//! * [`Offline`] -- the embedded North American prefix table. This is what
//!   actually answers in this demo build.
//! * [`legacy`] -- faithful ports of the four services `node-tel-carrier`
//!   shipped in 2014, kept exactly as they were as study material. Their hosts
//!   are dead, refusing bots, or paywalled today, so they are documentation
//!   that happens to compile rather than a way to get data.
//!
//! Commercial lookup APIs are deliberately not integrated: this is an
//! educational build, not a service.

pub mod legacy;

use crate::number::Number;
use crate::types::Lookup;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// What a provider needs before it can be called.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Credentials {
    /// No credentials at all (offline db, scrapers).
    None,
    /// A single API key / token.
    ApiKey,
    /// Username + password (or SID + token).
    UserPass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub label: String,
    pub credentials: Credentials,
    /// True for the original 2014 services.
    pub legacy: bool,
    /// True when results are per-number (ports included) rather than
    /// inferred from a prefix block.
    pub authoritative: bool,
    pub docs: Option<String>,
    /// What we know about whether this source still answers. Shown in the UI
    /// so nobody picks a provider that cannot work.
    pub health: Health,
    /// One line of context for the health state.
    pub note: Option<String>,
}

/// Whether a provider is expected to return data today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Health {
    /// Works.
    Ok,
    /// Reachable, but you need an account or credentials.
    NeedsAccount,
    /// The host refuses automated requests (bot protection).
    Blocked,
    /// The host no longer exists.
    Offline,
}

/// Per-provider credentials, keyed by provider id.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub pass: Option<String>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn info(&self) -> ProviderInfo;
    async fn lookup(
        &self,
        http: &reqwest::Client,
        number: &Number,
        cred: &Credential,
    ) -> Result<Lookup>;
    /// Providers that support batching override this.
    async fn lookup_many(
        &self,
        http: &reqwest::Client,
        numbers: &[Number],
        cred: &Credential,
    ) -> Vec<Result<Lookup>> {
        let mut out = Vec::with_capacity(numbers.len());
        for n in numbers {
            out.push(self.lookup(http, n, cred).await);
        }
        out
    }
}

/// The offline embedded database, exposed as a provider so the UI can treat
/// every source the same way.
pub struct Offline;

#[async_trait]
impl Provider for Offline {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "offline".into(),
            label: "Offline database (embedded)".into(),
            credentials: Credentials::None,
            legacy: false,
            authoritative: false,
            docs: None,
            health: Health::Ok,
            note: Some("Embedded North American prefix table; no network needed.".into()),
        }
    }

    async fn lookup(
        &self,
        _http: &reqwest::Client,
        number: &Number,
        _cred: &Credential,
    ) -> Result<Lookup> {
        crate::offline::lookup(number).ok_or_else(|| {
            if number.is_nanp() {
                anyhow::anyhow!("{} is not in the offline prefix database", number.national())
            } else {
                anyhow::anyhow!(
                    "the offline database covers North America only; configure an online provider for +{} numbers",
                    number.country.calling_code
                )
            }
        })
    }
}

/// Every provider this build knows about, in default fallback order.
pub fn all() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(Offline),
        Box::new(legacy::Data247),
        Box::new(legacy::FoneFinder),
        Box::new(legacy::XMinder),
        Box::new(legacy::FreeCarrierLookup),
    ]
}

pub fn get(id: &str) -> Option<Box<dyn Provider>> {
    all().into_iter().find(|p| p.info().id == id)
}

/// Shared helper: fill in gateway addresses from a carrier string.
pub fn enrich(lookup: &mut Lookup, number: &Number) {
    let carrier = lookup
        .sms_gateway
        .as_deref()
        .and_then(crate::gateways::by_gateway)
        .or_else(|| lookup.carrier_comment.as_deref().and_then(crate::gateways::by_comment));
    let Some(c) = carrier else { return };

    lookup.carrier.get_or_insert_with(|| c.name.clone());
    // Email-to-SMS gateways in this registry are North American only, so we
    // synthesize addresses just for NANP numbers.
    if let Some(ten) = number.ten() {
        if lookup.sms_gateway.is_none() {
            lookup.sms_gateway = c.sms.as_deref().map(|d| crate::gateways::address(&ten, d));
        }
        if lookup.mms_gateway.is_none() {
            lookup.mms_gateway = c.mms.as_deref().map(|d| crate::gateways::address(&ten, d));
        }
    }
    if lookup.wireless.is_none() {
        lookup.wireless = c.wireless;
    }
}
