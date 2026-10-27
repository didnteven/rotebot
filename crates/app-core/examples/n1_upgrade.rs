//! Manual N-1 upgrade check (plan M3): a data folder written by the previous
//! build must open in this build with its tools and settings intact.
//!
//! Run `create <dir>` with the previous build, then `check <dir>` with this one.
//! Uses the debug file secret store under `<dir>/dev-secrets`, like the app.
use macroscope_app_core::{AppCore, CoreConfig, NativeMuse};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn core(dir: &Path) -> AppCore {
    let vault = macroscope_vault::FileStore::new(dir.join("dev-secrets")).expect("secret store");
    AppCore::new(CoreConfig {
        data_dir: dir.join("data"),
        vault: Arc::new(vault),
        gateway_base_url: "https://api.meta.ai/v1".into(),
        // A private home: no Muse binary or sign-in is looked up for real.
        home: dir.join("home"),
        muse: Arc::new(NativeMuse {
            home: dir.join("home"),
        }),
        app_version: env!("CARGO_PKG_VERSION").into(),
        platform: std::env::consts::OS.into(),
        now_ms: CoreConfig::system_clock(),
    })
    .expect("open data folder")
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (mode, dir) = (args[1].as_str(), PathBuf::from(&args[2]));
    match mode {
        "create" => {
            let core = core(&dir);
            let tool = core.install_sample_tool().expect("sample tool");
            core.set_local_only(true).await.expect("local only");
            std::fs::write(dir.join("tool-id"), &tool.summary_row.id).unwrap();
            println!("created schema with tool {}", tool.summary_row.id);
        }
        "check" => {
            let id = std::fs::read_to_string(dir.join("tool-id")).unwrap();
            let core = core(&dir);
            let tools = core.list_tools().expect("list tools");
            assert!(tools.iter().any(|t| t.id == id), "tool lost in upgrade");
            assert_eq!(core.get_tool(&id).expect("get tool").summary_row.id, id);
            assert_eq!(
                core.database().setting("local_only").unwrap(),
                Some(serde_json::json!(true)),
                "setting lost in upgrade"
            );
            assert!(core.list_schedules().expect("schedules table").is_empty());
            assert!(core
                .list_today()
                .expect("today table")
                .iter()
                .all(|d| d.entries.is_empty()));
            println!(
                "upgrade OK: schema {} with {} tool(s), Local only kept",
                core.database().schema_version().unwrap(),
                tools.len()
            );
        }
        _ => panic!("usage: n1_upgrade create|check <dir>"),
    }
}
