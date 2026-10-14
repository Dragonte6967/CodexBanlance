#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod codex;
mod desktop;
mod model;
mod settings;

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use tokio::sync::mpsc;

pub struct WidgetState {
    snapshot: Mutex<model::Snapshot>,
    settings: Mutex<settings::Store>,
    refresh: mpsc::Sender<()>,
    stopping: AtomicBool,
    user_hidden: AtomicBool,
}

#[tauri::command]
fn get_snapshot(state: tauri::State<WidgetState>) -> model::Snapshot {
    state.snapshot.lock().unwrap().clone()
}
#[tauri::command]
fn refresh_quota(state: tauri::State<WidgetState>) {
    if !state.snapshot.lock().unwrap().refreshing {
        let _ = state.refresh.try_send(());
    }
}
#[tauri::command]
fn select_bucket(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<WidgetState>();
    {
        let mut snapshot = state.snapshot.lock().unwrap();
        if !snapshot.buckets.iter().any(|b| b.id == id) {
            return Err("额度类型不存在".into());
        }
        snapshot.selected = id.clone();
    }
    let mut settings = state.settings.lock().unwrap();
    settings.value.selected = id;
    settings.save()?;
    drop(settings);
    codex::publish(&app);
    Ok(())
}
#[tauri::command]
fn hide_widget(window: WebviewWindow) -> Result<(), String> {
    let state = window.state::<WidgetState>();
    state.user_hidden.store(true, Ordering::Relaxed);
    if window.hide().is_err() {
        state.user_hidden.store(false, Ordering::Relaxed);
        return Err("无法隐藏窗口".into());
    }
    Ok(())
}
#[tauri::command]
fn drag_widget(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|_| "无法拖动窗口".into())
}
#[tauri::command]
fn choose_cli(app: AppHandle) {
    let handle = app.clone();
    app.dialog()
        .file()
        .set_title("选择 Codex CLI（codex.exe）")
        .add_filter("Codex CLI", &["exe"])
        .pick_file(move |file| {
            if let Some(file) = file {
                if let Ok(path) = file.into_path() {
                    if path.is_file()
                        && path
                            .extension()
                            .is_some_and(|v| v.eq_ignore_ascii_case("exe"))
                    {
                        let state = handle.state::<WidgetState>();
                        let mut store = state.settings.lock().unwrap();
                        store.value.cli_path = Some(path);
                        if let Err(error) = store.save() {
                            state.snapshot.lock().unwrap().error = Some(error);
                        }
                        drop(store);
                        let _ = state.refresh.try_send(());
                        codex::publish(&handle);
                    }
                }
            }
        });
}
fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        app.state::<WidgetState>()
            .user_hidden
            .store(false, Ordering::Relaxed);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
fn tray_icon() -> Image<'static> {
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 0..32usize {
        for x in 0..32usize {
            let i = (y * 32 + x) * 4;
            let corner = (x < 5 || x > 26) && (y < 5 || y > 26);
            let white = ((7..=10).contains(&x) && (9..=22).contains(&y))
                || ((10..=23).contains(&x) && ((7..=10).contains(&y) || (21..=24).contains(&y)));
            rgba[i..i + 4].copy_from_slice(if corner {
                &[0, 0, 0, 0]
            } else if white {
                &[242, 250, 246, 255]
            } else {
                &[45, 88, 68, 255]
            });
        }
    }
    Image::new_owned(rgba, 32, 32)
}
fn geometry(window: &WebviewWindow, initial: bool) -> tauri::Result<()> {
    let app = window.app_handle();
    let state = app.state::<WidgetState>();
    let saved = state.settings.lock().unwrap().value.position;
    let position = if initial {
        saved.map(|(x, y)| PhysicalPosition::new(x, y))
    } else {
        window.outer_position().ok()
    };
    let size = window.outer_size().unwrap_or(PhysicalSize::new(500, 200));
    let monitor = if let Some(p) = position {
        window.monitor_from_point(
            (p.x + size.width as i32 / 2) as f64,
            (p.y + size.height as i32 / 2) as f64,
        )?
    } else {
        None
    }
    .or(window.primary_monitor()?);
    if let Some(monitor) = monitor {
        let target = model::widget_size(monitor.size().width, monitor.size().height);
        if window.inner_size()? != PhysicalSize::new(target.0, target.1) {
            window.set_size(PhysicalSize::new(target.0, target.1))?;
        }
        let work = monitor.work_area();
        let work_rect = (
            work.position.x,
            work.position.y,
            work.size.width,
            work.size.height,
        );
        let margin = (24.0 * monitor.size().height as f64 / 1440.0).round() as i32;
        let (x, y) = position.map(|p| (p.x, p.y)).unwrap_or((
            work.position.x + work.size.width as i32 - target.0 as i32 - margin,
            work.position.y + margin,
        ));
        let (x, y) = model::fit_position(x, y, work_rect, target);
        if window.outer_position()? != PhysicalPosition::new(x, y) {
            window.set_position(PhysicalPosition::new(x, y))?;
        }
    }
    Ok(())
}

fn main() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            refresh_quota,
            select_bucket,
            hide_widget,
            drag_widget,
            choose_cli
        ])
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let store = settings::Store::load(data_dir.join("settings.json"));
            let selected = if store.value.selected.is_empty() {
                "codex".into()
            } else {
                store.value.selected.clone()
            };
            let (sender, receiver) = mpsc::channel(1);
            app.manage(WidgetState {
                snapshot: Mutex::new(model::Snapshot {
                    selected,
                    refreshing: true,
                    ..Default::default()
                }),
                settings: Mutex::new(store),
                refresh: sender,
                stopping: AtomicBool::new(false),
                user_hidden: AtomicBool::new(false),
            });
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("Codex 额度")
                    .inner_size(500.0, 200.0)
                    .transparent(true)
                    .decorations(false)
                    .shadow(false)
                    .resizable(false)
                    .maximizable(false)
                    .minimizable(false)
                    .always_on_top(false)
                    .skip_taskbar(true)
                    .visible(false)
                    .data_directory(data_dir.join("webview"))
                    .build()?;
            geometry(&window, true)?;
            window.show()?;

            let show_item = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
            let refresh = MenuItem::with_id(app, "refresh", "立即刷新", true, None::<&str>)?;
            let choose = MenuItem::with_id(app, "choose", "选择 Codex CLI…", true, None::<&str>)?;
            let auto = CheckMenuItem::with_id(
                app,
                "auto",
                "开机自启",
                true,
                app.autolaunch().is_enabled().unwrap_or(false),
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &refresh, &choose, &auto, &quit])?;
            let auto_item = auto.clone();
            TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon())
                .tooltip("Codex 额度 · 右键菜单")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        show(tray.app_handle());
                    }
                })
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = hide_widget(w);
                            } else {
                                show(app);
                            }
                        }
                    }
                    "refresh" => refresh_quota(app.state::<WidgetState>()),
                    "choose" => choose_cli(app.clone()),
                    "auto" => {
                        let manager = app.autolaunch();
                        let enabled = manager.is_enabled().unwrap_or(false);
                        let result = if enabled {
                            manager.disable()
                        } else {
                            manager.enable()
                        };
                        let _ = auto_item.set_checked(manager.is_enabled().unwrap_or(false));
                        if result.is_err() {
                            app.state::<WidgetState>().snapshot.lock().unwrap().error =
                                Some("无法更改开机自启，请检查系统权限".into());
                            codex::publish(app);
                        }
                    }
                    "quit" => {
                        let state = app.state::<WidgetState>();
                        state.stopping.store(true, Ordering::Relaxed);
                        if let Some(w) = app.get_webview_window("main") {
                            if let Ok(p) = w.outer_position() {
                                let mut store = state.settings.lock().unwrap();
                                store.value.position = Some((p.x, p.y));
                                let _ = store.save();
                            }
                        }
                        let _ = state.refresh.try_send(());
                        app.exit(0);
                    }
                    _ => (),
                })
                .build(app)?;

            tauri::async_runtime::spawn(codex::run(
                app.handle().clone(),
                receiver,
                data_dir.clone(),
            ));
            // Opt-in integration check for developers; never runs on normal launch.
            if std::env::args_os().any(|arg| arg == "--smoke-test") {
                let smoke_app = app.handle().clone();
                let report_path = data_dir.join("smoke-report.json");
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(25)).await;
                    let snapshot = smoke_app
                        .state::<WidgetState>()
                        .snapshot
                        .lock()
                        .unwrap()
                        .clone();
                    if let Some(w) = smoke_app.get_webview_window("main") {
                        let report = serde_json::json!({
                            "snapshot": snapshot,
                            "size": w.inner_size().ok().map(|s| (s.width,s.height)),
                            "position": w.outer_position().ok().map(|p| (p.x,p.y)),
                            "decorated": w.is_decorated().ok(),
                            "alwaysOnTop": w.is_always_on_top().ok(),
                            "visible": w.is_visible().ok()
                        });
                        let _ = std::fs::write(
                            report_path,
                            serde_json::to_vec_pretty(&report).unwrap(),
                        );
                    }
                    smoke_app.exit(0);
                });
            }
            let handle = app.handle().clone();
            let desktop_app = app.handle().clone();
            let native_window = window.hwnd()?.0 as usize;
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    let ui_app = desktop_app.clone();
                    if desktop_app
                        .run_on_main_thread(move || {
                            let state = ui_app.state::<WidgetState>();
                            if !state.user_hidden.load(Ordering::Relaxed)
                                && !state.stopping.load(Ordering::Relaxed)
                            {
                                desktop::preserve_on_desktop(native_window as _);
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
            tauri::async_runtime::spawn(async move {
                let mut previous = None;
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let Some(window) = handle.get_webview_window("main") else {
                        break;
                    };
                    let position = window.outer_position().ok();
                    // Wait for dragging to settle before clamping or persisting.
                    if position == previous {
                        let _ = geometry(&window, false);
                        if let Ok(p) = window.outer_position() {
                            let state = handle.state::<WidgetState>();
                            let mut store = state.settings.lock().unwrap();
                            if store.value.position != Some((p.x, p.y)) {
                                store.value.position = Some((p.x, p.y));
                                let _ = store.save();
                            }
                        }
                    }
                    previous = position;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window
                    .state::<WidgetState>()
                    .user_hidden
                    .store(true, Ordering::Relaxed);
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!());
    if result.is_err() {
        // This native fallback works even when WebView2 cannot create a window.
        let title: Vec<u16> = "Codex 额度\0".encode_utf16().collect();
        let message: Vec<u16> = "无法启动小组件。请确认已安装 Microsoft Edge WebView2 Runtime，并且当前用户的数据目录可写。程序不会自动安装运行时。\0".encode_utf16().collect();
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                0x10,
            );
        }
    }
}
