//! Finding phone numbers inside arbitrary text.

use crate::number::Number;

/// Pull every plausible phone number out of arbitrary pasted text or CSV.
///
/// Matches international (`+<code> ...`) and bare North American forms; each
/// candidate is confirmed by the parser, so near-misses are dropped.
pub fn extract_numbers(text: &str) -> Vec<String> {
    let re = regex::Regex::new(
        r"(?x)
          \+ \d[\d\s.\-()]{5,20} \d      # international, e.g. +91 98765 43210
        | \b \(?\d{3}\)? [\s.\-]* \d{3} [\s.\-]* \d{4} \b   # bare NANP
        ",
    )
    .unwrap();
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for m in re.find_iter(text) {
        let raw = m.as_str().trim().to_string();
        if let Ok(n) = Number::parse(&raw) {
            if seen.insert(n.e164()) {
                out.push(raw);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_numbers_from_prose() {
        let found = extract_numbers("call 801-360-4427 or (212) 555-1234, dupe 8013604427");
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn extracts_international_numbers() {
        let found = extract_numbers("India +91 98765 43210 and UK +44 20 7946 0018");
        assert_eq!(found.len(), 2);
    }
}
