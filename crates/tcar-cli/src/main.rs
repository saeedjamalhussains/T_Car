//! `tcar` -- the T-Car command line, a thin front end over `tcar-core`.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use tcar::{export, extract, Config, Engine};

#[derive(Parser)]
#[command(name = "tcar", version, about = "T-Car: look up the carrier and SMS/MMS gateway for a phone number")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum Format {
    Text,
    Json,
    Csv,
}

#[derive(Subcommand)]
enum Command {
    /// Look up one number
    Lookup {
        number: String,
        /// Force a single provider instead of the configured chain
        #[arg(short, long)]
        provider: Option<String>,
        #[arg(short, long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Look up many numbers from a file, or from stdin with `-`
    Bulk {
        /// File of numbers (any text; numbers are extracted). Use `-` for stdin.
        input: String,
        #[arg(short, long, value_enum, default_value_t = Format::Csv)]
        format: Format,
    },
    /// Validate and reformat a number without any lookup
    Parse { number: String },
    /// List the country calling codes this build recognizes
    Countries {
        /// Filter by name, ISO code, or calling code
        filter: Option<String>,
    },
    /// List available providers and whether they are configured
    Providers,
    /// Show or edit configuration
    Config {
        /// Set the provider chain, e.g. `--chain offline,twilio`
        #[arg(long, value_delimiter = ',')]
        chain: Option<Vec<String>>,
        /// Set credentials: `--set-key twilio:SID:TOKEN` or `--set-key numverify:KEY`
        #[arg(long)]
        set_key: Option<String>,
        /// Calling code assumed for numbers typed without a `+`, e.g. `91`
        #[arg(long)]
        default_country: Option<String>,
    },
    /// Print an SMS-gateway mailto: link for a number
    Mailto {
        number: String,
        #[arg(default_value = "")]
        message: String,
        /// Prefer the MMS gateway
        #[arg(long)]
        mms: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load();

    match cli.command {
        Command::Parse { number } => {
            let engine = Engine::new(config)?;
            let n = engine.parse(&number)?;
            println!("{}", n.e164());
            println!("{}", n.national());
            println!(
                "+{}  {}",
                n.country.calling_code,
                n.country.name.as_deref().unwrap_or("(calling code shared by several countries)")
            );
            println!("{}", export::tel_link(&n));
        }

        Command::Countries { filter } => {
            let needle = filter.unwrap_or_default().to_lowercase();
            for c in Engine::countries() {
                let iso = c.iso.unwrap_or_default();
                let name = c.name.unwrap_or_default();
                if needle.is_empty()
                    || name.to_lowercase().contains(&needle)
                    || iso.to_lowercase() == needle
                    || c.calling_code == needle.trim_start_matches('+')
                {
                    println!("+{:<5} {:<3} {}", c.calling_code, iso, name);
                }
            }
        }

        Command::Providers => {
            for p in Engine::providers() {
                let cred = config.credential(&p.id);
                let ready = match p.credentials {
                    tcar::providers::Credentials::None => true,
                    tcar::providers::Credentials::ApiKey => cred.key.is_some(),
                    tcar::providers::Credentials::UserPass => {
                        cred.user.is_some() && cred.pass.is_some()
                    }
                };
                let health = match p.health {
                    tcar::providers::Health::Ok => "works",
                    tcar::providers::Health::NeedsAccount if ready => "account set",
                    tcar::providers::Health::NeedsAccount => "needs an account",
                    tcar::providers::Health::Blocked => "blocked (bot protection)",
                    tcar::providers::Health::Offline => "host gone",
                };
                println!(
                    "{:<22} {:<42} {:<26}{}",
                    p.id,
                    p.label,
                    health,
                    if config.chain.contains(&p.id) { "[in chain]" } else { "" }
                );
                if let Some(note) = &p.note {
                    println!("{:<22} {}", "", note);
                }
            }
        }

        Command::Config { chain, set_key, default_country } => {
            let mut config = config;
            let mut changed = false;
            if let Some(chain) = chain {
                config.chain = chain;
                changed = true;
            }
            if let Some(cc) = default_country {
                config.default_calling_code = cc.trim_start_matches('+').to_string();
                changed = true;
            }
            if let Some(spec) = set_key {
                let parts: Vec<&str> = spec.splitn(3, ':').collect();
                let entry = config.credentials.entry(parts[0].to_string()).or_default();
                match parts.len() {
                    2 => entry.key = Some(parts[1].into()),
                    3 => {
                        entry.user = Some(parts[1].into());
                        entry.pass = Some(parts[2].into());
                    }
                    _ => anyhow::bail!("expected provider:KEY or provider:USER:PASS"),
                }
                changed = true;
            }
            if changed {
                config.save()?;
                println!("saved {}", Config::path().display());
            } else {
                println!("{}", serde_json::to_string_pretty(&config)?);
            }
        }

        Command::Lookup { number, provider, format } => {
            let engine = Engine::new(config)?;
            let result = match provider {
                Some(p) => engine.lookup_with(&p, &number).await,
                None => engine.lookup(&number).await,
            }?;
            match format {
                Format::Json => println!("{}", serde_json::to_string_pretty(&result)?),
                Format::Csv => print!(
                    "{}",
                    export::to_csv(&[tcar::BulkItem {
                        input: number,
                        ok: true,
                        lookup: Some(result),
                        error: None
                    }])
                ),
                Format::Text => print_text(&result),
            }
        }

        Command::Bulk { input, format } => {
            let text = if input == "-" {
                use std::io::Read;
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s)?;
                s
            } else {
                std::fs::read_to_string(&input).with_context(|| format!("reading {input}"))?
            };
            let numbers = extract::extract_numbers(&text);
            if numbers.is_empty() {
                anyhow::bail!("no phone numbers found in {input}");
            }
            let engine = Engine::new(config)?;
            let items = engine.lookup_many(&numbers).await;
            match format {
                Format::Json => println!("{}", serde_json::to_string_pretty(&items)?),
                Format::Csv => print!("{}", export::to_csv(&items)),
                Format::Text => {
                    for it in &items {
                        match &it.lookup {
                            Some(l) => print_text(l),
                            None => println!("{}: {}", it.input, it.error.as_deref().unwrap_or("failed")),
                        }
                    }
                }
            }
        }

        Command::Mailto { number, message, mms } => {
            let engine = Engine::new(config)?;
            let result = engine.lookup(&number).await?;
            let link = export::mailto(&result, &message, mms)
                .context("no SMS/MMS gateway is known for this carrier")?;
            println!("{link}");
        }
    }
    Ok(())
}

fn print_text(l: &tcar::Lookup) {
    let row = |k: &str, v: Option<&str>| {
        if let Some(v) = v.filter(|s| !s.is_empty()) {
            println!("  {k:<14} {v}");
        }
    };
    println!("{}  ({})", l.national, l.number);
    row("carrier", l.carrier_comment.as_deref().or(l.carrier.as_deref()));
    row("type", l.line_type.as_deref());
    row(
        "wireless",
        l.wireless.map(|w| if w { "yes" } else { "no" }),
    );
    row("sms", l.sms_gateway.as_deref());
    row("mms", l.mms_gateway.as_deref());
    row("city", l.city.as_deref());
    row(
        "country",
        l.country_name.as_deref().or(l.country_iso.as_deref()),
    );
    row("state", l.state.as_deref().or(l.state_code.as_deref()));
    row("source", Some(&l.source));
    for note in &l.notes {
        println!("  note           {note}");
    }
}
