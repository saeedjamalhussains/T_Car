//! Worldwide phone number parsing (E.164).
//!
//! Educational scope: this is a compact parser, not libphonenumber. It splits
//! an international number into country calling code + national number,
//! validates E.164 length limits, and keeps the extra NANP structure (area
//! code / exchange / line) that the offline database needs. It does not carry
//! per-country national-number-length rules or national trunk-prefix tables
//! beyond the common `0` prefix.

use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

/// `[calling code, ISO 3166-1 alpha-2, country name]`
type CodeRow = (String, String, String);

static CODES_JSON: &str = include_str!("../../../datasets/calling-codes.json");

static CODES: Lazy<Vec<CodeRow>> = Lazy::new(|| {
    let rows: Vec<Vec<String>> = serde_json::from_str(CODES_JSON).expect("calling-codes.json parses");
    let mut rows: Vec<CodeRow> = rows
        .into_iter()
        .map(|r| (r[0].clone(), r[1].clone(), r[2].clone()))
        .collect();
    // Longest calling code first so `+1` never shadows `+123`.
    rows.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.1.cmp(&b.1)));
    rows
});

/// Country a number was matched to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Country {
    pub calling_code: String,
    /// ISO 3166-1 alpha-2, e.g. `IN`. `None` when the calling code is shared
    /// by several countries and we cannot tell them apart (all of +1 outside
    /// the US/Canada, +7, +44 dependencies, ...).
    pub iso: Option<String>,
    pub name: Option<String>,
}

/// A parsed international number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Number {
    pub country: Country,
    /// Digits after the calling code.
    pub national_number: String,
}

fn digits(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn country_for(code: &str) -> Country {
    let matches: Vec<&CodeRow> = CODES.iter().filter(|r| r.0 == code).collect();
    match matches.as_slice() {
        [one] => Country {
            calling_code: code.into(),
            iso: Some(one.1.clone()),
            name: Some(one.2.clone()),
        },
        // Shared codes (+1 NANP, +7 RU/KZ): the calling code alone is not
        // enough to name the country.
        _ => Country { calling_code: code.into(), iso: None, name: None },
    }
}

impl Number {
    /// Parse a number in international form (`+91 98765 43210`, `0091...`,
    /// `+1 801-360-4427`).
    ///
    /// Bare national numbers with no country context are assumed to be NANP
    /// when they are 10 or 11 digits, matching the original library. Use
    /// [`Number::parse_with_default`] to assume a different country instead.
    pub fn parse(input: &str) -> Result<Number> {
        Number::parse_with_default(input, "1")
    }

    /// `default_cc` is the calling code assumed for numbers written without a
    /// `+` — e.g. `"91"` to read `98765 43210` as an Indian number.
    pub fn parse_with_default(input: &str, default_cc: &str) -> Result<Number> {
        let trimmed = input.trim();
        let international = trimmed.starts_with('+')
            || trimmed.starts_with("00")
            || trimmed.starts_with("011");

        let mut d = digits(trimmed);
        if d.is_empty() {
            bail!("'{input}' contains no digits");
        }
        if international {
            // Strip an international access prefix if one was used.
            if !trimmed.starts_with('+') {
                if let Some(rest) = d.strip_prefix("00") {
                    d = rest.to_string();
                } else if let Some(rest) = d.strip_prefix("011") {
                    d = rest.to_string();
                }
            }
            return Number::from_e164_digits(&d, input);
        }

        // No country given. NANP-shaped input keeps the original behavior.
        if default_cc == "1" && (d.len() == 10 || (d.len() == 11 && d.starts_with('1'))) {
            return Number::from_e164_digits(&format!("1{}", d.trim_start_matches('1')), input);
        }
        // Otherwise drop a national trunk `0` and prepend the default code.
        let national = d.strip_prefix('0').unwrap_or(&d);
        Number::from_e164_digits(&format!("{default_cc}{national}"), input)
    }

    fn from_e164_digits(d: &str, original: &str) -> Result<Number> {
        // E.164: at most 15 digits, and a country code plus something.
        if d.len() > 15 {
            bail!("'{original}' has {} digits; E.164 allows at most 15", d.len());
        }
        let code = CODES
            .iter()
            .map(|r| r.0.as_str())
            .find(|c| d.starts_with(c))
            .ok_or_else(|| anyhow::anyhow!("'{original}' does not start with a known country calling code"))?;

        let national = &d[code.len()..];
        if national.len() < 4 {
            bail!("'{original}' is too short to be a real subscriber number");
        }

        let n = Number {
            country: country_for(code),
            national_number: national.to_string(),
        };

        // NANP has structural rules worth enforcing, since the offline
        // database is keyed by them.
        if n.is_nanp() {
            if national.len() != 10 {
                bail!("'{original}' is not a 10-digit North American number");
            }
            let b = national.as_bytes();
            if !matches!(b[0], b'2'..=b'9') {
                bail!("'{original}' has an invalid area code ({})", &national[..3]);
            }
            if !matches!(b[3], b'2'..=b'9') {
                bail!("'{original}' has an invalid exchange code ({})", &national[3..6]);
            }
        }
        Ok(n)
    }

    /// True for +1 numbers (US, Canada, and the Caribbean NANP members).
    pub fn is_nanp(&self) -> bool {
        self.country.calling_code == "1" && self.national_number.len() == 10
    }

    /// `+918012345678`
    pub fn e164(&self) -> String {
        format!("+{}{}", self.country.calling_code, self.national_number)
    }

    /// Digits only, no `+`.
    pub fn digits(&self) -> String {
        format!("{}{}", self.country.calling_code, self.national_number)
    }

    /// The subscriber part, which is what SMS gateways address (NANP) and what
    /// most national-format displays show.
    pub fn national(&self) -> String {
        if self.is_nanp() {
            let n = &self.national_number;
            format!("({}) {}-{}", &n[..3], &n[3..6], &n[6..])
        } else {
            format!("+{} {}", self.country.calling_code, self.national_number)
        }
    }

    // ---- NANP-only accessors, used by the offline prefix database ----

    pub fn area(&self) -> Option<&str> {
        self.is_nanp().then(|| &self.national_number[..3])
    }
    pub fn prefix(&self) -> Option<&str> {
        self.is_nanp().then(|| &self.national_number[3..6])
    }
    pub fn line(&self) -> Option<&str> {
        self.is_nanp().then(|| &self.national_number[6..])
    }
    /// The 10-digit NANP number used to build gateway addresses.
    pub fn ten(&self) -> Option<String> {
        self.is_nanp().then(|| self.national_number.clone())
    }
}

/// Every calling code this build knows, for UI country pickers.
pub fn known_countries() -> Vec<Country> {
    CODES
        .iter()
        .map(|(code, iso, name)| Country {
            calling_code: code.clone(),
            iso: Some(iso.clone()),
            name: Some(name.clone()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nanp_formats() {
        for s in ["8013604427", "1-801-360-4427", "+1 (801) 360-4427", "801.360.4427"] {
            let n = Number::parse(s).unwrap();
            assert_eq!(n.e164(), "+18013604427");
            assert_eq!(n.national(), "(801) 360-4427");
            assert!(n.is_nanp());
        }
    }

    #[test]
    fn parses_international() {
        let india = Number::parse("+91 98765 43210").unwrap();
        assert_eq!(india.e164(), "+919876543210");
        assert_eq!(india.country.iso.as_deref(), Some("IN"));
        assert!(!india.is_nanp());

        let uk = Number::parse("0044 20 7946 0018").unwrap();
        assert_eq!(uk.e164(), "+442079460018");
        assert_eq!(uk.country.iso.as_deref(), Some("GB"));

        // Shared calling code: country is deliberately left unnamed.
        let ru = Number::parse("+7 495 123 4567").unwrap();
        assert_eq!(ru.country.calling_code, "7");
        assert_eq!(ru.country.iso, None);
    }

    #[test]
    fn default_country_applies_to_bare_numbers() {
        let n = Number::parse_with_default("098765 43210", "91").unwrap();
        assert_eq!(n.e164(), "+919876543210");
    }

    #[test]
    fn rejects_bad_input() {
        for s in ["123", "0801360442", "abc", "+9999999999999999999"] {
            assert!(Number::parse(s).is_err(), "{s} should not parse");
        }
    }
}
