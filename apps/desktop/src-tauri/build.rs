fn main() {
    // Every command is enrolled in the app manifest and granted to the main window only
    // (capabilities/main.json). Keep this list identical to generate_handler! in main.rs.
    const COMMANDS: &[&str] = &[
        "app_info",
        "model_access",
        "enter_api_key_native",
        "save_api_key",
        "remove_api_key",
        "test_api_key",
        "refresh_muse_status",
        "open_muse_sign_in",
        "select_route",
        "set_muse_unverified_ok",
        "set_local_only",
        "pick_csv",
        "load_sample_csv",
        "draft_task",
        "answer_question",
        "update_mapping",
        "outbound_preview",
        "start_build",
        "build_state",
        "stop_build",
        "save_tool",
        "list_tools",
        "get_tool",
        "install_sample_tool",
        "delete_tool",
        "pick_output_dir",
        "stage_run",
        "commit_run",
        "discard_run",
        "reveal_output",
        "list_activity",
        "list_receipts",
        "privacy_overview",
        "delete_outbound_history",
        "emergency_stop",
        "open_external",
        "show_state",
        "start_show",
        "pause_show",
        "stop_show",
    ];
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("Tauri build configuration must be valid");
}
