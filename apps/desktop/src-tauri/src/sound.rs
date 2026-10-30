//! Sonidos de la app. Suenan desde aca y no desde la interfaz: con la ventana cerrada (en la
//! bandeja) no hay WebView, y es justo cuando mas hace falta el aviso.
//!
//! Son WAV de 16 bits embebidos en el exe. `PlaySound` no tiene volumen propio, asi que se
//! escalan las muestras de una copia antes de pasarsela.

use std::sync::Mutex;

/// Lo que esta sonando. Con `SND_ASYNC`, Windows lee de esta memoria mientras suena: no se
/// suelta hasta el proximo sonido, que antes corta el anterior.
static PLAYING: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Cabecera de un WAV PCM sin metadatos (los de `sounds/` se exportan asi).
const HEADER: usize = 44;

fn wav(name: &str) -> Option<&'static [u8]> {
    match name {
        "approval" => Some(include_bytes!("../sounds/approval.wav")),
        _ => None,
    }
}

/// Toca `name` con `volume` de 0 a 100 (0 = no suena). Vuelve enseguida; da `false` si
/// Windows no lo acepto.
pub fn play(name: &str, volume: u8) -> bool {
    let Some(src) = wav(name) else { return false };
    if volume == 0 || src.len() <= HEADER {
        return false;
    }
    // Al cuadrado: el oido no es lineal, y asi la mitad de la barra suena a la mitad.
    let gain = (volume.min(100) as f32 / 100.0).powi(2);
    let mut buf = src.to_vec();
    for s in buf[HEADER..].chunks_exact_mut(2) {
        let v = (i16::from_le_bytes([s[0], s[1]]) as f32 * gain) as i16;
        s.copy_from_slice(&v.to_le_bytes());
    }

    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FLAGS, SND_MEMORY, SND_NODEFAULT};
        let mut playing = PLAYING.lock().unwrap();
        unsafe {
            // Corta el anterior antes de soltar su memoria.
            let _ = PlaySoundW(PCWSTR::null(), None, SND_FLAGS(0));
            *playing = buf;
            PlaySoundW(PCWSTR(playing.as_ptr().cast()), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT).as_bool()
        }
    }
    #[cfg(not(windows))]
    {
        drop(buf);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_are_plain_pcm16() {
        let w = wav("approval").unwrap();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[36..40], b"data", "cabecera de 44 bytes, sin metadatos");
        assert_eq!(u16::from_le_bytes([w[20], w[21]]), 1, "PCM");
        assert_eq!(u16::from_le_bytes([w[34], w[35]]), 16, "16 bits");
    }

    /// Necesita placa de sonido: `cargo test -p ethereon -- --ignored plays`. Casi mudo.
    #[test]
    #[ignore]
    fn plays() {
        assert!(play("approval", 1));
        std::thread::sleep(std::time::Duration::from_millis(1200));
    }
}
