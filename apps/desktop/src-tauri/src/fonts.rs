//! Fuentes para la terminal: las instaladas en Windows y la que usa Windows Terminal, para
//! que la terminal de Ethereon pueda verse igual. La fuente la elige la terminal, no la
//! shell: el tema de PowerShell no la puede cambiar.

use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Palabras de estilo que el registro agrega al nombre ("Consolas Bold Italic").
const STYLES: &[&str] = &[
    "regular", "bold", "italic", "oblique", "light", "semilight", "semibold", "demibold", "extralight", "ultralight",
    "extrabold", "ultrabold", "black", "heavy", "medium", "thin", "book", "condensed", "semicondensed", "narrow",
];

fn family(name: &str) -> String {
    // "Cascadia Mono (TrueType)" -> "Cascadia Mono"; "Consolas Bold Italic" -> "Consolas"
    let mut n = name.split(" (").next().unwrap_or(name).trim().to_string();
    loop {
        let Some((head, last)) = n.rsplit_once(' ') else { break };
        if STYLES.contains(&last.to_ascii_lowercase().as_str()) {
            n = head.trim().to_string();
        } else {
            break;
        }
    }
    n
}

/// Familias instaladas (para todos y para este usuario), ordenadas.
#[tauri::command]
pub fn system_fonts() -> Vec<String> {
    let mut out = BTreeSet::new();
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        use winreg::RegKey;
        const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";
        for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
            if let Ok(k) = RegKey::predef(root).open_subkey(KEY) {
                for (name, _) in k.enum_values().flatten() {
                    for part in name.split(" & ") {
                        let f = family(part);
                        if !f.is_empty() && !f.starts_with('@') {
                            out.insert(f);
                        }
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}

#[derive(Serialize)]
pub struct WtFont {
    /// Como lo escribe Windows Terminal: una o varias familias separadas por coma.
    face: String,
    /// En puntos (12 pt = 16 px).
    size: f64,
}

fn wt_settings() -> Option<PathBuf> {
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    [
        local.join(r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json"),
        local.join(r"Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json"),
        local.join(r"Microsoft\Windows Terminal\settings.json"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// La fuente del perfil por defecto de Windows Terminal (o la de todos los perfiles).
#[tauri::command]
pub fn wt_font() -> Option<WtFont> {
    let text = std::fs::read_to_string(wt_settings()?).ok()?;
    // settings.json admite comentarios de linea
    let clean: String = text.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    let v: serde_json::Value = serde_json::from_str(&clean).ok()?;
    let default = v.get("defaultProfile").and_then(|d| d.as_str()).unwrap_or("");
    let profiles = v.get("profiles")?;
    let own = profiles
        .get("list")
        .and_then(|l| l.as_array())
        .and_then(|l| l.iter().find(|p| p.get("guid").and_then(|g| g.as_str()) == Some(default)))
        .and_then(|p| p.get("font"));
    let base = profiles.get("defaults").and_then(|d| d.get("font"));
    let pick = |k: &str| own.and_then(|f| f.get(k)).or_else(|| base.and_then(|f| f.get(k))).cloned();
    let face = pick("face").and_then(|f| f.as_str().map(String::from)).unwrap_or_else(|| "Cascadia Mono".into());
    let size = pick("size").and_then(|s| s.as_f64()).unwrap_or(12.0);
    Some(WtFont { face, size })
}

#[cfg(test)]
mod tests {
    #[test]
    fn family_names() {
        assert_eq!(super::family("Consolas Bold Italic (TrueType)"), "Consolas");
        assert_eq!(super::family("PxPlus IBM VGA 8x16 (TrueType)"), "PxPlus IBM VGA 8x16");
        assert_eq!(super::family("Hack Nerd Font Mono Regular"), "Hack Nerd Font Mono");
    }
}
