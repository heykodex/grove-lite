//! Mirrors the pure tab model from `grove-core` onto real webviews.
//!
//! Locking rule: the state mutex is never held across a call that talks to a
//! webview or window. Those calls can wait on the main thread, and the page
//! event handlers (which run on the main thread) take the same mutex.

use grove_core::download::unique_path;
use grove_core::{is_allowed_navigation, resolve, Action, CloseOutcome, Entry, History, Kind, Resolved, Snapshot, Tab, Tabs};
use std::str::FromStr;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::webview::{DownloadEvent, NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::window::{Color, WindowBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Url, Webview, WebviewUrl, Window, WindowEvent};

/// Height of the tab strip (36) plus the address bar (40).
const CHROME_H: f64 = 76.0;
/// The charcoal from the logo, also the colour of the blank page.
const INK: Color = Color(49, 49, 49, 255);
/// Keyboard shortcut forwarding, injected into every web page.
const KEYS_JS: &str = include_str!("../../ui/keys.js");

struct Browser {
    tabs: Tabs,
    history: History,
}

pub struct Shared(Mutex<Browser>);

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Browser> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn label_of(tab: &Tab) -> Option<String> {
    match tab.kind {
        Kind::Blank => None,
        Kind::Web => Some(format!("tab-{}", tab.id)),
        Kind::History => Some("history".to_string()),
    }
}

fn tab_id_from_label(label: &str) -> Option<u32> {
    label.strip_prefix("tab-")?.parse().ok()
}

fn main_window(app: &AppHandle) -> Option<Window> {
    app.get_window("main")
}

// ---------------------------------------------------------------------------
// Keeping webviews and the interface in step with the tab list
// ---------------------------------------------------------------------------

fn emit_tabs(app: &AppHandle) {
    let snapshot = app.state::<Shared>().lock().tabs.snapshot();
    let _ = app.emit_to("chrome", "tabs", snapshot);
}

fn focus_url(app: &AppHandle) {
    if let Some(chrome) = app.get_webview("chrome") {
        let _ = chrome.set_focus();
    }
    let _ = app.emit_to("chrome", "focus-url", ());
}

fn page_size(window: &Window) -> LogicalSize<f64> {
    let physical = window.inner_size().unwrap_or_default();
    let scale = window.scale_factor().unwrap_or(1.0);
    let size: LogicalSize<f64> = physical.to_logical(scale);
    LogicalSize::new(size.width.max(1.0), (size.height - CHROME_H).max(1.0))
}

/// Put the chrome webview on top and every page webview below it.
fn layout(window: &Window) {
    let page = page_size(window);
    for webview in window.webviews() {
        if webview.label() == "chrome" {
            let _ = webview.set_position(LogicalPosition::new(0.0, 0.0));
            let _ = webview.set_size(LogicalSize::new(page.width, CHROME_H));
        } else {
            let _ = webview.set_position(LogicalPosition::new(0.0, CHROME_H));
            let _ = webview.set_size(page);
        }
    }
}

/// Show only the active tab's webview, give it focus, and tell the interface.
fn sync_views(app: &AppHandle) {
    let Some(window) = main_window(app) else { return };
    let active = label_of(app.state::<Shared>().lock().tabs.active());

    for webview in window.webviews() {
        if webview.label() == "chrome" {
            continue;
        }
        if Some(webview.label()) == active.as_deref() {
            let _ = webview.show();
        } else {
            let _ = webview.hide();
        }
    }
    layout(&window);

    match active.and_then(|label| app.get_webview(&label)) {
        Some(webview) => {
            let _ = webview.set_focus();
        }
        None => focus_url(app),
    }
    emit_tabs(app);
}

/// On Linux, Tauri packs every child webview into one vertical GTK box and
/// ignores their positions and sizes, so by default the interface strip and
/// the page would split the window half and half. Pin the strip to its real
/// height; the page then gets whatever is left, and hidden pages take no space.
#[cfg(target_os = "linux")]
fn pin_chrome_height(app: &AppHandle) {
    use gtk::prelude::*;
    if let Some(chrome) = app.get_webview("chrome") {
        let _ = chrome.with_webview(|platform| {
            let widget: gtk::Widget = platform.inner().upcast();
            widget.set_size_request(-1, CHROME_H as i32);
            if let Some(parent) = widget.parent().and_then(|p| p.downcast::<gtk::Box>().ok()) {
                parent.set_child_packing(&widget, false, true, 0, gtk::PackType::Start);
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Creating webviews
// ---------------------------------------------------------------------------

fn create_web_view(app: &AppHandle, id: u32, url: Url) -> tauri::Result<()> {
    let window = main_window(app).ok_or(tauri::Error::WindowNotFound)?;
    let size = page_size(&window);

    let for_window = app.clone();
    let for_title = app.clone();
    let for_load = app.clone();
    let for_download = app.clone();

    let builder = WebviewBuilder::new(format!("tab-{id}"), WebviewUrl::External(url))
        .initialization_script(KEYS_JS)
        .on_navigation(is_allowed_navigation)
        .on_new_window(move |url, _features| {
            // target=_blank and window.open become a new tab.
            let app = for_window.clone();
            tauri::async_runtime::spawn(async move { open_in_new_tab(&app, url) });
            NewWindowResponse::Deny
        })
        .on_document_title_changed(move |webview, title| on_title(&for_title, &webview, title))
        .on_page_load(move |webview, payload| on_load(&for_load, &webview, payload.event(), payload.url()))
        .on_download(move |webview, event| on_download(&for_download, &webview, event));

    window.add_child(builder, LogicalPosition::new(0.0, CHROME_H), size)?;
    Ok(())
}

fn create_history_view(app: &AppHandle) -> tauri::Result<()> {
    let window = main_window(app).ok_or(tauri::Error::WindowNotFound)?;
    let size = page_size(&window);
    let builder = WebviewBuilder::new("history", WebviewUrl::App("history.html".into())).background_color(INK);
    window.add_child(builder, LogicalPosition::new(0.0, CHROME_H), size)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Page events
// ---------------------------------------------------------------------------

fn on_load(app: &AppHandle, webview: &Webview, event: PageLoadEvent, url: &Url) {
    let Some(id) = tab_id_from_label(webview.label()) else { return };
    {
        let state = app.state::<Shared>();
        let mut b = state.lock();
        let Browser { tabs, history } = &mut *b;
        let Some(tab) = tabs.get_mut(id) else { return };
        if tab.kind != Kind::Web {
            return;
        }
        match event {
            PageLoadEvent::Started => {
                tab.url = url.to_string();
                tab.loading = true;
            }
            PageLoadEvent::Finished => {
                tab.url = url.to_string();
                tab.loading = false;
                let _ = history.record_visit(url.as_str(), tab.title.trim(), now());
            }
        }
    }
    emit_tabs(app);
}

fn on_title(app: &AppHandle, webview: &Webview, title: String) {
    let Some(id) = tab_id_from_label(webview.label()) else { return };
    // Single-page apps change the address without a page load; the title
    // change that usually accompanies it is our cue to re-read it.
    let current = webview.url().ok().filter(|u| matches!(u.scheme(), "http" | "https"));
    {
        let state = app.state::<Shared>();
        let mut b = state.lock();
        let Browser { tabs, history } = &mut *b;
        let Some(tab) = tabs.get_mut(id) else { return };
        if tab.kind != Kind::Web {
            return;
        }
        tab.title = title.clone();
        if let Some(url) = current {
            tab.url = url.to_string();
        }
        let _ = history.set_title(&tab.url, title.trim());
    }
    emit_tabs(app);
}

fn on_download(app: &AppHandle, webview: &Webview, event: DownloadEvent<'_>) -> bool {
    let id = tab_id_from_label(webview.label());
    match event {
        DownloadEvent::Requested { destination, .. } => {
            let name = destination.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let dir = app.path().download_dir().ok().or_else(|| destination.parent().map(|p| p.to_path_buf()));
            if let Some(dir) = dir {
                *destination = unique_path(&dir, &name);
            }
        }
        DownloadEvent::Finished { .. } => {
            // A tab still "loading" when a download finishes was navigated to
            // the download itself, so no page will ever show in it.
            let loading = id.is_some_and(|id| app.state::<Shared>().lock().tabs.get(id).is_some_and(|t| t.loading));
            if let (true, Some(id)) = (loading, id) {
                let app = app.clone();
                tauri::async_runtime::spawn(async move { retire_download_tab(&app, id) });
            }
        }
        _ => {}
    }
    true
}

/// A tab that only started a download has nothing to show: close it, or if it
/// is the last tab turn it back into a blank one.
fn retire_download_tab(app: &AppHandle, id: u32) {
    let only_tab = app.state::<Shared>().lock().tabs.len() == 1;
    if !only_tab {
        do_close_tab(app, id);
        return;
    }
    if let Some(webview) = app.get_webview(&format!("tab-{id}")) {
        let _ = webview.close();
    }
    if let Some(tab) = app.state::<Shared>().lock().tabs.get_mut(id) {
        tab.kind = Kind::Blank;
        tab.url.clear();
        tab.title.clear();
        tab.loading = false;
    }
    sync_views(app);
}

// ---------------------------------------------------------------------------
// Tab operations
// ---------------------------------------------------------------------------

fn open_in_new_tab(app: &AppHandle, url: Url) {
    if !matches!(url.scheme(), "http" | "https") {
        return;
    }
    let id = {
        let state = app.state::<Shared>();
        let mut b = state.lock();
        let id = b.tabs.open_blank();
        if let Some(tab) = b.tabs.get_mut(id) {
            tab.kind = Kind::Web;
            tab.url = url.to_string();
            tab.loading = true;
        }
        id
    };
    if create_web_view(app, id, url).is_err() {
        do_close_tab(app, id);
        return;
    }
    sync_views(app);
}

fn do_new_tab(app: &AppHandle) {
    app.state::<Shared>().lock().tabs.open_blank();
    sync_views(app);
}

fn do_close_tab(app: &AppHandle, id: u32) {
    let (label, outcome) = {
        let state = app.state::<Shared>();
        let mut b = state.lock();
        let label = b.tabs.get(id).and_then(label_of);
        (label, b.tabs.close(id))
    };
    if let Some(webview) = label.and_then(|l| app.get_webview(&l)) {
        let _ = webview.close();
    }
    match outcome {
        CloseOutcome::Empty => app.exit(0),
        CloseOutcome::NotFound => {}
        CloseOutcome::Closed { .. } => sync_views(app),
    }
}

fn do_switch_tab(app: &AppHandle, id: u32) {
    if app.state::<Shared>().lock().tabs.activate(id) {
        sync_views(app);
    }
}

/// Load `url` in tab `id`, creating its webview if it has none yet.
fn go(app: &AppHandle, id: u32, url: Url) {
    enum Plan {
        Create,
        Navigate(String),
        ReplaceHistory,
    }
    let plan = {
        let state = app.state::<Shared>();
        let mut b = state.lock();
        let Some(tab) = b.tabs.get_mut(id) else { return };
        let plan = match tab.kind {
            Kind::Blank => Plan::Create,
            Kind::Web => Plan::Navigate(format!("tab-{id}")),
            Kind::History => Plan::ReplaceHistory,
        };
        tab.kind = Kind::Web;
        tab.url = url.to_string();
        tab.title.clear();
        tab.loading = true;
        plan
    };

    let ok = match plan {
        Plan::Create => create_web_view(app, id, url).is_ok(),
        Plan::Navigate(label) => match app.get_webview(&label) {
            Some(webview) => webview.navigate(url).is_ok(),
            None => create_web_view(app, id, url).is_ok(),
        },
        Plan::ReplaceHistory => {
            if let Some(webview) = app.get_webview("history") {
                let _ = webview.close();
            }
            create_web_view(app, id, url).is_ok()
        }
    };
    if !ok {
        if let Some(tab) = app.state::<Shared>().lock().tabs.get_mut(id) {
            tab.loading = false;
        }
    }
    sync_views(app);
}

fn open_history(app: &AppHandle) {
    let create = {
        let state = app.state::<Shared>();
        let mut b = state.lock();
        if let Some(id) = b.tabs.history_tab() {
            b.tabs.activate(id);
            false
        } else {
            // Reuse an empty tab rather than leaving it behind.
            let id = if b.tabs.active().kind == Kind::Blank { b.tabs.active_id() } else { b.tabs.open_blank() };
            if let Some(tab) = b.tabs.get_mut(id) {
                tab.kind = Kind::History;
            }
            true
        }
    };
    if create && create_history_view(app).is_err() {
        let id = app.state::<Shared>().lock().tabs.history_tab();
        if let Some(id) = id {
            do_close_tab(app, id);
        }
        return;
    }
    sync_views(app);
}

fn eval_in_active(app: &AppHandle, js: &str) {
    let label = label_of(app.state::<Shared>().lock().tabs.active());
    if let Some(webview) = label.and_then(|l| app.get_webview(&l)) {
        let _ = webview.eval(js);
    }
}

fn perform(app: &AppHandle, action: Action) {
    match action {
        Action::NewTab => do_new_tab(app),
        Action::CloseTab => {
            let id = app.state::<Shared>().lock().tabs.active_id();
            do_close_tab(app, id);
        }
        Action::NextTab => {
            app.state::<Shared>().lock().tabs.next_tab();
            sync_views(app);
        }
        Action::PrevTab => {
            app.state::<Shared>().lock().tabs.prev_tab();
            sync_views(app);
        }
        Action::Select(n) => {
            if app.state::<Shared>().lock().tabs.select(n).is_some() {
                sync_views(app);
            }
        }
        Action::SelectLast => {
            app.state::<Shared>().lock().tabs.select_last();
            sync_views(app);
        }
        Action::FocusUrl => focus_url(app),
        Action::History => open_history(app),
        Action::Reload => {
            let label = label_of(app.state::<Shared>().lock().tabs.active());
            if let Some(webview) = label.and_then(|l| app.get_webview(&l)) {
                let _ = webview.reload();
            }
        }
        Action::Back => eval_in_active(app, "history.back()"),
        Action::Forward => eval_in_active(app, "history.forward()"),
    }
}

// ---------------------------------------------------------------------------
// Commands. All async: they must not run on the main thread, because creating
// a webview waits for the main thread.
// ---------------------------------------------------------------------------

fn only(webview: &Webview, label: &str) -> Result<(), String> {
    if webview.label() == label {
        Ok(())
    } else {
        Err("not allowed".into())
    }
}

#[tauri::command]
async fn get_state(webview: Webview, state: State<'_, Shared>) -> Result<Snapshot, String> {
    only(&webview, "chrome")?;
    Ok(state.lock().tabs.snapshot())
}

#[tauri::command]
async fn chrome_ready(app: AppHandle, webview: Webview) -> Result<(), String> {
    only(&webview, "chrome")?;
    sync_views(&app);
    Ok(())
}

#[tauri::command]
async fn navigate(app: AppHandle, webview: Webview, input: String) -> Result<(), String> {
    only(&webview, "chrome")?;
    if let Resolved::Navigate(url) = resolve(&input) {
        let id = app.state::<Shared>().lock().tabs.active_id();
        go(&app, id, url);
    } else {
        sync_views(&app);
    }
    Ok(())
}

#[tauri::command]
async fn new_tab(app: AppHandle, webview: Webview) -> Result<(), String> {
    only(&webview, "chrome")?;
    do_new_tab(&app);
    Ok(())
}

#[tauri::command]
async fn close_tab(app: AppHandle, webview: Webview, id: u32) -> Result<(), String> {
    only(&webview, "chrome")?;
    do_close_tab(&app, id);
    Ok(())
}

#[tauri::command]
async fn switch_tab(app: AppHandle, webview: Webview, id: u32) -> Result<(), String> {
    only(&webview, "chrome")?;
    do_switch_tab(&app, id);
    Ok(())
}

#[tauri::command]
async fn focus_page(app: AppHandle, webview: Webview) -> Result<(), String> {
    only(&webview, "chrome")?;
    sync_views(&app);
    Ok(())
}

#[tauri::command]
async fn window_action(app: AppHandle, webview: Webview, action: String) -> Result<(), String> {
    only(&webview, "chrome")?;
    let window = main_window(&app).ok_or("no window")?;
    match action.as_str() {
        "minimize" => window.minimize().map_err(|e| e.to_string())?,
        "maximize" => {
            if window.is_maximized().unwrap_or(false) {
                window.unmaximize().map_err(|e| e.to_string())?
            } else {
                window.maximize().map_err(|e| e.to_string())?
            }
        }
        "close" => app.exit(0),
        _ => return Err("unknown action".into()),
    }
    Ok(())
}

/// Called by `ui/keys.js`, which runs inside web pages. Pages are untrusted,
/// so only a fixed list of action names is accepted, and only from the
/// interface pages or the tab that is currently showing.
#[tauri::command]
async fn shortcut(app: AppHandle, webview: Webview, action: String) -> Result<(), String> {
    let label = webview.label();
    let allowed = label == "chrome"
        || label == "history"
        || tab_id_from_label(label).is_some_and(|id| id == app.state::<Shared>().lock().tabs.active_id());
    if !allowed {
        return Err("not allowed".into());
    }
    let action = Action::from_str(&action).map_err(|_| "unknown action".to_string())?;
    perform(&app, action);
    Ok(())
}

#[tauri::command]
async fn history_list(webview: Webview, state: State<'_, Shared>, query: String) -> Result<Vec<Entry>, String> {
    only(&webview, "history")?;
    Ok(state.lock().history.search(&query, 500))
}

#[tauri::command]
async fn history_open(app: AppHandle, webview: Webview, url: String) -> Result<(), String> {
    only(&webview, "history")?;
    let url = Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(url.scheme(), "http" | "https") || !is_allowed_navigation(&url) {
        return Err("not allowed".into());
    }
    let id = app.state::<Shared>().lock().tabs.history_tab();
    if let Some(id) = id {
        go(&app, id, url);
    }
    Ok(())
}

#[tauri::command]
async fn history_clear(webview: Webview, state: State<'_, Shared>) -> Result<(), String> {
    only(&webview, "history")?;
    state.lock().history.clear().map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Start up
// ---------------------------------------------------------------------------

fn build_window(app: &AppHandle) -> tauri::Result<()> {
    let builder =
        WindowBuilder::new(app, "main").title("Grove Lite").inner_size(1100.0, 760.0).min_inner_size(480.0, 320.0).background_color(INK);

    // macOS keeps its traffic lights, floating over the tab strip. Everywhere
    // else the tab strip is the title bar and carries its own tiny controls.
    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);

    let window = builder.build()?;

    let size = page_size(&window);
    window.add_child(
        WebviewBuilder::new("chrome", WebviewUrl::App("index.html".into())).background_color(INK),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(size.width, CHROME_H),
    )?;
    #[cfg(target_os = "linux")]
    pin_chrome_height(app);

    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Resized(_) = event {
            if let Some(window) = main_window(&handle) {
                layout(&window);
            }
        }
    });
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let history = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|dir| History::open(dir.join("history.jsonl")).ok())
                .unwrap_or_else(History::in_memory);
            app.manage(Shared(Mutex::new(Browser { tabs: Tabs::new(), history })));
            build_window(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            chrome_ready,
            navigate,
            new_tab,
            close_tab,
            switch_tab,
            focus_page,
            window_action,
            shortcut,
            history_list,
            history_open,
            history_clear,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Grove Lite");
}
