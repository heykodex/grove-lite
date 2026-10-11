fn main() {
    // Only these commands exist. Each gets an `allow-<name>` permission, and
    // the capability files decide which webview may call which.
    let commands = &[
        "get_state",
        "chrome_ready",
        "navigate",
        "new_tab",
        "close_tab",
        "switch_tab",
        "focus_page",
        "window_action",
        "shortcut",
        "history_list",
        "history_open",
        "history_clear",
    ];
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(commands)))
        .expect("failed to run tauri-build");
}
