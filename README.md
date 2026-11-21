# T-Car

> ### ⚠️ Educational use only
> This project exists to demonstrate how phone-number → carrier lookup works:
> E.164 parsing, prefix-block databases, email-to-SMS gateways, and how the
> 2014 original talked to its data sources. It is **not** a production
> service and must not be used for billing, call routing, fraud scoring,
> compliance checks, marketing lists, or anything that affects a real person.
> The bundled offline data is from 2014 and is wrong for any ported number.
> Phone numbers are personal data — handle them under your local law, and
> respect the terms of every API you point it at.
>
> **Full version** — worldwide carrier scrapers, live gateway data, and an
> HNI / MCC-MNC database — is not part of this build.
> Email **projectsjh18@gmail.com**.

**T-Car** is a demo app: it looks up the carrier, line type, and **SMS/MMS
email gateway** for a phone number from a native app, the `tcar` command, or a
Rust library. One core, three front ends, identical answers.

Lookups are answered by a **bundled offline database**. The four services the
2014 original scraped are kept **exactly as they were, as study material** —
their hosts are gone, bot-protected, or paywalled, and the app labels each one
so you can see which is which. No commercial lookup API is wired in; this is
teaching material, not a service.

A ground-up rebuild of AJ ONeal's `node-tel-carrier` (2014).

What the original did, what its wire formats were, and what has since died is
written up in [docs/REVERSE-ENGINEERING.md](docs/REVERSE-ENGINEERING.md).

```
(801) 360-4427  (+18013604427)
  carrier        Verizon
  type           WIRELESS PROV
  wireless       yes
  sms            8013604427@vtext.com
  mms            8013604427@vzwpix.com
  city           PROVO
  state          Utah
  source         offline-db
  note           Prefix-block data: a ported number may be on another carrier.
```

## Layout

```
t-car/
├─ crates/
│  ├─ tcar-core/            the library everything else calls (lib name: tcar)
│  │  └─ src/
│  │     ├─ number.rs       worldwide E.164 parsing + country calling codes
│  │     ├─ offline.rs      embedded NANP prefix database
│  │     ├─ gateways.rs     carrier ↔ SMS/MMS gateway domain registry
│  │     ├─ providers/      one lookup source per file
│  │     │  ├─ mod.rs       the Provider trait, registry, shared enrichment
│  │     │  └─ legacy.rs    the 2014 services, kept as study material
│  │     ├─ extract.rs      pull phone numbers out of arbitrary text
│  │     ├─ export.rs       CSV rows, mailto: and tel: links
│  │     ├─ config.rs       provider chain, credentials, default country
│  │     ├─ types.rs        Lookup / BulkItem
│  │     └─ lib.rs          Engine: parse → provider chain → result
│  └─ tcar-cli/             the `tcar` command
├─ apps/
│  └─ desktop/              Tauri 2 shell (macOS, Windows, Linux; iOS/Android capable)
│     ├─ src/lib.rs         Tauri commands — a thin bridge to tcar-core
│     └─ ui/                index.html · style.css · app.js (no build step)
├─ datasets/
│  ├─ nanp-prefixes.json.gz        North American prefix table (from 2014)
│  ├─ nanp-carrier-gateways.json   carrier → SMS/MMS domains
│  └─ calling-codes.json           ITU country calling codes (hand-maintained)
├─ assets/logo/                    the T-Car mark: SVG, light/dark PNGs, originals
├─ scripts/
│  ├─ build-datasets.js            regenerates the two NANP datasets
│  ├─ build-icons.mjs              redraws the mark and the platform icon source
│  ├─ build-linux.sh               Linux bundles from a Mac, via Docker
│  ├─ linux-builder.Dockerfile     the container that does it
│  └─ build-windows-cli.sh         cross-build tcar.exe (CLI only) with mingw-w64
├─ .github/workflows/release.yml   macOS + Windows + Linux bundles in CI
└─ docs/
   ├─ BUILDING.md                  per-OS build, install, and troubleshooting
   └─ REVERSE-ENGINEERING.md       how the 2014 original worked
```

Everything shares one core, so the app, the CLI, and any Rust caller give
identical answers.

## Coverage: what is worldwide and what is not

| Capability | Coverage |
| --- | --- |
| Number parsing / validation (E.164, 208 calling codes) | **worldwide** |
| Country detection from the calling code | worldwide, except codes shared by several countries (+1 outside US/CA, +7) |
| Legacy service requests (calling code threaded through) | worldwide in shape — but the hosts do not answer |
| Bundled offline prefix database | North America (+1) only — it is the 2014 NANP table |
| Email-to-SMS / MMS gateway addresses | North America only — that registry never covered other countries |

So a `+91`, `+44`, or `+234` number parses and formats correctly, but nothing
in this build can name its carrier: the offline table is North American, and
the legacy services are dead. Worldwide carrier data is a full-version feature
— email **projectsjh18@gmail.com**.

## The app

```bash
cargo install tauri-cli --version "^2" --locked   # once
cd apps/desktop && cargo tauri build              # native bundle for the host OS
cargo tauri dev                                   # or run it live
```

| OS | you get |
| --- | --- |
| macOS | `T-Car.app` + `.dmg` |
| Windows | `.msi` and an NSIS `.exe` installer |
| Linux | `.deb`, `.rpm`, `.AppImage` |

Each bundle must be built **on** its own OS — the shell links to a platform
webview (WebKit / WebView2 / webkit2gtk), so GUI bundles do not cross-compile.
Per-platform prerequisites, commands, install steps, and troubleshooting are in
**[docs/BUILDING.md](docs/BUILDING.md)**; `.github/workflows/release.yml` builds
macOS, Windows, and Linux together on a tag or on demand. The same crate targets
iOS and Android via `cargo tauri ios init` / `android init`.

Five tabs:

- **Lookup** — live-validates as you type, copy buttons on the gateway addresses,
  one click to compose a text through the gateway.
- **Bulk** — paste any text; numbers are extracted, looked up concurrently, and
  exported as CSV.
- **Data** — browse the bundled datasets directly: every area code, drill into
  one to see its exchange blocks, plus the carrier→gateway registry and the
  country calling codes. Filter any table, copy what you are looking at as CSV.
  This is the honest view of what a prefix-block lookup actually knows.
- **Providers** — tick which sources to use, in order; each is labelled with
  whether it still answers.
- **About** — scope, dataset counts, and what changed from the 2014 original.

## The CLI

```bash
cargo install --path crates/tcar-cli     # `tcar` onto your PATH, any OS
```

```bash
tcar lookup 801-360-4427                    # bare digits use your default country
tcar lookup "+91 98765 43210"               # any country, written with +
tcar lookup 801-360-4427 --format json      # machine readable
tcar lookup 801-360-4427 -p offline         # force one provider
tcar bulk contacts.csv > carriers.csv       # any text file, or `-` for stdin
tcar parse "+44 20 7946 0018"               # validate / reformat only
tcar countries india                        # calling codes this build knows
tcar mailto 801-360-4427 "running late"     # gateway mailto: link
tcar providers                              # what's available and configured
tcar config --chain offline
tcar config --set-key data24-7:USER:PASS    # the one legacy service still sold
tcar config --default-country 91            # bare numbers are Indian from now on
```

## Providers

Lookups walk your chain in order until one answers. Out of the box the chain is
just `offline`.

| id | what it is | state today |
| --- | --- | --- |
| `offline` | the embedded 2014 NANP prefix table, 338 area codes, no network needed | **works** — this is what answers |
| `data24-7` | the original's paid API, batches numbers | needs a paid account |
| `freecarrierlookup.com` | the original's form-post scraper | blocked — Cloudflare 403s automated requests |
| `xminder.com` | the original's free JSON endpoint | host gone |
| `fonefinder.net` | the original's HTML table scraper, +1 only | host gone |

The four legacy entries are unchanged ports kept for reading, not for use.
When one fails, T-Car says *why* — bot protection, dead host, missing account —
rather than surfacing a bare status code. Working around a site's bot
protection is out of scope here by design.

### Authoritative vs prefix-block

`offline` and `fonefinder.net` answer from the block a number was *issued* in.
Number portability means that is a guess for any mobile line — often a wrong
one. Results carry `authoritative: false` and say so on every row.

## Not in this build

Deliberately out of scope here, and available in the full version — email
**projectsjh18@gmail.com**:

- working carrier lookups: scrapers for the rest of the world (EU, India, SEA, LATAM, Africa, MEA) and integrations with commercial APIs
- a maintained MCC/MNC + HNI operator database instead of a 2014 name table
- live-updated SMS/MMS gateway domains beyond North America
- HLR/MNP-grade portability data
- a hosted API and scheduled bulk jobs

## Regenerating the data

```bash
npm pack tel-carrier-db@1.1.3 tel-carrier-gateways@2.0.0 && tar xzf *.tgz
node scripts/build-datasets.js <tel-carrier-db dir> <tel-carrier-gateways dir>
```

`datasets/calling-codes.json` (ITU country calling codes) is hand-maintained
and edited in place.

## Regenerating the icons

The mark lives in `assets/logo/`. `tcar-mark-{dark,light}-250.png` are the
supplied originals; everything else is derived from their measured geometry so
the icons stay sharp at every size:

```bash
node scripts/build-icons.mjs                      # SVG + 1024px PNGs + icon source
cd apps/desktop && cargo tauri icon icons/icon.png  # the per-platform icon set
```

That writes each platform's conventional names into `apps/desktop/icons/`:
`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns` (macOS),
`icon.ico` (Windows), `Square*Logo.png` + `StoreLogo.png` (Windows Store), and
the `ios/` and `android/` sets.

## Tests

```bash
cargo test
```

## License

Apache-2.0, following the original. Provided as-is, for learning, with no
warranty and no guarantee of accuracy.
