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
