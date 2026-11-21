//! Offline NANP prefix database (area code + exchange -> telco record).
//!
//! Generated from `tel-carrier-db@1.1.3`; ~1 MB gzipped, embedded in the
//! binary and decompressed lazily on first lookup. Because it is keyed by
//! prefix block, results are NOT authoritative for ported numbers -- which is
//! exactly the caveat the original encoded as `opts.authoritative === false`.

use crate::number::Number;
use crate::types::Lookup;
use flate2::read::GzDecoder;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;

#[derive(Debug, Deserialize)]
struct Meta {
    cities: Vec<String>,
    states: Vec<String>,
    sts: Vec<String>,
    companies: Vec<String>,
    types: Vec<String>,
    #[allow(dead_code)]
    links: Vec<String>,
    carriers: Vec<String>,
    #[serde(rename = "carrierNames")]
    carrier_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Payload {
    #[allow(dead_code)]
    version: u32,
    pub source: String,
    meta: Meta,
    /// area code -> rows of
    /// [prefix, city, state, st, company, type, link, carrier, carrierName]
    list: HashMap<String, Vec<Vec<i64>>>,
}

static NANP_PREFIXES_GZ: &[u8] = include_bytes!("../../../datasets/nanp-prefixes.json.gz");

static DB: Lazy<Payload> = Lazy::new(|| {
    let mut json = String::new();
    GzDecoder::new(NANP_PREFIXES_GZ)
        .read_to_string(&mut json)
        .expect("embedded prefix db decompresses");
    serde_json::from_str(&json).expect("embedded prefix db parses")
});

fn get(table: &[String], idx: i64) -> Option<String> {
    let s = table.get(usize::try_from(idx).ok()?)?;
    if s.trim().is_empty() {
        None
    } else {
        Some(s.clone())
    }
}

/// Human description of where the offline data came from.
pub fn source_description() -> &'static str {
    &DB.source
}

/// Number of area codes covered.
pub fn area_code_count() -> usize {
    DB.list.len()
}

// ---------------------------------------------------------------------------
// Browsing the dataset
//
// The demo app exposes the embedded tables directly so you can see what a
// prefix-block database actually contains -- and why a lookup built on one is
// only ever a guess for a ported number.
// ---------------------------------------------------------------------------

/// One area code, summarized.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaSummary {
    pub area: String,
    /// How many exchange (NXX) blocks are recorded under it.
    pub prefixes: usize,
    /// The state that covers most of those blocks, if any.
    pub state: Option<String>,
    pub state_code: Option<String>,
    /// The most common city in the area code.
    pub city: Option<String>,
    /// How many of its blocks look like wireless.
    pub wireless_blocks: usize,
}

/// One exchange block: the smallest unit the database knows about.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefixRow {
    pub area: String,
    pub prefix: String,
    pub city: Option<String>,
    pub state_code: Option<String>,
    pub company: Option<String>,
    pub line_type: Option<String>,
    pub carrier: Option<String>,
    pub wireless: Option<bool>,
}

fn most_common(values: impl Iterator<Item = Option<String>>) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for v in values.flatten() {
        *counts.entry(v).or_default() += 1;
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(v, _)| v)
}

fn looks_wireless(line_type: &Option<String>, company: &Option<String>) -> Option<bool> {
    let hit = |s: &Option<String>| {
        s.as_deref()
            .map(|v| {
                let v = v.to_lowercase();
                v.contains("wireless") || v.contains("pcs") || v.contains("cellular")
            })
            .unwrap_or(false)
    };
    if hit(line_type) || hit(company) {
        return Some(true);
    }
    let landline = line_type
        .as_deref()
        .map(|t| {
            let t = t.to_lowercase();
            t.contains("landline") || t.contains("clec") || t.contains("ilec") || t.contains("rboc")
        })
        .unwrap_or(false);
    landline.then_some(false)
}

fn row_at(area: &str, row: &[i64]) -> PrefixRow {
    let m = &DB.meta;
    let company = get(&m.companies, row[4]);
    let line_type = get(&m.types, row[5]);
    PrefixRow {
        area: area.to_string(),
        prefix: format!("{:03}", row[0]),
        city: get(&m.cities, row[1]),
        state_code: get(&m.sts, row[3]),
        carrier: get(&m.carrier_names, row[8]).or_else(|| get(&m.carriers, row[7])),
        wireless: looks_wireless(&line_type, &company),
        company,
        line_type,
    }
}

/// Every area code in the database, summarized, sorted numerically.
pub fn area_codes() -> Vec<AreaSummary> {
    let mut out: Vec<AreaSummary> = DB
        .list
        .iter()
        .map(|(area, rows)| {
            let parsed: Vec<PrefixRow> = rows.iter().map(|r| row_at(area, r)).collect();
            AreaSummary {
                area: area.clone(),
                prefixes: parsed.len(),
                state: most_common(parsed.iter().map(|r| {
                    r.state_code
                        .as_deref()
                        .and_then(|sc| DB.meta.states.iter().find(|s| !s.is_empty() && s.starts_with(sc)))
                        .cloned()
                })),
                state_code: most_common(parsed.iter().map(|r| r.state_code.clone())),
                city: most_common(parsed.iter().map(|r| r.city.clone())),
                wireless_blocks: parsed.iter().filter(|r| r.wireless == Some(true)).count(),
            }
        })
        .collect();
    out.sort_by(|a, b| a.area.cmp(&b.area));
    out
}

/// Every exchange block recorded under one area code.
pub fn prefixes(area: &str) -> Option<Vec<PrefixRow>> {
    let rows = DB.list.get(area)?;
    let mut out: Vec<PrefixRow> = rows.iter().map(|r| row_at(area, r)).collect();
    out.sort_by(|a, b| a.prefix.cmp(&b.prefix));
    Some(out)
}

/// Totals for the About / Data tabs.
pub fn stats() -> (usize, usize) {
    let areas = DB.list.len();
    let blocks = DB.list.values().map(|v| v.len()).sum();
    (areas, blocks)
}

/// Look a number up against the embedded prefix database.
pub fn lookup(n: &Number) -> Option<Lookup> {
    // The embedded database is a North American prefix table; every other
    // country has to come from a live provider.
    let rows = DB.list.get(n.area()?)?;
    let prefix: i64 = n.prefix()?.parse().ok()?;
    let row = rows.iter().find(|r| r.first() == Some(&prefix))?;

    let m = &DB.meta;
    let company = get(&m.companies, row[4]);
    let line_type = get(&m.types, row[5]);
    let carrier_name = get(&m.carrier_names, row[8]).or_else(|| get(&m.carriers, row[7]));

    let wireless_hint = |s: &Option<String>| {
        s.as_deref()
            .map(|v| {
                let v = v.to_lowercase();
                v.contains("wireless") || v.contains("pcs") || v.contains("cellular")
            })
            .unwrap_or(false)
    };
    let wireless = if wireless_hint(&line_type) || wireless_hint(&company) {
        Some(true)
    } else if line_type
        .as_deref()
        .map(|t| t.to_lowercase().contains("landline") || t.to_lowercase().contains("clec") || t.to_lowercase().contains("ilec"))
        .unwrap_or(false)
    {
        Some(false)
    } else {
        None
    };

    let mut out = Lookup {
        number: n.e164(),
        national: n.national(),
        calling_code: n.country.calling_code.clone(),
        country_iso: n.country.iso.clone(),
        country_name: n.country.name.clone(),
        wireless,
        carrier_comment: carrier_name.clone(),
        city: get(&m.cities, row[1]),
        state: get(&m.states, row[2]),
        state_code: get(&m.sts, row[3]),
        line_type,
        company,
        source: "offline-db".into(),
        authoritative: false,
        notes: vec!["Prefix-block data: a ported number may be on another carrier.".into()],
        ..Default::default()
    };

    // Attach gateways when we recognize the carrier.
    let matched = carrier_name
        .as_deref()
        .and_then(crate::gateways::by_comment)
        .or_else(|| out.company.as_deref().and_then(crate::gateways::by_comment));
    if let Some(c) = matched {
        out.carrier = Some(c.name.clone());
        let ten = n.ten()?;
        out.sms_gateway = c.sms.as_deref().map(|d| crate::gateways::address(&ten, d));
        out.mms_gateway = c.mms.as_deref().map(|d| crate::gateways::address(&ten, d));
        if out.wireless.is_none() {
            out.wireless = c.wireless;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_prefix_resolves() {
        let n = Number::parse("8013604427").unwrap();
        let l = lookup(&n).expect("801-360 is in the database");
        assert_eq!(l.number, "+18013604427");
        assert!(!l.authoritative);
    }

    #[test]
    fn dataset_browsing_returns_usable_rows() {
        let (areas, blocks) = stats();
        assert_eq!(areas, area_code_count());
        assert!(blocks > areas, "every area code holds at least one block");

        let summaries = area_codes();
        assert_eq!(summaries.len(), areas);
        let utah = summaries.iter().find(|a| a.area == "801").expect("801 is present");
        assert!(utah.prefixes > 0);
        assert_eq!(utah.state_code.as_deref(), Some("UT"));

        let rows = prefixes("801").expect("801 has rows");
        assert_eq!(rows.len(), utah.prefixes);
        assert!(rows.windows(2).all(|w| w[0].prefix <= w[1].prefix), "sorted by exchange");
        let row = rows.iter().find(|r| r.prefix == "360").expect("801-360 is present");
        assert_eq!(row.area, "801");
        assert_eq!(row.wireless, Some(true));

        assert!(prefixes("999").is_none());
    }

    #[test]
    fn non_nanp_numbers_are_not_in_the_offline_table() {
        let n = Number::parse("+91 98765 43210").unwrap();
        assert!(lookup(&n).is_none());
    }
}
