#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use macroscope_storage::{MemoryStore, Store};
use serde::Serialize;
use std::sync::Mutex;

#[derive(Debug, Serialize)]
struct AppInfo {
    version: &'static str,
    platform: &'static str,
}

#[derive(Debug, Serialize)]
struct ToolSummary {
    id: String,
    name: String,
    version: u32,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
    }
}

#[tauri::command]
fn list_tools(
    store: tauri::State<'_, Mutex<MemoryStore>>,
) -> Result<Vec<ToolSummary>, &'static str> {
    let store = store.lock().map_err(|_| "storage_unavailable")?;
    let tools = store.list_tools().map_err(|_| "storage_unavailable")?;
    Ok(tools
        .into_iter()
        .map(|tool| ToolSummary {
            id: tool.id.0,
            name: tool.name,
            version: tool.version,
        })
        .collect())
}

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(MemoryStore::default()))
        .invoke_handler(tauri::generate_handler![app_info, list_tools])
        .run(tauri::generate_context!())
        .expect("Macroscope could not start");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_metadata_matches_package_and_host() {
        let info = app_info();
        assert_eq!(info.version, "0.1.0");
        assert_eq!(info.platform, std::env::consts::OS);
    }
}
