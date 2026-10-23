//! GlassTranslate Lite - backend.
//!
//! Pipeline: global shortcut -> screen capture (in memory) -> selection overlay -> crop ->
//! Windows OCR -> CTranslate2 offline EN->AR -> floating popup. Nothing is written to disk
//! except the small settings file, and nothing leaves the machine.

mod capture;
mod history;
mod models;
mod ocr;
mod settings;
mod translate;

use std::borrow::Cow;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, Theme, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use settings::Settings;

const POPUP_W: f64 = 432.0; // logical, includes the 12px transparent margin on each side
const POPUP_W_WIDE: f64 = 640.0;
const POPUP_MARGIN: f64 = 12.0;
const MAX_CHARS: usize = 4000;

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

enum EngineSlot {
    /// Lazy-loaded: nothing is read into RAM until a translation in this direction is needed.
    NotLoaded,
    Loading,
    Ready(Arc<translate::Engine>),
    Failed(String),
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct PopupState {
    /// idle | loading | done | error
    status: String,
    /// ocr | clipboard | text
    source: String,
    original: String,
    translation: String,
    error_kind: String,
    error_detail: String,
    truncated: bool,
    gen: u64,
    session: u64,
    /// Translation direction of this popup ("en-ar" | "he-ar"); Translate Again reuses it.
    direction: String,
    source_lang: String,
    target_lang: String,
    /// History entry for this translation (0 = not stored, e.g. history is off).
    entry_id: u64,
    favorite: bool,
}

#[derive(Clone, Copy)]
enum Anchor {
    /// Selection rectangle in physical screen pixels.
    Rect(i32, i32, i32, i32),
    /// Centre the popup on the monitor that holds this point.
    Center(i32, i32),
}

/// One translation engine (model) per direction.
struct Slot {
    engine: Mutex<EngineSlot>,
    model_dir: Mutex<Option<PathBuf>>,
    loading: AtomicBool,
}

impl Slot {
    fn new() -> Self {
        Slot { engine: Mutex::new(EngineSlot::NotLoaded), model_dir: Mutex::new(None), loading: AtomicBool::new(false) }
    }
}

struct AppState {
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    /// Indexed like `models::SPECS` (0 = en-ar, 1 = he-ar).
    slots: [Slot; 2],

    shot: Mutex<Option<capture::Shot>>,
    busy: AtomicBool,
    cap_gen: AtomicU64,
    selector_shown: AtomicBool,
    selector_focused: AtomicBool,
    selector_shown_at: Mutex<Option<Instant>>,
    restore_main: AtomicBool,

    popup: Mutex<PopupState>,
    anchor: Mutex<Anchor>,
    last_placed: Mutex<Option<(i32, i32)>>,
    popup_gen: AtomicU64,
    session: AtomicU64,
    wide: AtomicBool,
    popup_h: Mutex<f64>,
    /// Debounce for persisting a dragged popup position.
    move_pending: AtomicBool,
    last_move: Mutex<Option<Instant>>,

    shortcut_errors: Mutex<Vec<String>>,

    history: Mutex<Vec<history::Entry>>,
    history_path: PathBuf,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn cap_text(s: &str) -> (String, bool) {
    let s = s.trim();
    if s.chars().count() > MAX_CHARS {
        (s.chars().take(MAX_CHARS).collect(), true)
    } else {
        (s.to_string(), false)
    }
}

// ---------------------------------------------------------------------------------------------
// Engine loading
// ---------------------------------------------------------------------------------------------

/// Normalise a direction coming from the frontend or from disk.
fn norm_dir(d: &str) -> &'static str {
    if d == "he-ar" {
        "he-ar"
    } else {
        "en-ar"
    }
}

fn slot_of<'a>(s: &'a AppState, dir: &str) -> &'a Slot {
    &s.slots[models::SPECS.iter().position(|m| m.id == norm_dir(dir)).unwrap_or(0)]
}

/// (source language, target language) codes stored in history and shown in the popup.
fn langs_of(dir: &str) -> (&'static str, &'static str) {
    if norm_dir(dir) == "he-ar" {
        ("he", "ar")
    } else {
        ("en", "ar")
    }
}

fn dir_from_langs(source: &str) -> &'static str {
    if source == "he" {
        "he-ar"
    } else {
        "en-ar"
    }
}

fn user_models_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("models"))
}

fn find_model_dir(app: &AppHandle, dir: &str) -> Result<PathBuf, String> {
    let spec = models::spec(norm_dir(dir)).ok_or("Unknown translation direction")?;
    let user = user_models_dir(app);
    let res = app.path().resource_dir().ok();
    models::locate(spec, user.as_deref(), res.as_deref()).map(|(p, _)| p)
}

fn start_engine_load(app: &AppHandle, dir: &str) {
    let dir = norm_dir(dir);
    let s = app.state::<AppState>();
    let slot = slot_of(&s, dir);
    if slot.loading.swap(true, SeqCst) {
        return;
    }
    *lock(&slot.engine) = EngineSlot::Loading;
    let _ = app.emit("engine-status", ());
    let app = app.clone();
    std::thread::spawn(move || {
        let found = find_model_dir(&app, dir);
        let result = match found.clone() {
            Err(m) => Err(m),
            Ok(d) => catch_unwind(AssertUnwindSafe(|| translate::load(&d)))
                .unwrap_or_else(|_| Err("The translation engine crashed while loading.".to_string())),
        };
        let s = app.state::<AppState>();
        let slot = slot_of(&s, dir);
        *lock(&slot.model_dir) = found.ok();
        *lock(&slot.engine) = match result {
            Ok(e) => EngineSlot::Ready(Arc::new(e)),
            Err(m) => EngineSlot::Failed(m),
        };
        slot.loading.store(false, SeqCst);
        let _ = app.emit("engine-status", ());
    });
}

type EngineErr = (&'static str, String);

/// Returns the engine for a direction, loading it on first use (lazy) and waiting if `wait`.
fn get_engine(app: &AppHandle, dir: &str, wait: bool) -> Result<Arc<translate::Engine>, EngineErr> {
    let s = app.state::<AppState>();
    let slot = slot_of(&s, dir);
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut kicked = false;
    loop {
        let need_start = {
            match &*lock(&slot.engine) {
                EngineSlot::Ready(e) => return Ok(e.clone()),
                EngineSlot::Failed(m) => return Err(("engine_not_ready", m.clone())),
                EngineSlot::NotLoaded => true,
                EngineSlot::Loading => false,
            }
        };
        if need_start && !kicked {
            kicked = true;
            start_engine_load(app, dir);
        }
        if !wait || Instant::now() > deadline {
            return Err(("engine_loading", String::new()));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

// ---------------------------------------------------------------------------------------------
// Popup
// ---------------------------------------------------------------------------------------------

fn new_job(app: &AppHandle) -> u64 {
    app.state::<AppState>().popup_gen.fetch_add(1, SeqCst) + 1
}

/// Update the popup state if `job` is still the current job. Returns false for stale jobs.
fn update_popup(app: &AppHandle, job: u64, f: impl FnOnce(&mut PopupState)) -> bool {
    let s = app.state::<AppState>();
    let snap = {
        let mut p = lock(&s.popup);
        if s.popup_gen.load(SeqCst) != job {
            return false;
        }
        f(&mut p);
        p.gen = job;
        p.clone()
    };
    let _ = app.emit("popup-update", &snap);
    true
}

fn hide_popup_inner(app: &AppHandle) {
    let s = app.state::<AppState>();
    s.popup_gen.fetch_add(1, SeqCst);
    s.wide.store(false, SeqCst);
    let snap = {
        let mut p = lock(&s.popup);
        *p = PopupState { status: "idle".into(), session: p.session, ..Default::default() };
        p.clone()
    };
    let _ = app.emit("popup-update", &snap);
    if let Some(w) = app.get_webview_window("popup") {
        let _ = w.hide();
    }
}

fn popup_monitor(app: &AppHandle, x: i32, y: i32) -> Option<tauri::Monitor> {
    app.monitor_from_point(x as f64, y as f64)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
}

fn popup_width(s: &AppState) -> f64 {
    if s.wide.load(SeqCst) {
        POPUP_W_WIDE
    } else {
        POPUP_W
    }
}

/// Size and place the popup below (or above) the selection and keep it inside the work area.
fn place_popup(app: &AppHandle) {
    let Some(w) = app.get_webview_window("popup") else { return };
    let s = app.state::<AppState>();
    let anchor = *lock(&s.anchor);
    let saved = {
        let st = lock(&s.settings);
        if st.popup_remember_position { st.popup_pos } else { None }
    };
    let (ax, ay) = match (saved, anchor) {
        (Some((sx, sy)), _) => (sx + 60, sy + 60),
        (None, Anchor::Rect(x, y, rw, rh)) => (x + rw / 2, y + rh / 2),
        (None, Anchor::Center(x, y)) => (x, y),
    };
    let Some(mon) = popup_monitor(app, ax, ay) else { return };
    let sc = mon.scale_factor();
    let wa = mon.work_area();
    let (wx, wy) = (wa.position.x, wa.position.y);
    let (ww, wh) = (wa.size.width as i32, wa.size.height as i32);
    let m = (POPUP_MARGIN * sc).round() as i32;
    let gap = (8.0 * sc).round() as i32;
    let pw = ((popup_width(&s) * sc).round() as i32).min(ww);
    let ph = ((*lock(&s.popup_h) * sc).round() as i32).clamp((90.0 * sc) as i32, (wh - 4).max(100));

    let (x, y) = match anchor {
        Anchor::Rect(rx, ry, _rw, rh) => {
            let x = rx - m;
            let mut y = ry + rh + gap - m;
            if y + ph - m > wy + wh - gap {
                let above = ry - gap - ph + m;
                y = if above >= wy - m { above } else { wy + wh - ph + m - gap };
            }
            (x, y)
        }
        Anchor::Center(..) => (wx + (ww - pw) / 2, wy + (wh - ph) / 3),
    };
    let (x, y) = saved.unwrap_or((x, y));
    let x = x.clamp(wx - m, (wx + ww - pw + m).max(wx - m));
    let y = y.clamp(wy - m, (wy + wh - ph + m).max(wy - m));

    // Position, size, position again: a DPI change between monitors can rescale the window.
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.set_size(PhysicalSize::new(pw as u32, ph as u32));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    *lock(&s.last_placed) = Some((x, y));
}

/// Keep a user-dragged popup inside the monitor after it changed size.
fn clamp_popup(app: &AppHandle) {
    let Some(w) = app.get_webview_window("popup") else { return };
    let s = app.state::<AppState>();
    let Ok(pos) = w.outer_position() else { return };
    let Ok(size) = w.outer_size() else { return };
    let Some(mon) = popup_monitor(app, pos.x + size.width as i32 / 2, pos.y + size.height as i32 / 2) else {
        return;
    };
    let sc = mon.scale_factor();
    let wa = mon.work_area();
    let (wx, wy) = (wa.position.x, wa.position.y);
    let (ww, wh) = (wa.size.width as i32, wa.size.height as i32);
    let m = (POPUP_MARGIN * sc).round() as i32;
    let pw = ((popup_width(&s) * sc).round() as i32).min(ww);
    let ph = ((*lock(&s.popup_h) * sc).round() as i32).clamp((90.0 * sc) as i32, (wh - 4).max(100));
    let x = pos.x.clamp(wx - m, (wx + ww - pw + m).max(wx - m));
    let y = pos.y.clamp(wy - m, (wy + wh - ph + m).max(wy - m));
    let _ = w.set_size(PhysicalSize::new(pw as u32, ph as u32));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    // `last_placed` deliberately stays at the app's own placement: it is the reference that tells a
    // user-dragged popup apart from an untouched one, so a later relayout keeps honouring the drag.
}

fn popup_was_dragged(app: &AppHandle) -> bool {
    let s = app.state::<AppState>();
    let Some(w) = app.get_webview_window("popup") else { return false };
    let (Ok(pos), Some((lx, ly))) = (w.outer_position(), *lock(&s.last_placed)) else { return false };
    (pos.x - lx).abs() > 2 || (pos.y - ly).abs() > 2
}

fn relayout_popup(app: &AppHandle) {
    let visible = app
        .get_webview_window("popup")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if !visible {
        return;
    }
    if popup_was_dragged(app) {
        clamp_popup(app);
    } else {
        place_popup(app);
    }
}

/// Start a fresh popup session and show the window.
fn open_popup(app: &AppHandle, job: u64, anchor: Anchor, init: PopupState) {
    let s = app.state::<AppState>();
    let session = s.session.fetch_add(1, SeqCst) + 1;
    s.wide.store(false, SeqCst);
    *lock(&s.anchor) = anchor;
    let ok = update_popup(app, job, |p| {
        *p = init;
        p.session = session;
    });
    if !ok {
        return;
    }
    place_popup(app);
    if let Some(w) = app.get_webview_window("popup") {
        let _ = w.set_always_on_top(lock(&s.settings).popup_pinned);
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Direction currently selected in the app (what the shortcuts translate with).
fn cur_dir(app: &AppHandle) -> &'static str {
    norm_dir(&lock(&app.state::<AppState>().settings).direction)
}

/// A popup state with the language pair of `dir` already filled in.
fn popup_for(dir: &str, status: &str, source: &str) -> PopupState {
    let dir = norm_dir(dir);
    let (sl, tl) = langs_of(dir);
    PopupState {
        status: status.into(),
        source: source.into(),
        direction: dir.into(),
        source_lang: sl.into(),
        target_lang: tl.into(),
        ..Default::default()
    }
}

fn error_state(kind: &str, detail: &str, source: &str, dir: &str) -> PopupState {
    PopupState { error_kind: kind.into(), error_detail: detail.into(), ..popup_for(dir, "error", source) }
}

fn center_anchor() -> Anchor {
    let (x, y) = capture::cursor_pos();
    Anchor::Center(x, y)
}

fn show_error(app: &AppHandle, anchor: Anchor, kind: &str, detail: &str, source: &str) {
    let job = new_job(app);
    open_popup(app, job, anchor, error_state(kind, detail, source, cur_dir(app)));
}

/// Store a finished translation in the local history, but only if the user enabled it.
/// Returns (entry id, favorite); (0, false) when nothing was stored.
fn record_history(app: &AppHandle, original: &str, translation: &str, source: &str, dir: &str) -> (u64, bool) {
    let s = app.state::<AppState>();
    if !lock(&s.settings).history_enabled || original.trim().is_empty() || translation.trim().is_empty() {
        return (0, false);
    }
    let src = match source {
        "ocr" => "screen",
        "clipboard" => "clipboard",
        _ => "manual",
    };
    let (res, snapshot) = {
        let mut h = lock(&s.history);
        let (sl, tl) = langs_of(dir);
        let meta = history::Meta { source_lang: sl, target_lang: tl, method: "direct" };
        let r = history::record(&mut h, original.trim(), translation, src, meta);
        (r, h.clone())
    };
    let _ = history::save(&s.history_path, &snapshot);
    let _ = app.emit("history-changed", ());
    res
}

/// Translate on the calling (worker) thread and publish the result to the popup.
fn translate_job(app: &AppHandle, job: u64, text: &str) {
    let dir = norm_dir(&lock(&app.state::<AppState>().popup).direction);
    let res = catch_unwind(AssertUnwindSafe(|| -> Result<String, EngineErr> {
        let eng = get_engine(app, dir, true)?;
        eng.translate(text).map_err(|e| ("translate_failed", e))
    }));
    match res {
        Ok(Ok(t)) => {
            let source = lock(&app.state::<AppState>().popup).source.clone();
            let (entry_id, favorite) = record_history(app, text, &t, &source, dir);
            update_popup(app, job, |p| {
                p.status = "done".into();
                p.translation = t;
                p.error_kind.clear();
                p.error_detail.clear();
                p.entry_id = entry_id;
                p.favorite = favorite;
            });
        }
        Ok(Err((k, d))) => {
            update_popup(app, job, |p| {
                p.status = "error".into();
                p.error_kind = k.into();
                p.error_detail = d;
            });
        }
        Err(_) => {
            update_popup(app, job, |p| {
                p.status = "error".into();
                p.error_kind = "translate_failed".into();
                p.error_detail = "internal error".into();
            });
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Main window / tray
// ---------------------------------------------------------------------------------------------

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn open_settings(app: &AppHandle) {
    show_main(app);
    let _ = app.emit("navigate", "settings");
}

fn tray_labels(lang: &str) -> [&'static str; 5] {
    if lang == "ar" {
        ["فتح GlassTranslate", "تحديد منطقة للترجمة", "ترجمة الحافظة", "الإعدادات", "إنهاء"]
    } else {
        ["Open GlassTranslate", "Select Area to Translate", "Translate Clipboard", "Settings", "Quit"]
    }
}

fn build_tray_menu(app: &AppHandle, lang: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let l = tray_labels(lang);
    let open = MenuItem::with_id(app, "open", l[0], true, None::<&str>)?;
    let select = MenuItem::with_id(app, "select", l[1], true, None::<&str>)?;
    let clip = MenuItem::with_id(app, "clipboard", l[2], true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", l[3], true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", l[4], true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&open, &sep1, &select, &clip, &sep2, &settings, &quit])
}

fn quit_app(app: &AppHandle) {
    let s = app.state::<AppState>();
    if let Some(sh) = lock(&s.shot).as_mut() {
        sh.wipe();
    }
    *lock(&s.shot) = None;
    let _ = app.remove_tray_by_id("main");
    app.exit(0);
}

fn setup_tray(app: &AppHandle, lang: &str) -> tauri::Result<()> {
    let menu = build_tray_menu(app, lang)?;
    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("GlassTranslate Lite")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "select" => begin_capture(app.clone()),
            "clipboard" => clipboard_translate(app.clone()),
            "settings" => open_settings(app),
            "quit" => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn apply_theme(app: &AppHandle, theme: &str) {
    let t = match theme {
        "dark" => Some(Theme::Dark),
        "light" => Some(Theme::Light),
        _ => None,
    };
    for l in ["main", "selector", "popup"] {
        if let Some(w) = app.get_webview_window(l) {
            let _ = w.set_theme(t);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Capture flow
// ---------------------------------------------------------------------------------------------

/// Release everything held for a capture except the busy flag.
fn teardown_selector(app: &AppHandle) {
    let s = app.state::<AppState>();
    s.cap_gen.fetch_add(1, SeqCst);
    s.selector_shown.store(false, SeqCst);
    s.selector_focused.store(false, SeqCst);
    if let Some(sh) = lock(&s.shot).as_mut() {
        sh.wipe();
    }
    *lock(&s.shot) = None;
    if let Some(w) = app.get_webview_window("selector") {
        let _ = w.hide();
    }
    let _ = app.emit_to("selector", "selector-clear", ());
}

fn release_capture(app: &AppHandle, restore_main: bool) {
    teardown_selector(app);
    let s = app.state::<AppState>();
    if s.restore_main.swap(false, SeqCst) && restore_main {
        show_main(app);
    }
    s.busy.store(false, SeqCst);
}

/// Abort the capture identified by `my`. Does nothing if a newer capture owns the state.
fn abort_capture(app: &AppHandle, my: u64, err: Option<(&str, String)>) {
    let s = app.state::<AppState>();
    if s.cap_gen.load(SeqCst) != my {
        return;
    }
    release_capture(app, true);
    if let Some((kind, detail)) = err {
        show_error(app, center_anchor(), kind, &detail, "ocr");
    }
}

fn begin_capture(app: AppHandle) {
    let s = app.state::<AppState>();
    if s.busy.compare_exchange(false, true, SeqCst, SeqCst).is_err() {
        return; // a capture / clipboard job is already running
    }
    let my = s.cap_gen.fetch_add(1, SeqCst) + 1;
    s.selector_shown.store(false, SeqCst);
    drop(s);
    std::thread::spawn(move || {
        let r = catch_unwind(AssertUnwindSafe(|| capture_stage(&app, my)));
        if r.is_err() {
            abort_capture(&app, my, Some(("capture_failed", "internal error".into())));
        }
    });
}

fn capture_stage(app: &AppHandle, my: u64) {
    let s = app.state::<AppState>();
    hide_popup_inner(app);
    if let Some(m) = app.get_webview_window("main") {
        if m.is_visible().unwrap_or(false) && !m.is_minimized().unwrap_or(false) {
            let _ = m.hide();
            s.restore_main.store(true, SeqCst);
        }
    }
    // let the compositor remove our own windows from the frame
    std::thread::sleep(Duration::from_millis(160));

    let shot = match capture::capture_cursor_monitor() {
        Ok(sh) => sh,
        Err(e) => return abort_capture(app, my, Some(("capture_failed", e))),
    };
    let (cx, cy) = (shot.mon_x + shot.mon_w as i32 / 2, shot.mon_y + shot.mon_h as i32 / 2);
    let Some(mon) = popup_monitor(app, cx, cy) else {
        return abort_capture(app, my, Some(("capture_failed", "monitor not found".into())));
    };
    let (mp, ms) = (mon.position(), mon.size());
    let (iw, ih) = (shot.img.width() as i64, shot.img.height() as i64);
    let same = (mp.x - shot.mon_x).abs() <= 2
        && (mp.y - shot.mon_y).abs() <= 2
        && (ms.width as i64 - iw).abs() <= 2
        && (ms.height as i64 - ih).abs() <= 2;
    if !same {
        // Never guess: a mismatched capture would produce a wrong crop.
        return abort_capture(
            app,
            my,
            Some(("capture_failed", format!("monitor geometry mismatch ({iw}x{ih} vs {}x{})", ms.width, ms.height))),
        );
    }
    let (mx, my_, mw, mh) = (mp.x, mp.y, ms.width, ms.height);
    *lock(&s.shot) = Some(shot);

    let Some(sel) = app.get_webview_window("selector") else {
        return abort_capture(app, my, Some(("capture_failed", "overlay window missing".into())));
    };
    let _ = sel.set_position(PhysicalPosition::new(mx, my_));
    let _ = sel.set_size(PhysicalSize::new(mw, mh));
    let _ = sel.set_position(PhysicalPosition::new(mx, my_));
    let _ = app.emit_to("selector", "selector-start", my);

    // watchdog: overlay must come up quickly, and never stays forever
    let app2 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(3000));
        let s = app2.state::<AppState>();
        if s.cap_gen.load(SeqCst) == my && !s.selector_shown.load(SeqCst) {
            return abort_capture(&app2, my, Some(("capture_failed", "overlay did not load".into())));
        }
        drop(s);
        std::thread::sleep(Duration::from_secs(62));
        abort_capture(&app2, my, None);
    });
}

#[derive(Deserialize)]
struct Selection {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    vw: f64,
    vh: f64,
}

fn selection_stage(app: &AppHandle, sel: Selection) {
    let s = app.state::<AppState>();
    let Some(mut shot) = lock(&s.shot).take() else {
        return; // already handled
    };
    let sf = app
        .get_webview_window("selector")
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0);
    teardown_selector(app);
    let job = new_job(app);
    let dir = cur_dir(app);

    let (iw, ih) = (shot.img.width() as f64, shot.img.height() as f64);
    let fail = |app: &AppHandle, kind: &str, detail: &str, shot: &mut capture::Shot| {
        shot.wipe();
        let restore = true;
        release_capture(app, restore);
        let job = new_job(app);
        open_popup(app, job, center_anchor(), error_state(kind, detail, "ocr", cur_dir(app)));
    };
    if sel.vw < 1.0 || sel.vh < 1.0 || !(sel.vw.is_finite() && sel.vh.is_finite()) {
        return fail(app, "capture_failed", "invalid viewport", &mut shot);
    }
    // The overlay must have been exactly as large as the captured monitor.
    if (sel.vw * sf - iw).abs() / iw > 0.015 || (sel.vh * sf - ih).abs() / ih > 0.015 {
        return fail(app, "capture_failed", "overlay and capture sizes differ", &mut shot);
    }
    let (rx, ry) = (iw / sel.vw, ih / sel.vh);
    let x0 = (sel.x * rx).round().clamp(0.0, iw);
    let y0 = (sel.y * ry).round().clamp(0.0, ih);
    let x1 = ((sel.x + sel.w) * rx).round().clamp(0.0, iw);
    let y1 = ((sel.y + sel.h) * ry).round().clamp(0.0, ih);
    let (cw, ch) = ((x1 - x0) as u32, (y1 - y0) as u32);
    if cw < 8 || ch < 8 {
        return fail(app, "selection_invalid", "", &mut shot);
    }
    let crop = image::imageops::crop_imm(&shot.img, x0 as u32, y0 as u32, cw, ch).to_image();
    let anchor = Anchor::Rect(shot.mon_x + x0 as i32, shot.mon_y + y0 as i32, cw as i32, ch as i32);
    shot.wipe();
    drop(shot);
    s.restore_main.store(false, SeqCst);

    // Show the popup at once, then OCR.
    open_popup(
        app,
        job,
        anchor,
        popup_for(dir, "loading", "ocr"),
    );

    let mut crop = crop;
    let ocr_res = catch_unwind(AssertUnwindSafe(|| ocr::recognize(&crop, ocr::lang_for(dir))));
    crop.as_mut().iter_mut().for_each(|b| *b = 0);
    drop(crop);
    s.busy.store(false, SeqCst);

    match ocr_res {
        Ok(Ok(paras)) if !paras.is_empty() => {
            let (text, truncated) = cap_text(&paras.join("\n\n"));
            if update_popup(app, job, |p| {
                p.original = text.clone();
                p.truncated = truncated;
            }) {
                translate_job(app, job, &text);
            }
        }
        Ok(Ok(_)) => {
            update_popup(app, job, |p| {
                p.status = "error".into();
                p.error_kind = "no_text".into();
            });
        }
        Ok(Err(ocr::OcrError::Unavailable(d))) => {
            update_popup(app, job, |p| {
                p.status = "error".into();
                // Hebrew uses the bundled Tesseract, never a Windows language pack, so it gets its own message.
                p.error_kind = if dir == "he-ar" { "ocr_unavailable_he" } else { "ocr_unavailable" }.into();
                p.error_detail = d;
            });
        }
        Ok(Err(ocr::OcrError::Failed(d))) => {
            update_popup(app, job, |p| {
                p.status = "error".into();
                p.error_kind = "ocr_failed".into();
                p.error_detail = d;
            });
        }
        Err(_) => {
            update_popup(app, job, |p| {
                p.status = "error".into();
                p.error_kind = "ocr_failed".into();
                p.error_detail = "internal error".into();
            });
        }
    }
}

fn clipboard_translate(app: AppHandle) {
    let s = app.state::<AppState>();
    if s.busy.compare_exchange(false, true, SeqCst, SeqCst).is_err() {
        return;
    }
    drop(s);
    std::thread::spawn(move || {
        let r = catch_unwind(AssertUnwindSafe(|| {
            let text = app.clipboard().read_text();
            app.state::<AppState>().busy.store(false, SeqCst);
            let anchor = center_anchor();
            match text {
                Ok(t) if !t.trim().is_empty() => {
                    let (t, truncated) = cap_text(&t);
                    let job = new_job(&app);
                    open_popup(
                        &app,
                        job,
                        anchor,
                        PopupState {
                            original: t.clone(),
                            truncated,
                            ..popup_for(cur_dir(&app), "loading", "clipboard")
                        },
                    );
                    translate_job(&app, job, &t);
                }
                _ => show_error(&app, anchor, "clipboard_empty", "", "clipboard"),
            }
        }));
        if r.is_err() {
            app.state::<AppState>().busy.store(false, SeqCst);
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Shortcuts
// ---------------------------------------------------------------------------------------------

/// Toggle the main window: hide it when it is visible and focused, otherwise bring it forward.
fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let visible = w.is_visible().unwrap_or(false);
        let min = w.is_minimized().unwrap_or(false);
        let focused = w.is_focused().unwrap_or(false);
        if visible && !min && focused {
            let _ = w.hide();
        } else {
            show_main(app);
        }
    }
}

/// Register the global shortcuts (`main` is optional; empty = disabled). Returns a list of
/// error codes (empty on success). Nothing is changed when validation fails.
fn register_shortcuts(app: &AppHandle, cap: &str, clip: &str, main: &str) -> Vec<String> {
    let mut errs = Vec::new();
    let a = Shortcut::from_str(cap);
    let b = Shortcut::from_str(clip);
    let c = if main.trim().is_empty() { None } else { Some(Shortcut::from_str(main)) };
    if a.is_err() {
        errs.push("shortcut_invalid:capture".to_string());
    }
    if b.is_err() {
        errs.push("shortcut_invalid:clipboard".to_string());
    }
    if matches!(c, Some(Err(_))) {
        errs.push("shortcut_invalid:main".to_string());
    }
    if let (Ok(a), Ok(b)) = (&a, &b) {
        if a == b {
            errs.push("shortcut_same".to_string());
        }
        if let Some(Ok(c)) = &c {
            if c == a || c == b {
                errs.push("shortcut_same".to_string());
            }
        }
    }
    if !errs.is_empty() {
        return errs;
    }
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let r = gs.on_shortcut(cap, |app, _sc, ev| {
        if ev.state() == ShortcutState::Pressed {
            begin_capture(app.clone());
        }
    });
    if r.is_err() {
        errs.push("shortcut_conflict:capture".to_string());
    }
    let r = gs.on_shortcut(clip, |app, _sc, ev| {
        if ev.state() == ShortcutState::Pressed {
            clipboard_translate(app.clone());
        }
    });
    if r.is_err() {
        errs.push("shortcut_conflict:clipboard".to_string());
    }
    if c.is_some() {
        let r = gs.on_shortcut(main, |app, _sc, ev| {
            if ev.state() == ShortcutState::Pressed {
                toggle_main(app);
            }
        });
        if r.is_err() {
            errs.push("shortcut_conflict:main".to_string());
        }
    }
    errs
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> Settings {
    lock(&state.settings).clone()
}

#[tauri::command]
fn set_settings(app: AppHandle, state: tauri::State<AppState>, settings: Settings) -> Result<Settings, String> {
    let mut new = settings;
    new.sanitize();
    let old = lock(&state.settings).clone();
    // The backend owns the remembered position; never trust the frontend copy.
    new.popup_pos = if new.popup_remember_position { old.popup_pos } else { None };
    if new.shortcut_capture != old.shortcut_capture
        || new.shortcut_clipboard != old.shortcut_clipboard
        || new.shortcut_main != old.shortcut_main
    {
        let errs = register_shortcuts(&app, &new.shortcut_capture, &new.shortcut_clipboard, &new.shortcut_main);
        if !errs.is_empty() {
            // restore the previous, working shortcuts
            let _ = register_shortcuts(&app, &old.shortcut_capture, &old.shortcut_clipboard, &old.shortcut_main);
            return Err(errs[0].clone());
        }
        *lock(&state.shortcut_errors) = Vec::new();
    }
    new.save(&state.settings_path)?;
    *lock(&state.settings) = new.clone();
    if new.theme != old.theme {
        apply_theme(&app, &new.theme);
    }
    if new.popup_pinned != old.popup_pinned {
        if let Some(w) = app.get_webview_window("popup") {
            let _ = w.set_always_on_top(new.popup_pinned);
        }
    }
    if new.ui_language != old.ui_language {
        if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), build_tray_menu(&app, &new.ui_language)) {
            let _ = tray.set_menu(Some(menu));
        }
    }
    let _ = app.emit("settings-changed", &new);
    Ok(new)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    shortcut_errors: Vec<String>,
}

#[tauri::command]
fn get_app_info(state: tauri::State<AppState>) -> AppInfo {
    AppInfo { version: env!("CARGO_PKG_VERSION").into(), shortcut_errors: lock(&state.shortcut_errors).clone() }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OcrInfo {
    direction: String,
    available: bool,
    /// windows | tesseract
    engine: String,
    language: String,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelInfo {
    id: String,
    label: String,
    upstream: String,
    license: String,
    /// not_loaded | loading | ready | failed
    state: String,
    error: String,
    /// missing | ok (quick file check; full SHA-256 verification is `verify_model`)
    files: String,
    size_bytes: u64,
    model_dir: String,
    /// Same folder, shortened with an environment variable so the UI does not expose the user name.
    model_dir_display: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineStatus {
    models: Vec<ModelInfo>,
    ocr: Vec<OcrInfo>,
}

/// Plain Win32 path without the `\\?\` verbatim prefix.
fn plain_path(p: &std::path::Path) -> String {
    let s = p.display().to_string();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

/// Replaces a leading `%LOCALAPPDATA%` / `%APPDATA%` / `%USERPROFILE%` with the variable name.
fn shorten_with_env(path: &str) -> String {
    for var in ["LOCALAPPDATA", "APPDATA", "USERPROFILE"] {
        if let Ok(base) = std::env::var(var) {
            let base = base.trim_end_matches('\\');
            if !base.is_empty() && path.len() >= base.len() && path[..base.len()].eq_ignore_ascii_case(base) {
                return format!("%{}%{}", var, &path[base.len()..]);
            }
        }
    }
    path.to_string()
}

#[tauri::command]
async fn get_engine_status(app: AppHandle) -> Result<EngineStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let s = app.state::<AppState>();
        let mut models = Vec::new();
        let mut ocr_info = Vec::new();
        for (i, spec) in models::SPECS.iter().enumerate() {
            let slot = &s.slots[i];
            let (state, error) = match &*lock(&slot.engine) {
                EngineSlot::NotLoaded => ("not_loaded", String::new()),
                EngineSlot::Loading => ("loading", String::new()),
                EngineSlot::Ready(_) => ("ready", String::new()),
                EngineSlot::Failed(m) => ("failed", m.clone()),
            };
            let dir = lock(&slot.model_dir).clone().or_else(|| find_model_dir(&app, spec.id).ok());
            let (files, size_bytes) = match dir.as_ref().map(|d| models::quick_check(d)) {
                Some(Ok(n)) => ("ok", n),
                _ => ("missing", 0),
            };
            let model_dir = dir.as_ref().map(|p| plain_path(p)).unwrap_or_default();
            models.push(ModelInfo {
                id: spec.id.into(),
                label: spec.label.into(),
                upstream: spec.upstream.into(),
                license: spec.license.into(),
                state: state.into(),
                error,
                files: files.into(),
                size_bytes,
                model_dir_display: shorten_with_env(&model_dir),
                model_dir,
            });
            let st = ocr::status(ocr::lang_for(spec.id));
            ocr_info.push(OcrInfo {
                direction: spec.id.into(),
                available: st.available,
                engine: st.engine.into(),
                language: st.language,
                detail: st.detail,
            });
        }
        EngineStatus { models, ocr: ocr_info }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Full SHA-256 check of an installed model (reads every file once; runs off the UI thread).
#[tauri::command]
async fn verify_model(app: AppHandle, direction: String) -> Result<models::Check, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let spec = models::spec(&direction).ok_or("Unknown translation direction")?;
        match find_model_dir(&app, spec.id) {
            Ok(d) => Ok(models::verify(spec, &d)),
            Err(m) => Ok(models::Check { state: "missing".into(), detail: m, size_bytes: 0 }),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Retry a failed model, or preload one that has not been used yet.
#[tauri::command]
fn reload_engine(app: AppHandle, direction: String) {
    let dir = norm_dir(&direction);
    let startable = matches!(
        &*lock(&slot_of(&app.state::<AppState>(), dir).engine),
        EngineSlot::Failed(_) | EngineSlot::NotLoaded
    );
    if startable {
        start_engine_load(&app, dir);
    }
}

#[tauri::command]
fn reveal_model_dir(app: AppHandle, direction: String) -> Result<(), String> {
    let dir = find_model_dir(&app, &direction)?;
    std::process::Command::new("explorer").arg(plain_path(&dir)).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Persist the selected translation direction (used by the shortcuts and the main window).
#[tauri::command]
fn set_direction(app: AppHandle, state: tauri::State<AppState>, direction: String) -> Result<Settings, String> {
    let dir = norm_dir(&direction);
    let mut cur = lock(&state.settings).clone();
    if cur.direction != dir {
        cur.direction = dir.into();
        cur.save(&state.settings_path)?;
        *lock(&state.settings) = cur.clone();
        let _ = app.emit("settings-changed", &cur);
    }
    Ok(cur)
}

#[tauri::command]
fn start_capture(app: AppHandle) {
    begin_capture(app);
}

#[tauri::command]
fn clipboard_translate_cmd(app: AppHandle) {
    clipboard_translate(app);
}

#[tauri::command]
fn selector_ready(app: AppHandle, state: tauri::State<AppState>, gen: u64) {
    if state.cap_gen.load(SeqCst) != gen || lock(&state.shot).is_none() {
        return;
    }
    if let Some(w) = app.get_webview_window("selector") {
        state.selector_shown.store(true, SeqCst);
        state.selector_focused.store(false, SeqCst);
        *lock(&state.selector_shown_at) = Some(Instant::now());
        let _ = w.show();
        let _ = w.set_always_on_top(true);
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn selection_done(app: AppHandle, sel: Selection) {
    std::thread::spawn(move || {
        let r = catch_unwind(AssertUnwindSafe(|| selection_stage(&app, sel)));
        if r.is_err() {
            release_capture(&app, true);
            show_error(&app, center_anchor(), "capture_failed", "internal error", "ocr");
        }
    });
}

#[tauri::command]
fn selection_cancel(app: AppHandle, state: tauri::State<AppState>) {
    let my = state.cap_gen.load(SeqCst);
    abort_capture(&app, my, None);
}

#[tauri::command]
fn read_clipboard_text(app: AppHandle) -> Result<String, String> {
    match app.clipboard().read_text() {
        Ok(t) if !t.trim().is_empty() => Ok(cap_text(&t).0),
        _ => Err("clipboard_empty".into()),
    }
}

#[tauri::command]
fn get_popup_state(state: tauri::State<AppState>) -> PopupState {
    lock(&state.popup).clone()
}

#[tauri::command]
fn resize_popup(app: AppHandle, state: tauri::State<AppState>, height: f64) {
    if !height.is_finite() {
        return;
    }
    *lock(&state.popup_h) = height.clamp(80.0, 1600.0);
    relayout_popup(&app);
}

#[tauri::command]
fn set_popup_wide(app: AppHandle, state: tauri::State<AppState>, wide: bool) {
    state.wide.store(wide, SeqCst);
    relayout_popup(&app);
}

#[tauri::command]
fn hide_popup(app: AppHandle) {
    hide_popup_inner(&app);
}

#[tauri::command]
fn set_pinned(app: AppHandle, state: tauri::State<AppState>, pinned: bool) -> Result<(), String> {
    // Pin only controls z-order ("stay above other windows") and is persisted. It never decides
    // whether the popup closes.
    let snap = {
        let mut st = lock(&state.settings);
        st.popup_pinned = pinned;
        st.save(&state.settings_path)?;
        st.clone()
    };
    if let Some(w) = app.get_webview_window("popup") {
        let _ = w.set_always_on_top(pinned);
    }
    let _ = app.emit("settings-changed", &snap);
    Ok(())
}

#[tauri::command]
fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
fn retranslate(app: AppHandle, text: String) {
    let (text, truncated) = cap_text(&text);
    if text.is_empty() {
        return;
    }
    let job = new_job(&app);
    if !update_popup(&app, job, |p| {
        p.status = "loading".into();
        p.original = text.clone();
        p.translation.clear();
        p.error_kind.clear();
        p.error_detail.clear();
        p.truncated = truncated;
    }) {
        return;
    }
    std::thread::spawn(move || translate_job(&app, job, &text));
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TextResult {
    translation: String,
    truncated: bool,
}

#[tauri::command]
async fn translate_text(app: AppHandle, text: String, direction: Option<String>) -> Result<TextResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = norm_dir(direction.as_deref().unwrap_or_else(|| cur_dir(&app)));
        let (t, truncated) = cap_text(&text);
        if t.is_empty() {
            return Err("empty".to_string());
        }
        let r = catch_unwind(AssertUnwindSafe(|| -> Result<String, EngineErr> {
            let eng = get_engine(&app, dir, true)?;
            eng.translate(&t).map_err(|e| ("translate_failed", e))
        }));
        match r {
            Ok(Ok(translation)) => {
                record_history(&app, &t, &translation, "text", dir);
                Ok(TextResult { translation, truncated })
            }
            Ok(Err((k, _))) => Err(k.to_string()),
            Err(_) => Err("translate_failed".to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}


// ---------------------------------------------------------------------------------------------
// History & favorites (local, opt-in)
// ---------------------------------------------------------------------------------------------

#[tauri::command]
fn history_list(state: tauri::State<AppState>) -> Vec<history::Entry> {
    lock(&state.history).clone()
}

fn persist_history(app: &AppHandle, snapshot: &[history::Entry]) -> Result<(), String> {
    let s = app.state::<AppState>();
    history::save(&s.history_path, snapshot)?;
    let _ = app.emit("history-changed", ());
    Ok(())
}

#[tauri::command]
fn history_delete(app: AppHandle, state: tauri::State<AppState>, id: u64) -> Result<(), String> {
    let snap = {
        let mut h = lock(&state.history);
        history::delete(&mut h, id);
        h.clone()
    };
    persist_history(&app, &snap)
}

/// Remove every entry, including favorites, and the file on disk.
#[tauri::command]
fn history_clear(app: AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    lock(&state.history).clear();
    match std::fs::remove_file(&state.history_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    let _ = app.emit("history-changed", ());
    Ok(())
}

/// Set the favorite flag of one entry (also updates the popup if it shows that entry).
#[tauri::command]
fn history_set_favorite(app: AppHandle, state: tauri::State<AppState>, id: u64, favorite: bool) -> Result<(), String> {
    let (found, snap) = {
        let mut h = lock(&state.history);
        let f = history::set_favorite(&mut h, id, favorite);
        (f, h.clone())
    };
    if !found {
        return Err("not_found".into());
    }
    let popup_snap = {
        let mut p = lock(&state.popup);
        if p.entry_id == id {
            p.favorite = favorite;
            Some(p.clone())
        } else {
            None
        }
    };
    if let Some(p) = popup_snap {
        let _ = app.emit("popup-update", &p);
    }
    persist_history(&app, &snap)
}

/// Show an old history entry in the popup without translating again.
#[tauri::command]
fn history_reopen(app: AppHandle, state: tauri::State<AppState>, id: u64) -> Result<(), String> {
    let e = lock(&state.history).iter().find(|e| e.id == id).cloned().ok_or("not_found")?;
    let job = new_job(&app);
    let source = match e.source.as_str() {
        "screen" => "ocr",
        "clipboard" => "clipboard",
        _ => "text",
    };
    let st = PopupState {
        original: e.original,
        translation: e.translation,
        entry_id: e.id,
        favorite: e.favorite,
        ..popup_for(dir_from_langs(&e.source_lang), "done", source)
    };
    open_popup(&app, job, center_anchor(), st);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------------------------

fn create_windows(app: &AppHandle, s: &Settings) -> tauri::Result<()> {
    let theme = match s.theme.as_str() {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    };
    let bg = if s.theme == "light" { Color(245, 245, 244, 255) } else { Color(5, 5, 5, 255) };

    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("GlassTranslate Lite")
        .inner_size(480.0, 720.0)
        .min_inner_size(420.0, 560.0)
        .center()
        .theme(theme)
        .background_color(bg)
        .build()?;

    WebviewWindowBuilder::new(app, "selector", WebviewUrl::App("index.html".into()))
        .title("GlassTranslate Selector")
        .decorations(false)
        .resizable(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .inner_size(400.0, 300.0)
        .theme(theme)
        .background_color(Color(0, 0, 0, 255))
        .build()?;

    WebviewWindowBuilder::new(app, "popup", WebviewUrl::App("index.html".into()))
        .title("GlassTranslate")
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .shadow(false)
        .always_on_top(s.popup_pinned)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .inner_size(POPUP_W, 200.0)
        .theme(theme)
        .build()?;
    Ok(())
}

fn handle_window_event(window: &tauri::Window, event: &WindowEvent) {
    let app = window.app_handle();
    let s = app.state::<AppState>();
    match (window.label(), event) {
        ("main", WindowEvent::CloseRequested { api, .. }) => {
            api.prevent_close();
            let _ = window.hide();
        }
        ("selector", WindowEvent::CloseRequested { api, .. }) => {
            api.prevent_close();
            let my = s.cap_gen.load(SeqCst);
            abort_capture(app, my, None);
        }
        ("selector", WindowEvent::Focused(true)) => s.selector_focused.store(true, SeqCst),
        ("selector", WindowEvent::Focused(false)) => {
            let old_enough = lock(&s.selector_shown_at)
                .map(|t| t.elapsed() > Duration::from_millis(500))
                .unwrap_or(false);
            if s.selector_shown.load(SeqCst) && s.selector_focused.load(SeqCst) && old_enough {
                let my = s.cap_gen.load(SeqCst);
                abort_capture(app, my, None);
            }
        }
        ("popup", WindowEvent::CloseRequested { api, .. }) => {
            api.prevent_close();
            hide_popup_inner(app);
        }
        // The popup deliberately has NO focus handling: it closes only through hide_popup_inner,
        // which is reached from the Close button, Esc inside the popup, or quitting the app.
        ("popup", WindowEvent::Moved(_)) => note_popup_moved(app),
        _ => {}
    }
}

/// Debounced: once the popup has stopped moving, persist a user-dragged position (only when the
/// "remember position" setting is on). Programmatic placement is ignored.
fn note_popup_moved(app: &AppHandle) {
    let s = app.state::<AppState>();
    if !lock(&s.settings).popup_remember_position {
        return;
    }
    *lock(&s.last_move) = Some(Instant::now());
    if s.move_pending.swap(true, SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let s = app.state::<AppState>();
            let quiet = lock(&s.last_move).map(|t| t.elapsed() >= Duration::from_millis(400)).unwrap_or(true);
            if quiet {
                break;
            }
        }
        let s = app.state::<AppState>();
        s.move_pending.store(false, SeqCst);
        let visible = app.get_webview_window("popup").and_then(|w| w.is_visible().ok()).unwrap_or(false);
        if !visible || !popup_was_dragged(&app) {
            return;
        }
        if let Some(w) = app.get_webview_window("popup") {
            if let Ok(p) = w.outer_position() {
                let mut st = lock(&s.settings);
                if st.popup_remember_position && st.popup_pos != Some((p.x, p.y)) {
                    st.popup_pos = Some((p.x, p.y));
                    let _ = st.save(&s.settings_path);
                }
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .register_uri_scheme_protocol("gtshot", |ctx, req| {
            let ok = req.uri().path() == "/current";
            let body: Option<Vec<u8>> = if ok {
                let st = ctx.app_handle().state::<AppState>();
                let g = lock(&st.shot);
                g.as_ref().map(|sh| sh.bmp.clone())
            } else {
                None
            };
            match body {
                Some(b) => tauri::http::Response::builder()
                    .status(200)
                    .header("Content-Type", "image/bmp")
                    .header("Cache-Control", "no-store")
                    .header("Access-Control-Allow-Origin", "*")
                    .body(Cow::Owned(b))
                    .unwrap(),
                None => tauri::http::Response::builder()
                    .status(404)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(Cow::Owned(Vec::new()))
                    .unwrap(),
            }
        })
        .on_window_event(handle_window_event)
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            get_app_info,
            get_engine_status,
            reload_engine,
            reveal_model_dir,
            verify_model,
            set_direction,
            start_capture,
            clipboard_translate_cmd,
            selector_ready,
            selection_done,
            selection_cancel,
            read_clipboard_text,
            get_popup_state,
            resize_popup,
            set_popup_wide,
            hide_popup,
            set_pinned,
            copy_text,
            retranslate,
            translate_text,
            history_list,
            history_delete,
            history_clear,
            history_set_favorite,
            history_reopen,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let mut tess_dirs = Vec::new();
            if let Ok(res) = handle.path().resource_dir() {
                tess_dirs.push(res.join("tesseract"));
                tess_dirs.push(res.join("resources").join("tesseract"));
            }
            if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("tesseract"))) {
                tess_dirs.push(exe_dir);
            }
            ocr::init_tesseract_dirs(tess_dirs);
            let cfg_dir = handle.path().app_config_dir()?;
            let settings_path = cfg_dir.join("settings.json");
            let settings = Settings::load(&settings_path);
            let history_path = cfg_dir.join("history.json");
            let history_list = history::load(&history_path);
            app.manage(AppState {
                settings: Mutex::new(settings.clone()),
                settings_path,
                slots: [Slot::new(), Slot::new()],
                shot: Mutex::new(None),
                busy: AtomicBool::new(false),
                cap_gen: AtomicU64::new(0),
                selector_shown: AtomicBool::new(false),
                selector_focused: AtomicBool::new(false),
                selector_shown_at: Mutex::new(None),
                restore_main: AtomicBool::new(false),
                popup: Mutex::new(PopupState { status: "idle".into(), ..Default::default() }),
                anchor: Mutex::new(Anchor::Center(0, 0)),
                last_placed: Mutex::new(None),
                popup_gen: AtomicU64::new(0),
                session: AtomicU64::new(0),
                wide: AtomicBool::new(false),
                popup_h: Mutex::new(200.0),
                move_pending: AtomicBool::new(false),
                last_move: Mutex::new(None),
                shortcut_errors: Mutex::new(Vec::new()),
                history: Mutex::new(history_list),
                history_path,
            });
            create_windows(&handle, &settings)?;
            setup_tray(&handle, &settings.ui_language)?;
            let errs = register_shortcuts(
                &handle,
                &settings.shortcut_capture,
                &settings.shortcut_clipboard,
                &settings.shortcut_main,
            );
            *lock(&handle.state::<AppState>().shortcut_errors) = errs;
            start_engine_load(&handle, &settings.direction);
            Ok(())
        });

    builder
        .build(tauri::generate_context!())
        .expect("failed to build GlassTranslate Lite")
        .run(|app, event| match event {
            RunEvent::ExitRequested { code, api, .. } => {
                // closing windows hides to tray; only an explicit exit(0) quits
                if code.is_none() {
                    api.prevent_exit();
                }
            }
            RunEvent::Exit => {
                let _ = app.remove_tray_by_id("main");
            }
            _ => {}
        });
}
