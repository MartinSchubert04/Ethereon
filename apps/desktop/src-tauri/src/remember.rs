//! "Ask for the master password when Ethereon starts" apagado: la clave del vault queda en
//! `unlock.key`, cifrada con DPAPI de Windows para este usuario, y al arrancar (despues de
//! reiniciar la PC o cerrar la app) el vault se abre solo.
//!
//! Es una concesion que elige la persona, apagada por defecto: cualquier programa que corra
//! con su usuario (el agente incluido) puede descifrar ese archivo con DPAPI y abrir el vault.
//! Por eso la interfaz lo avisa y pide la contrasena maestra para apagarlo, como exportar con
//! secretos. Que el archivo exista es el ajuste: no hay otra marca que el agente pueda tocar
//! (borrarlo solo hace que la pida). Bloquear a mano o por inactividad no lo borra.

use std::path::{Path, PathBuf};

use ethereon_core::vault::Key;

const FILE: &str = "unlock.key";
/// Entropia extra de DPAPI: no agrega secreto, pero un blob de otro programa no sirve aca.
const ENTROPY: &[u8] = b"ethereon-vault-unlock-v1";

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Si la clave esta guardada (o sea, no pide la contrasena al arrancar).
pub fn is_on(dir: &Path) -> bool {
    path(dir).exists()
}

/// Guarda la clave cifrada con DPAPI (escribe al lado y renombra).
pub fn save(dir: &Path, key: &Key) -> std::io::Result<()> {
    let blob = protect(key.as_ref())?;
    let p = path(dir);
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, blob)?;
    std::fs::rename(&tmp, &p)
}

/// La clave guardada, si hay y se puede descifrar.
pub fn load(dir: &Path) -> Option<Key> {
    let blob = std::fs::read(path(dir)).ok()?;
    let plain = zeroize::Zeroizing::new(unprotect(&blob).ok()?);
    let mut key = zeroize::Zeroizing::new([0u8; 32]);
    if plain.len() != 32 {
        return None;
    }
    key.copy_from_slice(&plain);
    Some(key)
}

pub fn forget(dir: &Path) {
    let _ = std::fs::remove_file(path(dir));
}

#[cfg(windows)]
fn protect(data: &[u8]) -> std::io::Result<Vec<u8>> {
    dpapi(data, true)
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> std::io::Result<Vec<u8>> {
    dpapi(data, false)
}

#[cfg(windows)]
fn dpapi(data: &[u8], encrypt: bool) -> std::io::Result<Vec<u8>> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};

    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let entropy = CRYPT_INTEGER_BLOB { cbData: ENTROPY.len() as u32, pbData: ENTROPY.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB::default();
    let r = unsafe {
        if encrypt {
            CryptProtectData(&input, None, Some(&entropy), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        } else {
            CryptUnprotectData(&input, None, Some(&entropy), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        }
    };
    r.map_err(std::io::Error::other)?;
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe {
        // Lo descifrado se pisa antes de devolver la memoria a Windows.
        if !encrypt {
            std::ptr::write_bytes(out.pbData, 0, out.cbData as usize);
        }
        let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
    }
    Ok(bytes)
}

#[cfg(not(windows))]
fn protect(_: &[u8]) -> std::io::Result<Vec<u8>> {
    Err(std::io::Error::other("only on Windows"))
}

#[cfg(not(windows))]
fn unprotect(_: &[u8]) -> std::io::Result<Vec<u8>> {
    Err(std::io::Error::other("only on Windows"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Lo guardado vuelve igual, y sin archivo no hay clave.
    #[test]
    fn saves_and_loads_the_key() {
        let dir = std::env::temp_dir().join(format!("ethereon-remember-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!is_on(&dir) && load(&dir).is_none());
        save(&dir, &zeroize::Zeroizing::new([7u8; 32])).unwrap();
        assert!(is_on(&dir));
        // En disco no queda la clave en claro.
        assert!(!std::fs::read(path(&dir)).unwrap().windows(32).any(|w| w == [7u8; 32]));
        assert_eq!(*load(&dir).unwrap(), [7u8; 32]);
        forget(&dir);
        assert!(!is_on(&dir) && load(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
