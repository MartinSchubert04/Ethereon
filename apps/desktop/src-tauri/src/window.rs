//! La ventana principal: sin marco de Windows (la barra la dibuja la interfaz) y vuelve
//! con la misma posicion, tamaño y estado maximizado que tenia al cerrarse.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::AppState;

/// Posicion y tamaño de afuera, en pixeles fisicos.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct WindowBox {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

/// Mientras nace el WebView se siguen atendiendo eventos: otro pedido la crearia dos veces.
static OPENING: AtomicBool = AtomicBool::new(false);

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    if OPENING.swap(true, Ordering::SeqCst) {
        return;
    }
    // Si el lugar guardado ya no esta en ninguna pantalla (monitor desconectado), se
    // conserva el tamaño y aparece centrada.
    let saved = app.state::<AppState>().window.lock().unwrap().filter(|b| b.width > 0 && b.height > 0);
    let place = saved.is_some_and(|b| on_screen(app, b.x, b.y));
    let mut b = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title("Ethereon")
        .inner_size(1200.0, 780.0)
        .min_inner_size(860.0, 560.0)
        .decorations(false)
        .shadow(true)
        .visible(false);
    if !place {
        b = b.center();
    }
    let built = b.build();
    OPENING.store(false, Ordering::SeqCst);
    let w = match built {
        Ok(w) => w,
        Err(e) => {
            log::error!("no se pudo abrir la ventana: {e}");
            return;
        }
    };

    // Primero la posicion y despues el tamaño: si el lugar guardado esta en otro monitor
    // con otra escala, Windows la reescala al llegar.
    if let Some(s) = saved {
        if place {
            place_outer(&w, Some((s.x, s.y)), None);
        }
        place_outer(&w, None, Some((s.width, s.height)));
        if !place {
            let _ = w.center();
        }
        if s.maximized {
            let _ = w.maximize();
        }
    }
    let _ = w.show();
    let _ = w.set_focus();

    // Moverla o agrandarla actualiza la memoria; el archivo se escribe al cerrarla.
    // Windows avisa que se movio antes de avisar que se maximizo, y esa posicion no es la
    // de la ventana normal: se guarda la anterior para deshacerlo.
    let app2 = app.clone();
    let before_move = Mutex::new(None::<(i32, i32)>);
    w.on_window_event(move |e| {
        let state = app2.state::<AppState>();
        let win = app2.get_webview_window("main");
        let maximized = || win.as_ref().and_then(|w| w.is_maximized().ok()).unwrap_or(false);
        let minimized = || win.as_ref().and_then(|w| w.is_minimized().ok()).unwrap_or(false);
        match e {
            WindowEvent::Moved(p) => {
                let mut prev = before_move.lock().unwrap();
                if maximized() || minimized() {
                    *prev = None;
                    return;
                }
                let mut g = state.window.lock().unwrap();
                let b = g.get_or_insert_with(Default::default);
                *prev = Some((b.x, b.y));
                (b.x, b.y) = (p.x, p.y);
            }
            WindowEvent::Resized(size) => {
                let max = maximized();
                let prev = before_move.lock().unwrap().take();
                let outer = win.as_ref().and_then(|w| w.outer_size().ok());
                let mut g = state.window.lock().unwrap();
                let b = g.get_or_insert_with(Default::default);
                if max || size.width == 0 || size.height == 0 {
                    if let Some((x, y)) = prev {
                        (b.x, b.y) = (x, y);
                    }
                    if max {
                        b.maximized = true;
                    }
                } else if let Some(o) = outer {
                    (b.width, b.height, b.maximized) = (o.width, o.height, false);
                }
            }
            WindowEvent::Destroyed => crate::save_config(&app2),
            _ => {}
        }
    });
}

fn on_screen(app: &AppHandle, x: i32, y: i32) -> bool {
    let (px, py) = (x + 40, y + 20);
    app.available_monitors().unwrap_or_default().iter().any(|m| {
        let (p, sz) = (m.position(), m.size());
        px >= p.x && py >= p.y && px < p.x + sz.width as i32 && py < p.y + sz.height as i32
    })
}

/// Mueve o agranda la ventana por afuera, en pixeles fisicos.
///
/// No se usa `set_size`: fija el tamaño de adentro y, recien creada y oculta, la ventana
/// todavia tiene el marco de Windows; sumaba la barra de titulo cada vez que se abria.
fn place_outer(w: &tauri::WebviewWindow, pos: Option<(i32, i32)>, size: Option<(u32, u32)>) {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER};
        let Ok(hwnd) = w.hwnd() else { return };
        let (x, y) = pos.unwrap_or_default();
        let (cx, cy) = size.unwrap_or_default();
        let mut flags = SWP_NOZORDER | SWP_NOACTIVATE;
        if pos.is_none() {
            flags |= SWP_NOMOVE;
        }
        if size.is_none() {
            flags |= SWP_NOSIZE;
        }
        unsafe {
            let _ = SetWindowPos(HWND(hwnd.0), None, x, y, cx as i32, cy as i32, flags);
        }
    }
    #[cfg(not(windows))]
    {
        if let Some((x, y)) = pos {
            let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
        }
        if let Some((width, height)) = size {
            let _ = w.set_size(tauri::PhysicalSize::new(width, height));
        }
    }
}

// ---- ventanas de terminales
//
// Una pestaña (o varias) se puede sacar a su propia ventana. La ventana nueva carga la misma
// interfaz con `?window=term`, pide sus pestañas con `term_window_take` y toma las sesiones
// con `term_attach`: las terminales siguen sin cortarse.

static TERM_WINDOWS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Pestañas esperando a que su ventana termine de cargar, por etiqueta de ventana.
#[derive(Default)]
pub struct PendingTabs(pub Mutex<std::collections::HashMap<String, serde_json::Value>>);

#[tauri::command]
pub async fn term_window_open(
    app: AppHandle,
    pending: tauri::State<'_, PendingTabs>,
    tabs: serde_json::Value,
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    place: Option<WindowBox>,
) -> Result<String, ethereon_core::Error> {
    let n = TERM_WINDOWS.fetch_add(1, Ordering::SeqCst) + 1;
    let label = format!("term-{n}");
    pending.0.lock().unwrap().insert(label.clone(), tabs);
    let mut b = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html?window=term".into()))
        .title("Ethereon")
        .inner_size(width.unwrap_or(1000.0), height.unwrap_or(640.0))
        .min_inner_size(560.0, 340.0)
        .decorations(false)
        .shadow(true);
    // Donde se solto la pestaña (coordenadas de pantalla, logicas).
    b = match (x, y) {
        (Some(x), Some(y)) => b.position((x - 120.0).max(0.0), (y - 20.0).max(0.0)),
        _ => b.center(),
    };
    // Vuelve a donde estaba (despues de actualizar): oculta hasta acomodarla.
    if place.is_some() {
        b = b.visible(false);
    }
    let w = b.build().map_err(|e| ethereon_core::Error::Invalid(format!("could not open the window: {e}")))?;
    if let Some(p) = place.filter(|p| p.width > 0 && p.height > 0) {
        if on_screen(&app, p.x, p.y) {
            place_outer(&w, Some((p.x, p.y)), None);
        }
        place_outer(&w, None, Some((p.width, p.height)));
        if p.maximized {
            let _ = w.maximize();
        }
    }
    if place.is_some() {
        let _ = w.show();
    }
    Ok(label)
}

#[tauri::command]
pub fn term_window_take(window: tauri::WebviewWindow, pending: tauri::State<'_, PendingTabs>) -> Option<serde_json::Value> {
    pending.0.lock().unwrap().remove(window.label())
}

/// Las ventanas abiertas a las que se puede mandar una pestaña.
#[tauri::command]
pub fn term_windows(app: AppHandle) -> Vec<String> {
    let mut out: Vec<String> = app.webview_windows().keys().filter(|l| *l == "main" || l.starts_with("term-")).cloned().collect();
    out.sort();
    out
}

/// Deja pestañas para otra ventana (se suman a las que ya esperaban); despues se le avisa
/// con el evento "take-tabs". La principal las toma tambien al abrirse.
#[tauri::command]
pub fn term_window_give(pending: tauri::State<'_, PendingTabs>, label: String, tabs: serde_json::Value) {
    let mut p = pending.0.lock().unwrap();
    let list = p.entry(label).or_insert_with(|| serde_json::Value::Array(vec![]));
    if let (Some(l), Some(new)) = (list.as_array_mut(), tabs.as_array()) {
        l.extend(new.iter().cloned());
    }
}

/// Muestra la ventana principal (la crea si estaba cerrada en la bandeja).
#[tauri::command]
pub fn main_window_show(app: AppHandle) {
    let h = app.clone();
    let _ = app.run_on_main_thread(move || show_main(&h));
}

/// Se esta arrastrando una pestaña de otra ventana por encima de `label`: le manda donde
/// esta el cursor, en coordenadas de esa ventana ("drag-hover" con {x, y}), para que muestre
/// la pestaña de abajo. Sin cursor (`leave`), le avisa que se fue sin soltar (null).
#[tauri::command]
pub fn drag_hover(app: AppHandle, label: String, leave: bool) {
    use tauri::Emitter;
    let payload = if leave {
        None
    } else {
        let (Some(w), Ok(c)) = (app.get_webview_window(&label), app.cursor_position()) else { return };
        let (Ok(p), Ok(s)) = (w.inner_position(), w.scale_factor()) else { return };
        Some(serde_json::json!({ "x": (c.x - p.x as f64) / s, "y": (c.y - p.y as f64) / s }))
    };
    let _ = app.emit_to(label.as_str(), "drag-hover", payload);
}

/// La otra ventana de Ethereon que esta bajo el cursor (para soltar ahi una pestaña), o
/// ninguna. Usa la posicion real del cursor: las coordenadas del WebView no sirven entre
/// monitores con distinta escala.
#[tauri::command]
pub fn window_at(app: AppHandle, except: String) -> Option<String> {
    let c = app.cursor_position().ok()?;
    let mut wins: Vec<_> = app
        .webview_windows()
        .into_iter()
        .filter(|(l, _)| *l != except && (l == "main" || l.starts_with("term-")))
        .collect();
    // La que tiene el foco primero, por si se superponen.
    wins.sort_by_key(|(_, w)| !w.is_focused().unwrap_or(false));
    wins.into_iter()
        .find(|(_, w)| {
            if !w.is_visible().unwrap_or(false) || w.is_minimized().unwrap_or(false) {
                return false;
            }
            let (Ok(p), Ok(s)) = (w.outer_position(), w.outer_size()) else { return false };
            c.x >= p.x as f64 && c.x < (p.x + s.width as i32) as f64 && c.y >= p.y as f64 && c.y < (p.y + s.height as i32) as f64
        })
        .map(|(l, _)| l)
}
