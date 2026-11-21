//! Tauri commands: a thin, typed bridge from the T-Car UI to `tcar-core`.

use tokio::sync::RwLock;
use tauri::State;
use tcar::{export, extract, BulkItem, Config, Engine, Lookup, ProviderInfo};

struct AppState {
    engine: RwLock<Engine>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
async fn lookup(state: State<'_, AppState>, number: String, provider: Option<String>) -> Result<Lookup, String> {
    let engine = state.engine.read().await;
    match provider.as_deref() {
        Some(p) if !p.is_empty() => engine.lookup_with(p, &number).await,
        _ => engine.lookup(&number).await,
    }
    .map_err(err)
}

#[tauri::command]
async fn lookup_many(state: State<'_, AppState>, text: String) -> Result<Vec<BulkItem>, String> {
    let numbers = extract::extract_numbers(&text);
    if numbers.is_empty() {
        return Err("No phone numbers found in that text".into());
    }
    let engine = state.engine.read().await;
    Ok(engine.lookup_many(&numbers).await)
}

#[tauri::command]
async fn parse_number(state: State<'_, AppState>, number: String) -> Result<serde_json::Value, String> {
    let n = state.engine.read().await.parse(&number).map_err(err)?;
    Ok(serde_json::json!({
        "e164": n.e164(),
        "national": n.national(),
        "callingCode": n.country.calling_code,
        "countryIso": n.country.iso,
        "countryName": n.country.name,
        "isNanp": n.is_nanp(),
        "tel": export::tel_link(&n),
    }))
}

// ---------------------------------------------------------------- Data tab --

/// Every area code in the embedded database, summarized.
#[tauri::command]
fn data_area_codes() -> Vec<tcar::offline::AreaSummary> {
    tcar::offline::area_codes()
}

/// Every exchange block under one area code.
#[tauri::command]
fn data_prefixes(area: String) -> Result<Vec<tcar::offline::PrefixRow>, String> {
    tcar::offline::prefixes(&area).ok_or_else(|| format!("area code {area} is not in the database"))
}

/// The carrier → SMS/MMS gateway registry.
#[tauri::command]
fn data_carriers() -> Vec<tcar::gateways::Carrier> {
    tcar::gateways::CARRIERS.clone()
}

/// Country calling codes, for the "assume this country" picker.
#[tauri::command]
fn countries() -> Vec<tcar::number::Country> {
    Engine::countries()
}

#[tauri::command]
fn providers() -> Vec<ProviderInfo> {
    Engine::providers()
}

#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<Config, String> {
    Ok(state.engine.read().await.config().clone())
}

#[tauri::command]
async fn set_config(state: State<'_, AppState>, config: Config) -> Result<(), String> {
    config.save().map_err(err)?;
    state.engine.write().await.set_config(config).map_err(err)
}

#[tauri::command]
fn to_csv(items: Vec<BulkItem>) -> String {
    export::to_csv(&items)
}

#[tauri::command]
fn mailto_link(lookup: Lookup, message: String, mms: bool) -> Result<String, String> {
    export::mailto(&lookup, &message, mms).ok_or_else(|| "No gateway known for this carrier".into())
}

#[tauri::command]
fn db_info() -> serde_json::Value {
    let (areas, blocks) = tcar::offline::stats();
    serde_json::json!({
        "source": tcar::offline::source_description(),
        "areaCodes": areas,
        "prefixBlocks": blocks,
        "carriers": tcar::gateways::CARRIERS.len(),
        "countries": Engine::countries().len(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = Engine::new(Config::load()).expect("HTTP client builds");
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
            .manage(AppState { engine: RwLock::new(engine) })
        .invoke_handler(tauri::generate_handler![
            lookup,
            lookup_many,
            parse_number,
            countries,
            data_area_codes,
            data_prefixes,
            data_carriers,
            providers,
            get_config,
            set_config,
            to_csv,
            mailto_link,
            db_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running T-Car");
}
