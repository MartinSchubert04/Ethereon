//! Atajo global (por defecto Ctrl Shift Alt T): trae Ethereon al frente desde cualquier
//! lado, aunque este escondido en la bandeja. Si en ese momento la ventana activa es una
//! carpeta del Explorador (o el Escritorio), la interfaz abre PowerShell 7 en esa ruta,
//! como "Abrir en Terminal".
//!
//! La carpeta se abre en Windows Terminal (con el perfil y el tema de la persona) o en una
//! pestaña de Ethereon, segun Settings. La carpeta se lee antes de mostrar la ventana:
//! despues, la activa ya es Ethereon.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::AppState;

pub const DEFAULT: &str = "Ctrl+Shift+Alt+T";

/// Pedido de abrir una terminal local que espera a la interfaz (que puede estar naciendo,
/// o con el vault bloqueado).
#[derive(Clone, Debug, Serialize)]
pub struct OpenHere {
    pub cwd: String,
    /// Sin carpeta del Explorador: si ya hay una terminal local abierta se va a esa en vez
    /// de abrir otra (apretar el atajo varias veces no apila pestañas).
    pub reuse: bool,
}

#[derive(Default)]
pub struct HotkeyState {
    pub combo: Mutex<Option<String>>,
    pub pending: Mutex<Option<OpenHere>>,
    /// Donde se abre la carpeta: "wt" (Windows Terminal) o "tab" (pestaña de Ethereon).
    pub open_in: Mutex<String>,
}

/// Windows Terminal, si esta instalado (wt.exe es un alias en WindowsApps).
fn wt_path() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(r"Microsoft\WindowsApps\wt.exe");
    std::fs::symlink_metadata(&p).is_ok().then_some(p)
}

/// Abre Windows Terminal en `dir`, en una ventana nueva y con el perfil por defecto.
fn open_wt(dir: &std::path::Path) -> bool {
    let Some(wt) = wt_path() else { return false };
    // Una barra al final ("C:\") escaparia la comilla de cierre del argumento.
    let mut d = dir.to_string_lossy().trim_end_matches('\\').to_string();
    if d.ends_with(':') {
        d.push_str("\\.");
    }
    std::process::Command::new(wt).args(["-w", "new", "-d", &d]).spawn().is_ok()
}

/// Registra `combo` (o nada) en lugar del atajo anterior.
pub fn register(app: &AppHandle, combo: Option<&str>) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if let Some(c) = combo.filter(|c| !c.trim().is_empty()) {
        gs.on_shortcut(c, |app, _shortcut, ev| {
            if ev.state() == ShortcutState::Pressed {
                pressed(app);
            }
        })
        .map_err(|e| format!("could not use {c}: {e}. Another app may already use it."))?;
    }
    Ok(())
}

fn pressed(app: &AppHandle) {
    let folder = active_folder();
    let st = app.state::<AppState>();
    let in_wt = st.hotkey.open_in.lock().unwrap().as_str() == "wt";
    // Con una carpeta y Windows Terminal elegido, se abre ahi y Ethereon no se muestra.
    if let (Some(dir), true) = (&folder, in_wt) {
        if open_wt(dir) {
            return;
        }
    }
    // Con carpeta: una terminal ahi. Sin carpeta: PowerShell en el Escritorio.
    let open = match folder {
        Some(dir) => Some(OpenHere { cwd: dir.to_string_lossy().to_string(), reuse: false }),
        None => desktop_dir().map(|d| OpenHere { cwd: d.to_string_lossy().to_string(), reuse: true }),
    };
    *app.state::<AppState>().hotkey.pending.lock().unwrap() = open;
    let h = app.clone();
    // Igual que desde la bandeja: la ventana se crea fuera de este callback.
    tauri::async_runtime::spawn(async move {
        let h2 = h.clone();
        let _ = h.run_on_main_thread(move || {
            crate::window::show_main(&h2);
            let _ = h2.emit("open-here", ());
        });
    });
}

/// La interfaz lo pide al arrancar y cuando llega el aviso "open-here".
#[tauri::command]
pub fn take_open_here(s: tauri::State<AppState>) -> Option<OpenHere> {
    s.hotkey.pending.lock().unwrap().take()
}

#[tauri::command]
pub fn hotkey_get(s: tauri::State<AppState>) -> Option<String> {
    s.hotkey.combo.lock().unwrap().clone()
}

#[derive(Serialize)]
pub struct OpenIn {
    mode: String,
    wt_installed: bool,
}

#[tauri::command]
pub fn open_in_get(s: tauri::State<AppState>) -> OpenIn {
    OpenIn { mode: s.hotkey.open_in.lock().unwrap().clone(), wt_installed: wt_path().is_some() }
}

#[tauri::command]
pub fn open_in_set(app: AppHandle, s: tauri::State<AppState>, mode: String) -> Result<(), ethereon_core::Error> {
    if mode != "wt" && mode != "tab" {
        return Err(ethereon_core::Error::Invalid(format!("unknown place: {mode}")));
    }
    *s.hotkey.open_in.lock().unwrap() = mode;
    crate::save_config(&app);
    Ok(())
}

/// `None` lo apaga. Se guarda en config.json (no afecta la seguridad).
#[tauri::command]
pub fn hotkey_set(app: AppHandle, s: tauri::State<AppState>, combo: Option<String>) -> Result<(), ethereon_core::Error> {
    let old = s.hotkey.combo.lock().unwrap().clone();
    if let Err(e) = register(&app, combo.as_deref()) {
        let _ = register(&app, old.as_deref()); // si el nuevo no se pudo, queda el de antes
        return Err(ethereon_core::Error::Invalid(e));
    }
    *s.hotkey.combo.lock().unwrap() = combo;
    crate::save_config(&app);
    Ok(())
}

/// La carpeta de la ventana activa: una del Explorador (la pestaña que se ve) o el
/// Escritorio. `None` si es otra cosa o una ubicacion sin ruta (Este equipo, la Papelera).
#[cfg(windows)]
pub fn active_folder() -> Option<PathBuf> {
    use windows::core::{w, Interface};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, IServiceProvider, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Shell::{
        IFolderView, IPersistFolder2, IShellBrowser, IShellWindows, IWebBrowser2, SHGetNameFromIDList, SID_STopLevelBrowser,
        SIGDN_FILESYSPATH, ShellWindows,
    };
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetClassNameW, GetForegroundWindow};

    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return None;
        }
        let mut buf = [0u16; 64];
        let n = GetClassNameW(fg, &mut buf);
        let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);

        if class == "Progman" || class == "WorkerW" {
            return desktop_dir();
        }
        if class != "CabinetWClass" {
            return None;
        }

        // En el hilo de la interfaz COM ya esta iniciado; si no, se inicia aca.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        // Windows 11 tiene pestañas: todas comparten la ventana, la visible es la primera
        // ShellTabWindowClass.
        let tab = FindWindowExW(Some(fg), None, w!("ShellTabWindowClass"), None).ok();
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).ok()?;
        for i in 0..windows.Count().ok()? {
            let Ok(disp) = windows.Item(&VARIANT::from(i)) else { continue };
            let Ok(browser) = disp.cast::<IWebBrowser2>() else { continue };
            let Ok(hwnd) = browser.HWND() else { continue };
            if hwnd.0 != fg.0 as isize {
                continue;
            }
            let Ok(sp) = browser.cast::<IServiceProvider>() else { continue };
            let Ok(shell) = sp.QueryService::<IShellBrowser>(&SID_STopLevelBrowser) else { continue };
            if let (Some(tab), Ok(w)) = (tab, shell.GetWindow()) {
                if w != tab {
                    continue;
                }
            }
            let Ok(view) = shell.QueryActiveShellView() else { continue };
            let Ok(fv) = view.cast::<IFolderView>() else { continue };
            let Ok(folder) = fv.GetFolder::<IPersistFolder2>() else { continue };
            let Ok(pidl) = folder.GetCurFolder() else { continue };
            let name = SHGetNameFromIDList(pidl, SIGDN_FILESYSPATH);
            CoTaskMemFree(Some(pidl as _));
            let p = name.ok()?;
            let out = p.to_string().ok().map(PathBuf::from);
            CoTaskMemFree(Some(p.0 as _));
            return out.filter(|p| p.is_dir());
        }
        None
    }
}

#[cfg(not(windows))]
pub fn active_folder() -> Option<PathBuf> {
    None
}

/// El Escritorio de la persona (sigue la carpeta aunque este en OneDrive).
#[cfg(windows)]
pub fn desktop_dir() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_Desktop, KF_FLAG_DEFAULT};
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
        let out = p.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(p.0 as _));
        out.filter(|d| d.is_dir())
    }
}

#[cfg(not(windows))]
pub fn desktop_dir() -> Option<PathBuf> {
    None
}
