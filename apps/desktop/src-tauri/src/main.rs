#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Thin Tauri layer over `AppCore`: one command per `Backend` method, native pickers and
//! dialogs, event forwarding, and the tray. Product logic lives in `macroscope-app-core`.

use macroscope_app_core::connections::Connections;
use macroscope_app_core::dto::*;
use macroscope_app_core::show::ShowController;
use macroscope_app_core::{AppCore, CoreConfig, CoreError, NativeMuse};
use macroscope_domain::CsvMapping;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Runtime, State, WindowEvent};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// Errors cross IPC as `{code, message}`; messages are already safe to display.
#[derive(Debug, Serialize)]
struct CommandError {
    code: &'static str,
    message: String,
}

impl From<CoreError> for CommandError {
    fn from(e: CoreError) -> Self {
        CommandError {
            code: e.code(),
            message: e.to_string(),
        }
    }
}

fn invalid(message: impl Into<String>) -> CommandError {
    CommandError {
        code: "invalid",
        message: message.into(),
    }
}

type Cmd<T> = Result<T, CommandError>;

struct Core(AppCore);
struct Show(ShowController);
struct Conns(Connections);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Consent {
    outbound_reviewed: bool,
}

// ---------- app and model access ----------

#[tauri::command]
fn app_info(core: State<'_, Core>) -> AppInfo {
    core.0.app_info()
}

#[tauri::command]
async fn model_access(core: State<'_, Core>) -> Cmd<ModelAccessStatus> {
    Ok(core.0.model_access().await)
}

/// macOS: a native hidden-input dialog. The key travels only over the osascript
/// stdout pipe into the core and the Keychain; never argv, logs, or the webview.
#[tauri::command]
async fn enter_api_key_native(core: State<'_, Core>) -> Cmd<ModelAccessStatus> {
    if !cfg!(target_os = "macos") {
        return Err(invalid("Use the key field on this platform."));
    }
    let script = r#"text returned of (display dialog "Paste your Meta API key. Macroscope stores it in your macOS Keychain." default answer "" with hidden answer with title "Macroscope" buttons {"Cancel", "Save key"} default button "Save key")"#;
    let output = tokio::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .env_clear()
        .output()
        .await
        .map_err(|_| invalid("The key dialog could not open."))?;
    if !output.status.success() {
        return Ok(core.0.model_access().await); // cancelled
    }
    let mut key =
        String::from_utf8(output.stdout).map_err(|_| invalid("That key is not valid text."))?;
    let trimmed = key.trim_end_matches(['\n', '\r']).to_string();
    key.clear();
    if trimmed.is_empty() {
        return Ok(core.0.model_access().await);
    }
    Ok(core.0.save_api_key(&trimmed).await?)
}

#[tauri::command]
async fn save_api_key(core: State<'_, Core>, key: String) -> Cmd<ModelAccessStatus> {
    Ok(core.0.save_api_key(&key).await?)
}

#[tauri::command]
async fn remove_api_key(core: State<'_, Core>) -> Cmd<ModelAccessStatus> {
    Ok(core.0.remove_api_key().await?)
}

#[tauri::command]
async fn test_api_key(core: State<'_, Core>) -> Cmd<ModelAccessStatus> {
    Ok(core.0.test_api_key().await?)
}

#[tauri::command]
async fn refresh_muse_status(core: State<'_, Core>) -> Cmd<ModelAccessStatus> {
    Ok(core.0.refresh_muse_status().await)
}

/// Opens Terminal running `muse login`, only on this explicit click (ADR 0001 §1).
/// Macroscope itself never signs in or touches Muse's saved login.
#[tauri::command]
async fn open_muse_sign_in() -> Cmd<()> {
    if !cfg!(target_os = "macos") {
        return Err(invalid("Run `muse login` in a terminal."));
    }
    let status = tokio::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            r#"tell application "Terminal" to do script "muse login""#,
            "-e",
            r#"tell application "Terminal" to activate"#,
        ])
        .status()
        .await
        .map_err(|_| invalid("Terminal could not be opened."))?;
    if status.success() {
        Ok(())
    } else {
        Err(invalid(
            "Terminal could not be opened. Run `muse login` yourself.",
        ))
    }
}

#[tauri::command]
async fn select_route(core: State<'_, Core>, route: ModelRoute) -> Cmd<ModelAccessStatus> {
    Ok(core.0.select_route(route).await?)
}

#[tauri::command]
async fn set_muse_unverified_ok(core: State<'_, Core>, ok: bool) -> Cmd<ModelAccessStatus> {
    Ok(core.0.set_muse_unverified_ok(ok).await?)
}

#[tauri::command]
async fn set_local_only(core: State<'_, Core>, local_only: bool) -> Cmd<ModelAccessStatus> {
    Ok(core.0.set_local_only(local_only).await?)
}

// ---------- create ----------

#[tauri::command]
async fn pick_csv<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>) -> Cmd<Option<CsvPreview>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Choose a CSV")
        .add_filter("CSV", &["csv"])
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let handle = core.0.register_picked_file(&path)?;
    Ok(Some(core.0.preview_handle(&handle.handle_id)?))
}

#[tauri::command]
fn load_sample_csv(core: State<'_, Core>) -> Cmd<CsvPreview> {
    Ok(core.0.load_sample_csv()?)
}

#[tauri::command]
fn draft_task(core: State<'_, Core>, input: FileHandle, description: String) -> Cmd<TaskDraft> {
    Ok(core.0.draft_task(&input.handle_id, &description)?)
}

#[tauri::command]
fn answer_question(
    core: State<'_, Core>,
    draft_id: String,
    question_id: String,
    answer: String,
) -> Cmd<TaskDraft> {
    Ok(core.0.answer_question(&draft_id, &question_id, &answer)?)
}

#[tauri::command]
fn update_mapping(
    core: State<'_, Core>,
    draft_id: String,
    mapping: CsvMapping,
    output_name: String,
) -> Cmd<TaskDraft> {
    Ok(core.0.update_mapping(&draft_id, mapping, &output_name)?)
}

#[tauri::command]
async fn outbound_preview(core: State<'_, Core>, draft_id: String) -> Cmd<OutboundPreview> {
    Ok(core.0.outbound_preview(&draft_id).await?)
}

/// Starts a build and forwards its state changes as `build://<id>` events.
#[tauri::command]
async fn start_build<R: Runtime>(
    app: AppHandle<R>,
    core: State<'_, Core>,
    draft_id: String,
    consent: Consent,
) -> Cmd<BuildState> {
    let state = core
        .0
        .start_build(&draft_id, consent.outbound_reviewed)
        .await?;
    let mut rx = core.0.subscribe_build(&state.build_id)?;
    let state = core.0.build_state(&state.build_id)?;
    let channel = format!("build://{}", state.build_id);
    let current = core.0.clone();
    let id = state.build_id.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(update) = receive_update(&mut rx, || current.build_state(&id).ok()).await {
            let done = matches!(
                update.phase,
                BuildPhase::Ready | BuildPhase::Failed | BuildPhase::Stopped
            );
            let _ = app.emit(&channel, &update);
            if done {
                break;
            }
        }
    });
    Ok(state)
}

#[tauri::command]
fn build_state(core: State<'_, Core>, build_id: String) -> Cmd<BuildState> {
    Ok(core.0.build_state(&build_id)?)
}

#[tauri::command]
fn stop_build(core: State<'_, Core>, build_id: String) -> Cmd<BuildState> {
    Ok(core.0.stop_build(&build_id)?)
}

#[tauri::command]
async fn save_tool<R: Runtime>(
    app: AppHandle<R>,
    core: State<'_, Core>,
    build_id: String,
    name: String,
) -> Cmd<ToolDetail> {
    let tool = core.0.save_tool(&build_id, &name).await?;
    refresh_tray(&app);
    Ok(tool)
}

#[tauri::command]
fn build_try_input(core: State<'_, Core>, build_id: String, input: FileHandle) -> Cmd<BuildState> {
    Ok(core.0.build_try_input(&build_id, &input.handle_id)?)
}

// ---------- tools and runs ----------

#[tauri::command]
fn list_tools(core: State<'_, Core>) -> Cmd<Vec<ToolSummary>> {
    Ok(core.0.list_tools()?)
}

#[tauri::command]
fn get_tool(core: State<'_, Core>, tool_id: String) -> Cmd<ToolDetail> {
    Ok(core.0.get_tool(&tool_id)?)
}

#[tauri::command]
fn install_sample_tool<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>) -> Cmd<ToolDetail> {
    let tool = core.0.install_sample_tool()?;
    refresh_tray(&app);
    Ok(tool)
}

#[tauri::command]
fn delete_tool<R: Runtime>(app: AppHandle<R>, core: State<'_, Core>, tool_id: String) -> Cmd<()> {
    core.0.delete_tool(&tool_id)?;
    refresh_tray(&app);
    Ok(())
}

#[cfg(test)]
struct ExportSelection(PathBuf);

#[tauri::command]
async fn export_tool<R: Runtime>(
    app: AppHandle<R>,
    core: State<'_, Core>,
    tool_id: String,
) -> Cmd<Option<String>> {
    #[cfg(not(test))]
    let path = app
        .dialog()
        .file()
        .set_title("Export tool to folder")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok());
    #[cfg(test)]
    let path = app.try_state::<ExportSelection>().map(|s| s.0.clone());
    let Some(path) = path else {
        return Ok(None);
    };
    let handle = core.0.register_picked_dir(&path)?;
    Ok(Some(core.0.export_tool(&tool_id, &handle.handle_id)?))
}

#[tauri::command]
fn revoke_tool<R: Runtime>(
    app: AppHandle<R>,
    core: State<'_, Core>,
    tool_id: String,
) -> Cmd<ToolDetail> {
    let tool = core.0.revoke_tool(&tool_id)?;
    refresh_tray(&app);
    Ok(tool)
}

#[tauri::command]
fn restore_tool<R: Runtime>(
    app: AppHandle<R>,
    core: State<'_, Core>,
    tool_id: String,
) -> Cmd<ToolDetail> {
    let tool = core.0.restore_tool(&tool_id)?;
    refresh_tray(&app);
    Ok(tool)
}

#[tauri::command]
async fn pick_output_dir<R: Runtime>(
    app: AppHandle<R>,
    core: State<'_, Core>,
) -> Cmd<Option<FileHandle>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Choose an output folder")
        .blocking_pick_folder();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    Ok(Some(core.0.register_picked_dir(&path)?))
}

#[tauri::command]
fn stage_run(
    core: State<'_, Core>,
    tool_id: String,
    input: FileHandle,
    output_dir: FileHandle,
    output_name: String,
) -> Cmd<StageResult> {
    Ok(core.0.stage_run(
        &tool_id,
        &input.handle_id,
        &output_dir.handle_id,
        &output_name,
    )?)
}

#[tauri::command]
fn commit_run(core: State<'_, Core>, run_id: String, plan_hash: String) -> Cmd<CommitResult> {
    Ok(core.0.commit_run(&run_id, &plan_hash)?)
}

#[tauri::command]
fn discard_run(core: State<'_, Core>, run_id: String) -> Cmd<()> {
    Ok(core.0.discard_run(&run_id)?)
}

/// Reveals only a path recorded in a receipt; there is no generic open-path command.
#[tauri::command]
fn reveal_output<R: Runtime>(
    app: AppHandle<R>,
    core: State<'_, Core>,
    receipt_run_id: String,
) -> Cmd<()> {
    let path = core.0.receipt_output_path(&receipt_run_id)?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|_| invalid("The file could not be shown; it may have been moved."))
}

// ---------- activity and privacy ----------

#[tauri::command]
fn list_activity(core: State<'_, Core>) -> Cmd<Vec<ActivityItem>> {
    Ok(core.0.list_activity()?)
}

#[tauri::command]
fn list_receipts(core: State<'_, Core>) -> Cmd<Vec<Receipt>> {
    Ok(core.0.list_receipts()?)
}

#[tauri::command]
async fn privacy_overview(core: State<'_, Core>) -> Cmd<PrivacyOverview> {
    Ok(core.0.privacy_overview().await?)
}

#[tauri::command]
fn delete_preview(core: State<'_, Core>) -> Cmd<DeletePreview> {
    Ok(core.0.delete_preview()?)
}

#[tauri::command]
async fn delete_data<R: Runtime>(
    app: AppHandle<R>,
    show: State<'_, Show>,
    options: DeleteOptions,
) -> Cmd<()> {
    show.0.delete_data(options).await?;
    refresh_tray(&app);
    Ok(())
}

#[tauri::command]
fn delete_outbound_history(core: State<'_, Core>) -> Cmd<()> {
    Ok(core.0.delete_outbound_history()?)
}

#[tauri::command]
async fn emergency_stop(core: State<'_, Core>, show: State<'_, Show>) -> Cmd<()> {
    show.0.stop().await;
    Ok(core.0.emergency_stop().await?)
}

// ---------- connections ----------

#[tauri::command]
fn list_connectors(conns: State<'_, Conns>) -> Vec<ConnectorInfo> {
    conns.0.list()
}

#[tauri::command]
async fn set_connector_url(
    conns: State<'_, Conns>,
    connector_id: String,
    url: String,
) -> Cmd<ConnectorInfo> {
    Ok(conns.0.set_url(&connector_id, &url).await?)
}

#[tauri::command]
fn set_client_credentials(
    conns: State<'_, Conns>,
    connector_id: String,
    client_id: String,
    client_secret: Option<String>,
) -> Cmd<ConnectorInfo> {
    Ok(conns
        .0
        .set_client_credentials(&connector_id, &client_id, client_secret.as_deref())?)
}

/// Starts sign-in and opens the provider's page in the system browser.
#[tauri::command]
async fn begin_connect<R: Runtime>(
    app: AppHandle<R>,
    conns: State<'_, Conns>,
    connector_id: String,
) -> Cmd<ConnectStart> {
    let start = conns.0.begin_connect(&connector_id).await?;
    let _ = app
        .opener()
        .open_url(start.authorization_url.clone(), None::<&str>);
    Ok(start)
}

#[tauri::command]
async fn complete_connect(conns: State<'_, Conns>, pending_id: String) -> Cmd<ConnectorInfo> {
    Ok(conns.0.complete_connect(&pending_id).await?)
}

#[tauri::command]
async fn test_connection(conns: State<'_, Conns>, connector_id: String) -> Cmd<ConnectorInfo> {
    Ok(conns.0.test(&connector_id).await?)
}

#[tauri::command]
async fn disconnect(conns: State<'_, Conns>, connector_id: String) -> Cmd<ConnectorInfo> {
    Ok(conns.0.disconnect(&connector_id).await?)
}

#[tauri::command]
async fn preview_connector_tools(
    conns: State<'_, Conns>,
    connector_id: String,
) -> Cmd<Vec<ConnectorTool>> {
    Ok(conns.0.preview_tools(&connector_id).await?)
}

#[tauri::command]
fn connection_suggestions(conns: State<'_, Conns>) -> Cmd<Vec<ConnectionSuggestion>> {
    Ok(conns.0.suggestions()?)
}

#[tauri::command]
fn dismiss_suggestion(conns: State<'_, Conns>, connector_id: String) -> Cmd<()> {
    Ok(conns.0.dismiss(&connector_id)?)
}

// ---------- Show ----------

#[tauri::command]
fn show_state(show: State<'_, Show>) -> ShowState {
    match show.0.state() {
        ShowState::Idle { .. } => ShowState::Idle {
            accessibility: show.0.accessibility(),
        },
        other => other,
    }
}

#[tauri::command]
async fn start_show(show: State<'_, Show>) -> Cmd<ShowState> {
    Ok(show.0.start().await)
}

#[tauri::command]
async fn pause_show(show: State<'_, Show>, paused: bool) -> Cmd<ShowState> {
    Ok(show.0.pause(paused).await)
}

#[tauri::command]
async fn stop_show(show: State<'_, Show>) -> Cmd<ShowState> {
    Ok(show.0.stop().await)
}

/// The observer helper: `MACROSCOPE_OBSERVER`, a sidecar next to the executable, or the
/// development build inside the repository.
fn observer_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MACROSCOPE_OBSERVER") {
        return Some(PathBuf::from(p));
    }
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let sidecar = exe_dir.join("macroscope-observer");
    if sidecar.is_file() {
        return Some(sidecar);
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../native/macos-observer/.build/release/macroscope-observer");
    dev.is_file().then_some(dev)
}

/// Hosts whose documentation pages the UI may open. Authorization URLs from a pending
/// connection are allowed separately by the connections service.
fn allowed_external(url: &str) -> bool {
    const HOSTS: &[&str] = &[
        "dev.meta.ai",
        "developers.google.com",
        "console.cloud.google.com",
        "linear.app",
        "developers.notion.com",
        "docs.github.com",
        "github.com",
        "api.slack.com",
        "docs.slack.dev",
        "support.atlassian.com",
        "modelcontextprotocol.io",
    ];
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    !host.contains(['@', ':']) && HOSTS.contains(&host)
}

#[tauri::command]
fn open_external<R: Runtime>(app: AppHandle<R>, conns: State<'_, Conns>, url: String) -> Cmd<()> {
    if !allowed_external(&url) && !conns.0.allow_authorization_url(&url) {
        return Err(invalid(
            "That link is not on Macroscope's list of documentation sites.",
        ));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| invalid("The browser could not be opened."))
}

// ---------- startup ----------

async fn receive_update<T: Clone>(
    receiver: &mut tokio::sync::broadcast::Receiver<T>,
    snapshot: impl Fn() -> Option<T>,
) -> Option<T> {
    match receiver.recv().await {
        Ok(update) => Some(update),
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => Some(snapshot()?),
        Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn vault() -> Arc<dyn macroscope_vault::SecretStore> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(macroscope_vault::KeychainStore)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Arc::new(macroscope_vault::MemoryStore::default())
    }
}

fn tool_menu_rows(tools: Vec<ToolSummary>) -> Vec<(String, String)> {
    tools
        .into_iter()
        .take(5)
        .map(|tool| {
            let sample = if tool.sample && !tool.name.contains("(sample)") {
                " (sample)"
            } else {
                ""
            };
            (
                format!("run:{}", tool.id),
                format!("Run {}{}…", tool.name, sample),
            )
        })
        .collect()
}

fn tray_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    let open = MenuItem::with_id(app, "open", "Open Macroscope", true, None::<&str>)?;
    menu.append(&open)?;
    for (id, label) in tool_menu_rows(app.state::<Core>().0.list_tools().unwrap_or_default()) {
        menu.append(&MenuItem::with_id(app, id, label, true, None::<&str>)?)?;
    }
    menu.append(&MenuItem::with_id(
        app,
        "create",
        "Describe a task…",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "stop",
        "Stop builds and runs",
        true,
        None::<&str>,
    )?)?;
    let local = app
        .state::<Core>()
        .0
        .database()
        .setting("local_only")
        .ok()
        .flatten()
        == Some(serde_json::json!(true));
    menu.append(&CheckMenuItem::with_id(
        app,
        "local_only",
        "Local only",
        true,
        local,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Quit Macroscope",
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

fn refresh_tray<R: Runtime>(app: &AppHandle<R>) {
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(menu) = tray_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn open_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let menu = tray_menu(app.handle())?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Macroscope")
        .menu(&menu)
        .on_menu_event(move |app, event| {
            let core = app.state::<Core>().0.clone();
            match event.id().as_ref() {
                "open" => open_main(app),
                "create" => {
                    open_main(app);
                    let _ = app.emit("navigate://create", ());
                }
                "stop" => {
                    tauri::async_runtime::spawn(async move {
                        let _ = core.emergency_stop().await;
                    });
                }
                "local_only" => {
                    let on = core.database().setting("local_only").ok().flatten()
                        != Some(serde_json::json!(true));
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = core.set_local_only(on).await;
                        refresh_tray(&handle);
                    });
                }
                "quit" => app.exit(0),
                id if id.starts_with("run:") => {
                    open_main(app);
                    let _ = app.emit("navigate://run", &id[4..]);
                }
                _ => {}
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Every product command. Shared by the app and the IPC tests so the lists cannot drift.
fn handlers<R: Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        build_try_input,
        export_tool,
        revoke_tool,
        restore_tool,
        delete_preview,
        delete_data,
        app_info,
        model_access,
        enter_api_key_native,
        save_api_key,
        remove_api_key,
        test_api_key,
        refresh_muse_status,
        open_muse_sign_in,
        select_route,
        set_muse_unverified_ok,
        set_local_only,
        pick_csv,
        load_sample_csv,
        draft_task,
        answer_question,
        update_mapping,
        outbound_preview,
        start_build,
        build_state,
        stop_build,
        save_tool,
        list_tools,
        get_tool,
        install_sample_tool,
        delete_tool,
        pick_output_dir,
        stage_run,
        commit_run,
        discard_run,
        reveal_output,
        list_activity,
        list_receipts,
        privacy_overview,
        delete_outbound_history,
        emergency_stop,
        open_external,
        show_state,
        start_show,
        pause_show,
        stop_show,
        list_connectors,
        set_connector_url,
        set_client_credentials,
        begin_connect,
        complete_connect,
        test_connection,
        disconnect,
        preview_connector_tools,
        connection_suggestions,
        dismiss_suggestion,
    ]
}

/// The generated context (config, assets, ACL). Expanded once, shared with the IPC tests.
fn context<R: Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let home = home_dir();
            let core = AppCore::new(CoreConfig {
                data_dir,
                vault: vault(),
                gateway_base_url: "https://api.meta.ai/v1".into(),
                home: home.clone(),
                muse: Arc::new(NativeMuse { home }),
                app_version: env!("CARGO_PKG_VERSION").into(),
                platform: std::env::consts::OS.into(),
                now_ms: CoreConfig::system_clock(),
            })
            .map_err(|e| {
                Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error>
            })?;
            let show = ShowController::new(observer_path(), core.clone());
            let mut updates = show.subscribe();
            let handle = app.handle().clone();
            let current = show.clone();
            tauri::async_runtime::spawn(async move {
                while let Some(state) = receive_update(&mut updates, || Some(current.state())).await
                {
                    let _ = handle.emit("show://state", &state);
                }
            });
            let conns = Connections::new(core.clone(), vault()).map_err(|e| {
                Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error>
            })?;
            app.manage(Core(core));
            app.manage(Show(show));
            app.manage(Conns(conns));
            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it; Quit (tray) stops the app (plan §2.3).
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(handlers())
        .run(context())
        .expect("Macroscope could not start");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
    use tauri::webview::InvokeRequest;

    fn invoke<W: AsRef<tauri::Webview<tauri::test::MockRuntime>>>(
        w: &W,
        cmd: &str,
        args: Value,
    ) -> Result<Value, Value> {
        get_ipc_response(
            w,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: if cfg!(windows) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .unwrap(),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.into(),
            },
        )
        .map(|body| body.deserialize::<Value>().unwrap())
    }

    /// Sends the exact argument shapes `apps/desktop/src/backend/tauri.ts` sends, through
    /// the real command layer and ACL, so name or shape drift fails here.
    #[test]
    fn frontend_argument_shapes_drive_the_tell_flow_over_ipc() {
        let tmp = tempfile::tempdir().unwrap();
        let core = AppCore::new(CoreConfig {
            data_dir: tmp.path().join("data"),
            vault: Arc::new(macroscope_vault::MemoryStore::default()),
            gateway_base_url: "http://127.0.0.1:9".into(),
            home: tmp.path().join("home"),
            muse: Arc::new(NativeMuse {
                home: tmp.path().join("home"),
            }),
            app_version: "0.1.0".into(),
            platform: "macos".into(),
            now_ms: CoreConfig::system_clock(),
        })
        .unwrap();
        let out_dir = tmp.path().join("Reports");
        std::fs::create_dir_all(&out_dir).unwrap();
        let output = core.register_picked_dir(&out_dir).unwrap();
        let app = mock_builder()
            .invoke_handler(handlers())
            .manage(Show(ShowController::new(None, core.clone())))
            .manage(Conns(
                Connections::new(
                    core.clone(),
                    Arc::new(macroscope_vault::MemoryStore::default()),
                )
                .unwrap(),
            ))
            .manage(ExportSelection(out_dir.clone()))
            .manage(Core(core.clone()))
            // The generated context carries the real ACL manifest and capabilities.
            .build(context())
            .unwrap();
        let w = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        let info = invoke(&w, "app_info", json!({})).unwrap();
        assert_eq!(info["mock"], false);
        let listener = invoke(
            &w,
            "plugin:event|listen",
            json!({"event": "navigate://run", "target": {"kind": "Any"}, "handler": 0}),
        )
        .unwrap();
        invoke(
            &w,
            "plugin:event|unlisten",
            json!({"event": "navigate://run", "eventId": listener}),
        )
        .unwrap();
        assert!(invoke(
            &w,
            "plugin:event|emit",
            json!({"event": "navigate://run", "payload": "forged"})
        )
        .is_err());
        let access = invoke(&w, "model_access", json!({})).unwrap();
        assert_eq!(access["effectiveRoute"], "offline_demo");
        assert_eq!(access["muse"]["kind"], "missing");

        let preview = invoke(&w, "load_sample_csv", json!({})).unwrap();
        let draft = invoke(
            &w,
            "draft_task",
            json!({"input": preview["handle"], "description": "Clean orders"}),
        )
        .unwrap();
        let draft_id = draft["draftId"].clone();
        assert_eq!(draft["readyToBuild"], true);
        let draft = invoke(&w, "update_mapping", json!({"draftId": draft_id, "mapping": draft["mapping"], "outputName": "orders_ready.csv"})).unwrap();
        assert_eq!(draft["revision"], 2);
        let outbound = invoke(&w, "outbound_preview", json!({"draftId": draft_id})).unwrap();
        assert_eq!(outbound["containsRealRows"], false);

        let build = invoke(
            &w,
            "start_build",
            json!({"draftId": draft_id, "consent": {"outboundReviewed": false}}),
        )
        .unwrap();
        let build_id = build["buildId"].clone();
        let mut state = Value::Null;
        for _ in 0..200 {
            state = invoke(&w, "build_state", json!({"buildId": build_id})).unwrap();
            if state["phase"] == "ready" || state["phase"] == "failed" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert_eq!(state["phase"], "ready", "{state}");

        let rechecked = invoke(
            &w,
            "build_try_input",
            json!({"buildId": build_id, "input": preview["handle"]}),
        )
        .unwrap();
        assert_eq!(rechecked["phase"], "ready");
        assert_eq!(rechecked["comparison"]["rowsOut"], 18);
        let tool = invoke(
            &w,
            "save_tool",
            json!({"buildId": build_id, "name": "Clean import CSV"}),
        )
        .unwrap();
        let tool_id = tool["id"].clone();
        let exported = invoke(&w, "export_tool", json!({"toolId": tool_id})).unwrap();
        assert!(PathBuf::from(exported.as_str().unwrap())
            .join("README.txt")
            .exists());
        assert_eq!(
            invoke(&w, "revoke_tool", json!({"toolId": tool_id})).unwrap()["state"],
            "paused"
        );
        let denied = invoke(&w, "stage_run", json!({"toolId": tool_id, "input": preview["handle"], "outputDir": output, "outputName": "paused.csv"})).unwrap();
        assert_eq!(denied["failure"]["kind"], "revoked");
        assert_eq!(
            invoke(&w, "restore_tool", json!({"toolId": tool_id})).unwrap()["state"],
            "enabled"
        );
        assert_eq!(
            invoke(&w, "list_tools", json!({}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let input = invoke(&w, "load_sample_csv", json!({})).unwrap()["handle"].clone();
        let staged = invoke(
            &w,
            "stage_run",
            json!({"toolId": tool_id, "input": input, "outputDir": output, "outputName": "orders_ready.csv"}),
        )
        .unwrap();
        assert_eq!(staged["rowsOut"], 18, "{staged}");
        let receipt = invoke(
            &w,
            "commit_run",
            json!({"runId": staged["runId"], "planHash": staged["planHash"]}),
        )
        .unwrap();
        assert_eq!(receipt["status"], "committed", "{receipt}");
        assert!(out_dir.join("orders_ready.csv").exists());
        let stale = invoke(&w, "stage_run", json!({"toolId": tool_id, "input": input, "outputDir": output, "outputName": "stale.csv"})).unwrap();
        invoke(&w, "revoke_tool", json!({"toolId": tool_id})).unwrap();
        invoke(&w, "restore_tool", json!({"toolId": tool_id})).unwrap();
        assert_eq!(
            invoke(
                &w,
                "commit_run",
                json!({"runId": stale["runId"], "planHash": stale["planHash"]})
            )
            .unwrap()["failure"]["kind"],
            "revoked"
        );
        assert!(!out_dir.join("stale.csv").exists());
        let bad = core
            .register_picked_file(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../fixtures/orders/negative/bad-date.csv"),
            )
            .unwrap();
        let checked = invoke(
            &w,
            "build_try_input",
            json!({"buildId": build_id, "input": bad}),
        )
        .unwrap();
        assert_eq!(checked["phase"], "failed");
        assert_eq!(checked["failure"]["kind"], "checks");
        assert!(checked["failure"]["message"]
            .as_str()
            .unwrap()
            .contains("created"));

        let err = invoke(&w, "get_tool", json!({"toolId": "missing"})).unwrap_err();
        assert_eq!(err["code"], "not_found");
        assert!(invoke(&w, "open_external", json!({"url": "https://evil.example/"})).is_err());
        assert!(
            invoke(&w, "list_activity", json!({}))
                .unwrap()
                .as_array()
                .unwrap()
                .len()
                >= 3
        );
        let show = invoke(&w, "start_show", json!({})).unwrap();
        assert_eq!(show["kind"], "unavailable", "{show}");
        let paused = invoke(&w, "pause_show", json!({"paused": true})).unwrap();
        assert_eq!(paused["kind"], "unavailable");

        let connectors = invoke(&w, "list_connectors", json!({})).unwrap();
        assert_eq!(connectors.as_array().unwrap().len(), 9);
        let gmail = connectors
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "gmail")
            .unwrap();
        assert_eq!(gmail["registration"], "preregistered");
        assert_eq!(gmail["state"]["kind"], "not_connected");
        let saved = invoke(
            &w,
            "set_client_credentials",
            json!({"connectorId": "gmail", "clientId": "1234.apps.googleusercontent.com", "clientSecret": null}),
        )
        .unwrap();
        assert_eq!(saved["hasClientCredentials"], true);
        // The draft above said nothing about a service; a new one mentions Slack.
        invoke(
            &w,
            "draft_task",
            json!({"input": input, "description": "Send it to Slack"}),
        )
        .unwrap();
        let suggestions = invoke(&w, "connection_suggestions", json!({})).unwrap();
        assert_eq!(suggestions[0]["connectorId"], "slack", "{suggestions}");
        assert_eq!(suggestions[0]["signalKind"], "task_text");
        invoke(&w, "dismiss_suggestion", json!({"connectorId": "slack"})).unwrap();
        assert_eq!(
            invoke(&w, "connection_suggestions", json!({})).unwrap(),
            json!([])
        );
        let preview = invoke(&w, "delete_preview", json!({})).unwrap();
        assert_eq!(preview["toolsIncluded"], false);
        assert!(preview["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["count"].as_i64().unwrap() > 0));
        invoke(
            &w,
            "delete_data",
            json!({"options": {"includeTools": false}}),
        )
        .unwrap();
        assert_eq!(
            invoke(&w, "list_tools", json!({}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        invoke(
            &w,
            "delete_data",
            json!({"options": {"includeTools": true}}),
        )
        .unwrap();
        assert_eq!(invoke(&w, "list_tools", json!({})).unwrap(), json!([]));
        assert_eq!(invoke(&w, "list_activity", json!({})).unwrap(), json!([]));
    }

    #[test]
    fn tray_rows_are_limited_to_five_and_label_samples() {
        let tools = (0..7)
            .map(|i| ToolSummary {
                id: format!("tool{i}"),
                name: "Clean import CSV".into(),
                version: 1,
                engine: "csv-mapping".into(),
                created_at_ms: 0,
                last_run_at_ms: None,
                run_count: 0,
                sample: i == 0,
                state: "enabled".into(),
            })
            .collect();
        let rows = tool_menu_rows(tools);
        assert_eq!(rows.len(), 5);
        assert_eq!(
            rows[0],
            ("run:tool0".into(), "Run Clean import CSV (sample)…".into())
        );
        assert_eq!(
            rows[1],
            ("run:tool1".into(), "Run Clean import CSV…".into())
        );
    }

    #[tokio::test]
    async fn event_forwarding_recovers_from_a_lagged_receiver() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(2);
        for n in 0..10 {
            tx.send(n).unwrap();
        }
        assert_eq!(receive_update(&mut rx, || Some(9)).await, Some(9));
    }

    #[test]
    fn every_catalog_documentation_link_can_be_opened() {
        let catalog: Value =
            serde_json::from_str(include_str!("../../../../contracts/connectors.json")).unwrap();
        for c in catalog["connectors"].as_array().unwrap() {
            let docs = c["auth"]["docs_url"].as_str().unwrap();
            assert!(allowed_external(docs), "{docs}");
        }
    }

    #[test]
    fn external_links_are_https_allowlisted_only() {
        assert!(allowed_external(
            "https://developers.google.com/workspace/gmail/api/guides/configure-mcp-server"
        ));
        assert!(allowed_external("https://dev.meta.ai/docs/muse-code"));
        for bad in [
            "http://developers.google.com/",
            "https://evil.example/",
            "https://developers.google.com.evil.example/",
            "https://user@developers.google.com/",
            "https://developers.google.com:8443/",
            "javascript:alert(1)",
            "file:///etc/hosts",
        ] {
            assert!(!allowed_external(bad), "{bad}");
        }
    }
}
