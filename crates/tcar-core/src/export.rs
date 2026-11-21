//! Turning a lookup into something you can use elsewhere: CSV rows, a
//! `mailto:` that texts through the carrier gateway, a `tel:` dial link.

use crate::number::Number;
use crate::types::{BulkItem, Lookup};

/// Build a `mailto:` URL that sends a text through the carrier's SMS (or MMS)
/// email gateway, so a lookup turns straight into a usable action.
pub fn mailto(lookup: &Lookup, body: &str, prefer_mms: bool) -> Option<String> {
    let addr = if prefer_mms {
        lookup.mms_gateway.as_ref().or(lookup.sms_gateway.as_ref())
    } else {
        lookup.sms_gateway.as_ref().or(lookup.mms_gateway.as_ref())
    }?;
    Some(format!("mailto:{addr}?body={}", urlencoding::encode(body)))
}

/// A `tel:` link for the parsed number.
pub fn tel_link(n: &Number) -> String {
    format!("tel:{}", n.e164())
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub const CSV_HEADER: &str = "input,e164,national,callingCode,country,carrier,carrierName,wireless,\
lineType,smsGateway,mmsGateway,city,state,source,authoritative,error";

/// Render bulk results as CSV (header included).
pub fn to_csv(items: &[BulkItem]) -> String {
    let mut out = String::from(CSV_HEADER);
    out.push('\n');
    for it in items {
        let l = it.lookup.clone().unwrap_or_default();
        let cols = [
            it.input.clone(),
            l.number,
            l.national,
            l.calling_code,
            l.country_name.clone().or(l.country_iso.clone()).unwrap_or_default(),
            l.carrier.unwrap_or_default(),
            l.carrier_comment.unwrap_or_default(),
            l.wireless.map(|w| w.to_string()).unwrap_or_default(),
            l.line_type.unwrap_or_default(),
            l.sms_gateway.unwrap_or_default(),
            l.mms_gateway.unwrap_or_default(),
            l.city.unwrap_or_default(),
            l.state_code.unwrap_or_default(),
            l.source,
            l.authoritative.to_string(),
            it.error.clone().unwrap_or_default(),
        ];
        out.push_str(&cols.iter().map(|c| csv_escape(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_has_one_row_per_item_and_quotes_commas() {
        let csv = to_csv(&[BulkItem {
            input: "801-360-4427".into(),
            ok: false,
            lookup: None,
            error: Some("nope, failed".into()),
        }]);
        assert_eq!(csv.lines().count(), 2);
        assert!(csv.contains("\"nope, failed\""));
    }
}
