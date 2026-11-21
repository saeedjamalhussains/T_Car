//! Carrier registry: maps carrier names / gateway domains onto a normalized
//! carrier key plus its SMS and MMS email-to-text domains.
//!
//! Data is generated from `tel-carrier-gateways@2.0.0` by `tools/gen-data.js`.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Carrier {
    pub name: String,
    pub carrier: String,
    pub wireless: Option<bool>,
    pub sms: Option<String>,
    pub mms: Option<String>,
    /// Regex source for carriers whose printed name varies (AT&T / Cingular).
    pub test: Option<String>,
    pub historic: bool,
    pub updated: Option<i64>,
}

static CARRIER_GATEWAYS_JSON: &str = include_str!("../../../datasets/nanp-carrier-gateways.json");

pub static CARRIERS: Lazy<Vec<Carrier>> =
    Lazy::new(|| serde_json::from_str(CARRIER_GATEWAYS_JSON).expect("nanp-carrier-gateways.json is valid"));

fn matcher(c: &Carrier) -> Option<Regex> {
    let src = match &c.test {
        Some(t) => t.clone(),
        None => format!(r"\b{}\b", regex::escape(&c.carrier)),
    };
    Regex::new(&format!("(?i){src}")).ok()
}

/// Match a printed carrier string ("Verizon Wireless") to a known carrier.
pub fn by_comment(comment: &str) -> Option<&'static Carrier> {
    if comment.trim().is_empty() {
        return None;
    }
    // Prefer in-the-wild entries over historic ones.
    CARRIERS
        .iter()
        .filter(|c| !c.historic)
        .chain(CARRIERS.iter().filter(|c| c.historic))
        .find(|c| matcher(c).map(|re| re.is_match(comment)).unwrap_or(false))
}

/// Match a gateway address or bare domain (`8015551212@vtext.com`, `vtext.com`).
pub fn by_gateway(addr: &str) -> Option<&'static Carrier> {
    let domain = addr.rsplit('@').next()?.trim().to_lowercase();
    if domain.is_empty() {
        return None;
    }
    CARRIERS.iter().find(|c| {
        [c.sms.as_deref(), c.mms.as_deref()]
            .into_iter()
            .flatten()
            .any(|d| d.eq_ignore_ascii_case(&domain))
    })
}

pub fn by_key(key: &str) -> Option<&'static Carrier> {
    CARRIERS.iter().find(|c| c.name.eq_ignore_ascii_case(key))
}

/// Build `8013604427@txt.att.net` from a 10-digit number and a domain.
pub fn address(ten: &str, domain: &str) -> String {
    format!("{ten}@{}", domain.rsplit('@').next().unwrap_or(domain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_verizon_by_gateway_and_comment() {
        assert_eq!(by_gateway("8013604427@vtext.com").unwrap().name, "verizon");
        assert_eq!(by_comment("Verizon Wireless").unwrap().name, "verizon");
    }

    #[test]
    fn att_matches_cingular_alias() {
        assert_eq!(by_comment("Cingular Wireless").unwrap().name, "att");
    }
}
