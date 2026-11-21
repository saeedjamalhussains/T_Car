//! Ports of the four services shipped by node-tel-carrier in 2014.
//!
//! Reverse-engineered request shapes and response parsing are preserved; see
//! docs/REVERSE-ENGINEERING.md for the wire formats. The original hard-coded
//! North America into all four (`cc=1`, `p1=1<ten digits>`, NPA/NXX query
//! parameters). Here the country calling code is threaded through instead, so
//! any service that can answer for a country will be asked properly:
//!
//! | service | reach |
//! | --- | --- |
//! | freecarrierlookup.com | worldwide -- its form takes a country code |
//! | data24-7.com | worldwide -- send full international digits |
//! | xminder.com | worldwide digits, though it was a NANP-era service |
//! | fonefinder.net | +1 only -- its query is literally NPA/NXX |
//!
//! Expect most of these to fail today regardless: the endpoints are dead or
//! bot-protected. They are kept so a revived endpoint keeps working, and
//! because they document how the 2014 package fetched its data.

use super::{Credential, Credentials, Health, Provider, ProviderInfo};
use crate::number::Number;
use crate::types::Lookup;
use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use scraper::{Html, Selector};

/// Turn a scraper's HTTP response into something a user can act on.
///
/// These sites are a decade past their prime: several answer with a bot-check
/// page or a plain refusal rather than data. Saying which is which beats
/// surfacing a bare status code -- and none of these states are meant to be
/// worked around.
async fn body_or_reason(res: reqwest::Response, service: &str) -> Result<String> {
    let status = res.status();
    if status.is_success() {
        return Ok(res.text().await?);
    }
    let cloudflare = res
        .headers()
        .get("server")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("cloudflare"))
        .unwrap_or(false);

    let reason = match status.as_u16() {
        403 if cloudflare => format!(
            "{service} is behind Cloudflare bot protection and refuses automated requests (403). \
             That is the site's choice, and this educational build does not try to work \
             around it -- the offline provider is what answers here."
        ),
        401 | 403 => format!("{service} refused the request ({status})"),
        429 => format!("{service} is rate limiting us ({status}); wait and retry"),
        404 => format!("{service} no longer serves this endpoint ({status})"),
        500..=599 => format!("{service} is having server trouble ({status})"),
        _ => format!("{service} answered {status}"),
    };
    bail!("{reason}")
}

fn base(number: &Number, source: &str, authoritative: bool) -> Lookup {
    Lookup {
        number: number.e164(),
        national: number.national(),
        calling_code: number.country.calling_code.clone(),
        country_iso: number.country.iso.clone(),
        country_name: number.country.name.clone(),
        source: source.into(),
        authoritative,
        ..Default::default()
    }
}

// ---------------------------------------------------------------- data24-7 --

/// api.data24-7.com v2.0 -- the only original service that was a real, paid
/// API. Batches up to N numbers as `p1=..&p2=..`, returns JSON.
pub struct Data247;

#[async_trait]
impl Provider for Data247 {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "data24-7".into(),
            label: "data24-7.com (paid API)".into(),
            credentials: Credentials::UserPass,
            legacy: true,
            authoritative: true,
            docs: Some("https://www.data24-7.com/".into()),
            health: Health::NeedsAccount,
            note: Some("The one 2014 service still commercially alive; needs a paid account.".into()),
        }
    }

    async fn lookup(&self, http: &reqwest::Client, number: &Number, cred: &Credential) -> Result<Lookup> {
        self.lookup_many(http, std::slice::from_ref(number), cred)
            .await
            .pop()
            .unwrap_or_else(|| Err(anyhow!("no result")))
    }

    async fn lookup_many(
        &self,
        http: &reqwest::Client,
        numbers: &[Number],
        cred: &Credential,
    ) -> Vec<Result<Lookup>> {
        let (Some(user), Some(pass)) = (cred.user.as_deref(), cred.pass.as_deref()) else {
            return numbers
                .iter()
                .map(|_| Err(anyhow!("data24-7 needs a username and password")))
                .collect();
        };
        let mut url = format!(
            "https://api.data24-7.com/v/2.0?user={}&pass={}&api=T",
            urlencoding::encode(user),
            urlencoding::encode(pass)
        );
        for (i, n) in numbers.iter().enumerate() {
            // The original always sent `1` + ten digits. `Number::digits()` is
            // the same string for +1 numbers and the correct international
            // form for every other country.
            url.push_str(&format!("&p{}={}", i + 1, n.digits()));
        }
        url.push_str("&out=json");

        let body: serde_json::Value = match http.get(&url).send().await.and_then(|r| r.error_for_status()) {
            Ok(r) => match r.json().await {
                Ok(v) => v,
                Err(e) => return numbers.iter().map(|_| Err(anyhow!("bad JSON: {e}"))).collect(),
            },
            Err(e) => return numbers.iter().map(|_| Err(anyhow!("{e}"))).collect(),
        };

        let results = body.pointer("/response/results").and_then(|v| v.as_array()).cloned();
        let Some(results) = results else {
            return numbers.iter().map(|_| Err(anyhow!("unexpected response: {body}"))).collect();
        };

        numbers
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let r = results.get(i).ok_or_else(|| anyhow!("missing result"))?;
                let status = r.get("status").and_then(|v| v.as_str()).unwrap_or("");
                if status != "OK" {
                    bail!("data24-7 returned status {status}");
                }
                let mut l = base(n, "data24-7", true);
                l.wireless = r.get("wless").and_then(|v| v.as_str()).map(|v| v == "y");
                l.carrier_comment = r.get("carrier_name").and_then(|v| v.as_str()).map(String::from);
                l.sms_gateway = r.get("sms_address").and_then(|v| v.as_str()).map(String::from);
                l.mms_gateway = r.get("mms_address").and_then(|v| v.as_str()).map(String::from);
                super::enrich(&mut l, n);
                Ok(l)
            })
            .collect()
    }
}

// -------------------------------------------------------------- xminder.com --

/// xminder.com/number.check.php -- free JSON endpoint with a daily quota.
pub struct XMinder;

#[async_trait]
impl Provider for XMinder {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "xminder.com".into(),
            label: "xminder.com (legacy)".into(),
            credentials: Credentials::None,
            legacy: true,
            authoritative: true,
            docs: None,
            health: Health::Offline,
            note: Some("Host no longer resolves; kept for reference.".into()),
        }
    }

    async fn lookup(&self, http: &reqwest::Client, number: &Number, _c: &Credential) -> Result<Lookup> {
        // NANP numbers keep the exact 10-digit form the original sent; other
        // countries get their full international digits.
        let query = number.ten().unwrap_or_else(|| number.digits());
        let url = format!("http://www.xminder.com/number.check.php?number={query}");
        let res = http.get(url).send().await?;
        let text = body_or_reason(res, "xminder.com").await?;
        let body: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| anyhow!("xminder.com did not return JSON ({e})"))?;
        if body.get("success").and_then(|v| v.as_bool()) != Some(true) {
            bail!("xminder returned failure (usually daily quota): {body}");
        }
        let d = body.get("data").ok_or_else(|| anyhow!("no data field"))?;
        let mut l = base(number, "xminder.com", true);
        l.wireless = d.get("status").and_then(|v| v.as_str()).map(|s| s == "YES");
        l.carrier_comment = d.get("carrier_name").and_then(|v| v.as_str()).map(String::from);
        l.sms_gateway = d.get("sms_address").and_then(|v| v.as_str()).map(String::from);
        l.mms_gateway = d.get("mms_address").and_then(|v| v.as_str()).map(String::from);
        super::enrich(&mut l, number);
        Ok(l)
    }
}

// ------------------------------------------------------- freecarrierlookup --

/// freecarrierlookup.com -- POST form, answer scraped out of a "yellowbox"
/// div as `Label: value<br>` pairs.
pub struct FreeCarrierLookup;

#[async_trait]
impl Provider for FreeCarrierLookup {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "freecarrierlookup.com".into(),
            label: "freecarrierlookup.com (legacy scraper)".into(),
            credentials: Credentials::None,
            legacy: true,
            authoritative: true,
            docs: None,
            health: Health::Blocked,
            note: Some(
                "Sits behind Cloudflare and returns 403 to automated requests. This build \
                 does not try to get around that; it is here to show how the 2014 scraper \
                 worked."
                    .into(),
            ),
        }
    }

    async fn lookup(&self, http: &reqwest::Client, number: &Number, _c: &Credential) -> Result<Lookup> {
        // The site's form has always had a country-code field; the original
        // pinned it to `1`. Sending the real calling code with the national
        // number is what makes this work outside North America.
        let cc = number.country.calling_code.clone();
        let national = number.national_number.clone();
        let form = [
            ("cc", cc.as_str()),
            ("phonenum", national.as_str()),
            ("error", "true"),
            // `x` and `y` are the image-submit button coordinates the form
            // required; kept as the original sent them.
            ("x", "95"),
            ("y", "10"),
        ];
        let res = http
            .post("https://www.freecarrierlookup.com/")
            .header("Referer", "https://www.freecarrierlookup.com/")
            .form(&form)
            .send()
            .await?;
        let html = body_or_reason(res, "freecarrierlookup.com").await?;

        let doc = Html::parse_document(&html);
        let sel = Selector::parse(".yellowbox, #yellowbox, .result").unwrap();
        let text = doc
            .select(&sel)
            .next()
            .map(|e| e.text().collect::<Vec<_>>().join("\n"))
            .ok_or_else(|| anyhow!("freecarrierlookup returned no result box (site layout changed or blocked)"))?;

        let mut map = std::collections::HashMap::new();
        for line in text.lines() {
            if let Some((k, v)) = line.split_once(':') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        if map.is_empty() {
            bail!("could not parse freecarrierlookup response");
        }
        let mut l = base(number, "freecarrierlookup.com", true);
        l.wireless = map.get("Is Wireless").map(|v| v.eq_ignore_ascii_case("y") || v.eq_ignore_ascii_case("yes"));
        l.carrier_comment = map.get("Carrier").cloned();
        l.sms_gateway = map.get("SMS Gateway Address").cloned().filter(|s| s.contains('@'));
        l.mms_gateway = map.get("MMS Gateway Address").cloned().filter(|s| s.contains('@'));
        // The site reports the country for international lookups; keep it when
        // our own calling-code table could not name one (shared codes).
        if l.country_name.is_none() {
            l.country_name = map.get("Country").cloned();
        }
        super::enrich(&mut l, number);
        if !number.is_nanp() && l.sms_gateway.is_none() {
            l.notes.push(
                "Email-to-SMS gateways are a North American convention; most carriers elsewhere have none.".into(),
            );
        }
        Ok(l)
    }
}

// ------------------------------------------------------------- fonefinder --

/// fonefinder.net/findome.php -- HTML table scrape. Prefix-block data, so
/// results are marked non-authoritative exactly as the original did.
pub struct FoneFinder;

#[async_trait]
impl Provider for FoneFinder {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "fonefinder.net".into(),
            label: "fonefinder.net (legacy scraper, +1 only)".into(),
            credentials: Credentials::None,
            legacy: true,
            authoritative: false,
            docs: None,
            health: Health::Offline,
            note: Some("Host no longer serves the lookup page; kept for reference.".into()),
        }
    }

    async fn lookup(&self, http: &reqwest::Client, number: &Number, _c: &Credential) -> Result<Lookup> {
        let url = format!(
            "http://www.fonefinder.net/findome.php?npa={}&nxx={}&thoublock={}&usaquerytype=Search+by+Number&cityname=",
            number.area().ok_or_else(|| anyhow!(
                "fonefinder.net only covers North American (+1) numbers -- its query takes an area code and exchange"
            ))?,
            number.prefix().unwrap_or_default(),
            number.line().unwrap_or_default()
        );
        let res = http.get(url).send().await?;
        let html = body_or_reason(res, "fonefinder.net").await?;
        let doc = Html::parse_document(&html);
        let row = Selector::parse("table tr").unwrap();
        let cell = Selector::parse("td").unwrap();

        // The answer is the second row of the first results table.
        let cells: Vec<String> = doc
            .select(&row)
            .nth(1)
            .ok_or_else(|| anyhow!("fonefinder returned no result table (site layout changed)"))?
            .select(&cell)
            .map(|c| c.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .collect();
        if cells.len() < 6 {
            bail!("unexpected fonefinder table shape");
        }
        let mut l = base(number, "fonefinder.net", false);
        l.city = cells.get(2).cloned().filter(|s| !s.is_empty());
        l.state_code = cells.get(3).cloned().filter(|s| !s.is_empty());
        l.carrier_comment = cells.get(4).cloned().filter(|s| !s.is_empty());
        l.line_type = cells.get(5).cloned().filter(|s| !s.is_empty());
        l.notes.push("Prefix-block data: a ported number may be on another carrier.".into());
        super::enrich(&mut l, number);
        Ok(l)
    }
}
