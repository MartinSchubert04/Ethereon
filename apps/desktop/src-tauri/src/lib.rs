//! App de escritorio: aloja el broker, el servidor MCP y la bandeja. La ventana es
//! descartable: al cerrarla se destruye el WebView entero y queda solo el proceso Rust.

mod commands;
mod files;
mod fonts;
mod hotkey;
mod terminal;
mod updater;
mod window;

pub use window::show_main;

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, Wry};
use tauri_plugin_notification::NotificationExt;
use ethereon_core::{Broker, Notifier};

pub const DEFAULT_PORT: u16 = 47321;

/// Lo que no va en el vault: hace falta antes de desbloquearlo, o no afecta la seguridad.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalConfig {
    pub port: u16,
    pub window: Option<window::WindowBox>,
    /// Atajo global para traer la ventana (None = apagado).
    pub hotkey: Option<String>,
    /// Donde abre la carpeta del Explorador el atajo: "wt" o "tab".
    pub open_in: String,
}

impl Default for LocalConfig {
    fn default() -> Self {
        LocalConfig { port: DEFAULT_PORT, window: None, hotkey: Some(hotkey::DEFAULT.into()), open_in: "wt".into() }
    }
}

/// Guarda `config.json` con la ventana como quedo. El puerto se respeta tal como estaba
/// en el archivo (no el de `ETHEREON_PORT`).
pub fn save_config(app: &AppHandle) {
    let st = app.state::<AppState>();
    let path = st.broker.dir().join("config.json");
    let mut cfg: LocalConfig = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    cfg.window = *st.window.lock().unwrap();
    cfg.hotkey = st.hotkey.combo.lock().unwrap().clone();
    cfg.open_in = st.hotkey.open_in.lock().unwrap().clone();
    let _ = std::fs::write(&path, serde_json::to_vec_pretty(&cfg).unwrap());
}

pub struct AppState {
    pub broker: Arc<Broker>,
    pub config: LocalConfig,
    pub mcp_error: Mutex<Option<String>>,
    pub terms: terminal::Terms,
    pub sftps: files::Sftps,
    pub hotkey: hotkey::HotkeyState,
    pub window: Mutex<Option<window::WindowBox>>,
    pub update: Mutex<updater::Pending>,
    tray: Mutex<Option<TrayItems>>,
}

struct TrayItems {
    agent: CheckMenuItem<Wry>,
    lock: MenuItem<Wry>,
    tray: tauri::tray::TrayIcon,
}

struct UiNotifier(AppHandle);

impl Notifier for UiNotifier {
    fn event(&self, name: &str, payload: serde_json::Value) {
        let _ = self.0.emit(name, payload);
        if matches!(name, "vault" | "approvals" | "data") {
            let h = self.0.clone();
            // Fuera del lock del broker: la bandeja vuelve a preguntarle el estado.
            tauri::async_runtime::spawn(async move { refresh_tray(&h) });
        }
    }
    fn notify(&self, title: &str, body: &str) {
        let _ = self.0.notification().builder().title(title).body(body).show();
    }
}

/// `ETHEREON_DATA_DIR` permite probar con otro vault sin tocar el de verdad.
fn data_dir(app: &AppHandle) -> PathBuf {
    match std::env::var_os("ETHEREON_DATA_DIR") {
        Some(d) => PathBuf::from(d),
        None => app.path().app_data_dir().expect("sin carpeta de datos"),
    }
}

/// Crear ventanas desde el hilo del menu de la bandeja traba Windows: se difiere.
fn defer(app: &AppHandle, f: fn(&AppHandle)) {
    let h = app.clone();
    tauri::async_runtime::spawn(async move {
        let h2 = h.clone();
        let _ = h.run_on_main_thread(move || f(&h2));
    });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Ethereon", true, None::<&str>)?;
    let agent = CheckMenuItem::with_id(app, "agent", "Agent access", true, false, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", "Lock vault", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &sep1, &agent, &lock, &sep2, &quit])?;
    let tray = TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().unwrap())
        .tooltip("Ethereon")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => defer(app, show_main),
            "agent" => {
                let b = app.state::<AppState>().broker.clone();
                let on = !b.agent_enabled();
                let _ = b.set_agent_enabled(on);
                refresh_tray(app);
            }
            "lock" => {
                let b = app.state::<AppState>().broker.clone();
                tauri::async_runtime::spawn(async move { b.lock().await });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                defer(tray.app_handle(), show_main);
            }
        })
        .build(app)?;
    *app.state::<AppState>().tray.lock().unwrap() = Some(TrayItems { agent, lock, tray });
    refresh_tray(app);
    Ok(())
}

fn refresh_tray(app: &AppHandle) {
    let st = app.state::<AppState>();
    let unlocked = st.broker.is_unlocked();
    let agent = unlocked && st.broker.agent_enabled();
    let waiting = st.broker.approvals().len();
    let guard = st.tray.lock().unwrap();
    if let Some(t) = guard.as_ref() {
        let _ = t.agent.set_enabled(unlocked);
        let _ = t.agent.set_checked(agent);
        let _ = t.lock.set_enabled(unlocked);
        let tip = match (unlocked, agent, waiting) {
            (false, _, _) => "Ethereon: locked".to_string(),
            (true, false, _) => "Ethereon: agent access off".to_string(),
            (true, true, 0) => "Ethereon: ready".to_string(),
            (true, true, n) => format!("Ethereon: {n} waiting for approval"),
        };
        let _ = t.tray.set_tooltip(Some(&tip));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .manage(window::PendingTabs::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let dir = data_dir(&handle);
            let _ = std::fs::create_dir_all(&dir);
            let cfg_path = dir.join("config.json");
            let mut config: LocalConfig =
                std::fs::read(&cfg_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            if let Some(p) = std::env::var("ETHEREON_PORT").ok().and_then(|p| p.parse().ok()) {
                config.port = p;
            }
            if !cfg_path.exists() {
                let _ = std::fs::write(&cfg_path, serde_json::to_vec_pretty(&config).unwrap());
            }
            let broker = Broker::new(dir, Arc::new(UiNotifier(handle.clone())));
            app.manage(AppState {
                broker: broker.clone(),
                config: config.clone(),
                mcp_error: Mutex::new(None),
                terms: Default::default(),
                sftps: Default::default(),
                hotkey: hotkey::HotkeyState {
                    combo: Mutex::new(config.hotkey.clone()),
                    pending: Default::default(),
                    open_in: Mutex::new(config.open_in.clone()),
                },
                window: Mutex::new(config.window),
                update: Mutex::new(Default::default()),
                tray: Mutex::new(None),
            });

            tauri::async_runtime::spawn(broker.clone().maintenance_loop());
            // MCP solo en 127.0.0.1: nada de afuera de la PC llega (el modo servidor es otra etapa).
            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
                if let Err(e) = ethereon_core::mcp::serve(broker, addr).await {
                    let msg = format!("could not listen on {addr}: {e}");
                    log::error!("{msg}");
                    *h.state::<AppState>().mcp_error.lock().unwrap() = Some(msg);
                }
            });

            build_tray(&handle)?;
            // Si otra app ya usa el atajo, Ethereon arranca igual y lo avisa en Settings.
            if let Err(e) = hotkey::register(&handle, config.hotkey.as_deref()) {
                log::warn!("{e}");
            }
            updater::start(handle.clone());
            // Despues de una actualizacion vuelve como estaba; si no, respeta --hidden
            // (el arranque con Windows).
            let show = match updater::take_relaunch(&handle) {
                Some(r) => r.show_main,
                None => !std::env::args().any(|a| a == "--hidden"),
            };
            if show {
                show_main(&handle);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::vault_create,
            commands::vault_unlock,
            commands::vault_lock,
            commands::vault_change_password,
            commands::touch,
            commands::snapshot,
            commands::host_save,
            commands::host_move,
            commands::host_delete,
            commands::host_test,
            commands::open_link,
            updater::session_part,
            updater::session_take,
            commands::host_trust,
            commands::cred_save,
            commands::cred_delete,
            commands::group_save,
            commands::group_delete,
            commands::denylist_save,
            commands::settings_save,
            commands::agent_set,
            commands::device_create,
            commands::device_revoke,
            commands::device_delete,
            commands::approvals_list,
            commands::approval_resolve,
            commands::alerts_list,
            commands::alert_dismiss,
            commands::policy_test,
            commands::audit_search,
            commands::audit_verify,
            commands::audit_export,
            commands::import_ssh_config,
            commands::import_key_file,
            commands::import_bitwarden,
            commands::import_detect,
            commands::import_auto,
            commands::export_data,
            commands::backup_vault,
            commands::app_version,
            commands::update_status,
            commands::update_check,
            commands::update_now,
            terminal::term_open,
            terminal::term_write,
            terminal::term_resize,
            terminal::term_close,
            terminal::term_detach,
            terminal::term_attach,
            window::term_window_open,
            window::term_window_take,
            window::term_windows,
            window::term_window_give,
            window::main_window_show,
            window::window_at,
            terminal::term_open_local,
            terminal::local_shells,
            fonts::system_fonts,
            fonts::wt_font,
            hotkey::take_open_here,
            hotkey::hotkey_get,
            hotkey::hotkey_set,
            hotkey::open_in_get,
            hotkey::open_in_set,
            files::sftp_open,
            files::sftp_close,
            files::sftp_list,
            files::sftp_realpath,
            files::sftp_mkdir,
            files::sftp_rename,
            files::sftp_remove,
            files::files_transfer,
            files::sftp_cancel,
            files::local_home,
            files::local_list,
            files::local_mkdir,
            files::local_rename,
            files::local_remove,
        ])
        .build(tauri::generate_context!())
        .expect("no se pudo iniciar Ethereon")
        .run(|app, e| {
            // Cerrar la ventana no cierra la app: el broker sigue en la bandeja.
            match e {
                RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
                RunEvent::Exit => {
                    save_config(app);
                    let b = app.state::<AppState>().broker.clone();
                    tauri::async_runtime::block_on(b.lock());
                }
                _ => {}
            }
        });
}
