# Reverse engineering `node-tel-carrier` (2014)

Notes taken while reading [coolaj86/node-tel-carrier](https://github.com/coolaj86/node-tel-carrier)
(v1.3.10, last touched 2014) before rebuilding it. The upstream README says
only "Moved" and points at a `git.daplie.com` host that no longer resolves, so
everything below comes from the source itself.

## What the package did

`TelCarrier.create({ service, username, password }).lookup(number, cb)` turned a
US/Canada phone number into:

```js
{ number: '+18013604427'
, wireless: true
, carrier: 'verizon'              // normalized key from tel-carrier-gateways
, carrierComment: 'Verizon Wireless'
, smsGateway: '8013604427@vtext.com'
, mmsGateway: '8013604427@vzwpix.com'
}
```

The point was the *gateway* addresses: emailing them sends an SMS/MMS to the
handset for free.

## Architecture

| Piece | Role |
| --- | --- |
| `tel-carrier.js` | factory, number normalization, analytics upload |
| `services/index.js` | name -> service module map |
| `services/*.js` | one module per upstream data source |
| `tel-carrier-db` (npm) | offline area-code + exchange -> telco table (~6 MB JSON) |
| `tel-carrier-gateways` (npm) | carrier name/domain -> SMS & MMS gateway domains |

A service module is a function
`(request, jar, number, opts, cb, pre, post)` calling back with a partial map,
plus an optional third argument `{ authoritative: false }` meaning "this came
from a prefix block, not from the number itself". `data24-7.com` additionally
exports `.one` and `.many`; `tel-carrier.js` picks `service.many` when handed an
array.

### Number normalization

```js
map.number = '+1' + /(?=\+?1)?(\d{10})$/.exec(String(map.number))[1];
```

The lookahead is a no-op and the regex is unanchored at the front, so
`"garbage8013604427"` parsed fine while anything shorter threw a `TypeError` on
`null[1]`. NANP rules (area and exchange codes must start with 2-9) were not
checked at all.

### The analytics call

Every successful lookup POSTed to `http://tel-carrier.coolaj86.com/analytics`:

```json
{ "carriers": { "verizon": { "sms": "vtext.com", "mms": "vzwpix.com" } },
  "numbers":  { "8013604427": { "wireless": true, "carrier": "verizon" } } }
```

i.e. the looked-up phone number and its carrier were silently uploaded to a
third party, with a `GET /gateways` on module load to prime the registry. Both
endpoints are dead today (the host does not answer). **This rebuild does not
reimplement either one.**

## The four services, as wire formats

### `data24-7.com` — paid JSON API (`one` + `many`)

```
GET https://api.data24-7.com/v/2.0?user=U&pass=P&api=T&p1=18013604427&p2=...&out=json
```
```json
{ "response": { "results": [ { "status": "OK", "number": "17818640108",
  "wless": "y", "carrier_name": "Sprint", "carrier_id": "9",
  "sms_address": "7818640108@messaging.sprintpcs.com",
  "mms_address": "7818640108@pm.sprint.com" } ] } }
```
Numbers are positional: `p1..pN` map to `results[0..N-1]`. Bug in the original:
`if (!results && results.length)` throws whenever `results` is missing — the
error path was unreachable, the crash path was not.

### `xminder.com` — free JSON, daily quota

```
GET http://www.xminder.com/number.check.php?number=8013604427
```
```json
{ "success": true, "data": { "number": "8013604427", "status": "YES",
  "carrier_name": "Verizon Wireless", "sms_address": "8013604427@vtext.com",
  "mms_address": "8013604427@vzwpix.com" } }
```
`status: "YES"` means wireless. `success: false` usually meant quota exhausted.

### `freecarrierlookup.com` — form POST + HTML scrape

`POST /` with `cc=1&phonenum=<10 digits>&error=true&x=95&y=10` (`x`/`y` are
image-submit-button coordinates; the site required them). The answer was inside
a `yellowbox` element as `Label: value<br>` pairs: `Phone Number`,
`Is Wireless`, `Carrier`, `SMS Gateway Address`, `MMS Gateway Address`.

### `fonefinder.net` — HTML table scrape

```
GET /findome.php?npa=801&nxx=360&thoublock=4427&usaquerytype=Search+by+Number&cityname=
```
The original chewed through the markup with three regexes and index arithmetic
("lines 0 & 1 are area code and prefix, 2 & 3 are bogus"), taking cell 4 as the
carrier and cell 5 as the line type. Prefix-block data, hence
`{ authoritative: false }`.

### `tel-carrier-cache` — the offline database

`telDb.lookup(1, npa, nxx, xxxx)` against `data.json` + `meta.json`. Rows are
integer-interned:

```
data.list["801"] = [ [prefix, city, state, st, company, type, link, carrier, carrierName], ... ]
```
where every field except `prefix` indexes the matching array in `meta.json`.
`wireless` was inferred by testing the type/company strings for
`/wireless|pcs/i`.

## What is dead in 2026

| Source | Status |
| --- | --- |
| `tel-carrier.coolaj86.com` (gateways + analytics) | host does not resolve |
| `xminder.com` | gone |
| `fonefinder.net` | gone / no result table |
| `freecarrierlookup.com` | live but bot-protected; the old form shape no longer parses |
| `data24-7.com` | still a commercial API; needs an account |
| `tel-carrier-db` / `tel-carrier-gateways` npm packages | still downloadable, last published 2014 |

The 2014 data is therefore usable for prefix-block geography and carrier
guesses, but not for anything ported since — which is most mobile numbers.
That is why the rebuild marks offline results non-authoritative and lets you
chain a live API in front of them.

## How T-Car maps onto the original

| Original | Now |
| --- | --- |
| `services/*.js` | `crates/tcar-core/src/providers/legacy.rs` (ported verbatim in behavior) |
| — | no commercial API is integrated: this is an educational build |
| `tel-carrier-cache` + `tel-carrier-db` | `offline.rs` with the same tables gzipped and embedded (`datasets/nanp-prefixes.json.gz`, regenerate with `scripts/build-datasets.js`) |
| `tel-carrier-gateways` | `gateways.rs` + `datasets/nanp-carrier-gateways.json` |
| loose regex normalization | `number.rs`, with NANP validation and real errors |
| `opts.authoritative === false` | `Lookup.authoritative` + a note on every result |
| analytics upload | removed |
| single service per instance | ordered provider chain with fallback |
| `service.many` (data24-7 only) | `lookup_many` for every provider, batched where the API supports it |

## Going worldwide

The original was North American by construction: a NANP regex, an NPA/NXX
database, and an email-to-SMS gateway table that only ever listed US and
Canadian carriers. The rebuild keeps that data but no longer assumes it:

- `number.rs` parses any E.164 number against a table of 208 ITU country
  calling codes (`datasets/calling-codes.json`), longest-prefix first, and applies
  NANP structural rules only when the calling code is `1`.
- Bare digits with no `+` are read using a configurable default country
  (`defaultCallingCode`, `1` out of the box).
- The offline database and the gateway registry now decline non-`+1` numbers
  with an explicit message instead of guessing.
- The legacy services take the calling code too, instead of the hard-coded
  North America the original assumed:

  | service | 2014 request | now |
  | --- | --- | --- |
  | freecarrierlookup.com | `cc=1&phonenum=<10 digits>` | `cc=<calling code>&phonenum=<national number>` |
  | data24-7.com | `p1=1<10 digits>` | `p1=<full international digits>` |
  | xminder.com | `number=<10 digits>` | 10 digits for +1, international digits otherwise |
  | fonefinder.net | `npa=&nxx=&thoublock=` | unchanged: the query *is* an NANP area code and exchange, so it stays +1 only |

  Note this changes what we *ask*; whether a given host still answers is a
  separate matter, and today most of them do not.

What is still missing for real worldwide coverage — per-country scrapers,
an MCC/MNC operator database, non-NANP gateway domains, portability data — is
listed under "Not in this build" in the README.
