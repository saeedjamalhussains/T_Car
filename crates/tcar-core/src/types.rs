use serde::{Deserialize, Serialize};

/// A single carrier lookup result.
///
/// This is the modern shape of the map the original `node-tel-carrier`
/// returned from `telCarrier.lookup()`; field names are kept recognizable
/// (`carrier`, `wireless`, `smsGateway`, `mmsGateway`) so old consumers can
/// port over easily.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Lookup {
    /// E.164, e.g. `+18013604427`.
    pub number: String,
    /// National/display form, e.g. `(801) 360-4427` or `+91 9876543210`.
    pub national: String,
    /// Country calling code without `+`, e.g. `1`, `91`.
    pub calling_code: String,
    /// ISO 3166-1 alpha-2, when the calling code identifies one country.
    pub country_iso: Option<String>,
    pub country_name: Option<String>,
    /// `true` cellular, `false` landline, `None` unknown.
    pub wireless: Option<bool>,
    /// Normalized carrier key (`att`, `verizon`, ...) when we can match one.
    pub carrier: Option<String>,
    /// Human carrier string exactly as the source reported it.
    pub carrier_comment: Option<String>,
    /// Full SMS gateway address, e.g. `8013604427@txt.att.net`.
    pub sms_gateway: Option<String>,
    /// Full MMS gateway address.
    pub mms_gateway: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub state_code: Option<String>,
    /// Line type as reported by the source ("CELLULAR", "LANDLINE", ...).
    pub line_type: Option<String>,
    /// OCN / registered operating company, when known.
    pub company: Option<String>,
    /// Which provider produced this result.
    pub source: String,
    /// `false` when the record is inferred from a prefix block rather than
    /// from a per-number authoritative lookup (ports are invisible to it).
    pub authoritative: bool,
    /// Non-fatal notes (e.g. "number may have been ported").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// Result of one number inside a bulk run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkItem {
    pub input: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lookup: Option<Lookup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
