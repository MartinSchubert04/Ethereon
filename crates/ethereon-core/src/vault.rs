//! Archivo del vault: AES-256-GCM con una clave derivada de la contrasena maestra (Argon2id).
//!
//! Formato (JSON): `{ v, kdf: {m, t, p, salt}, nonce, data }`. `data` es el VaultData
//! serializado y cifrado. Cada guardado usa un nonce nuevo. La cabecera va como datos
//! asociados, asi que cambiar los parametros a mano rompe el descifrado.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::Zeroizing;

use crate::model::VaultData;
use crate::Error;

pub type Key = Zeroizing<[u8; 32]>;

#[derive(Clone, Serialize, Deserialize)]
struct Kdf {
    m: u32,
    t: u32,
    p: u32,
    salt: String,
}

#[derive(Serialize, Deserialize)]
struct File {
    v: u32,
    kdf: Kdf,
    nonce: String,
    data: String,
}

pub fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("sin fuente de azar del sistema");
    b
}

fn derive(password: &str, kdf: &Kdf) -> Result<Key, Error> {
    let salt = B64.decode(&kdf.salt).map_err(|_| Error::Corrupt)?;
    let params = argon2::Params::new(kdf.m, kdf.t, kdf.p, Some(32)).map_err(|_| Error::Corrupt)?;
    let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    a.hash_password_into(password.as_bytes(), &salt, key.as_mut()).map_err(|_| Error::Corrupt)?;
    Ok(key)
}

fn aad(kdf: &Kdf) -> Vec<u8> {
    format!("ethereon-vault-v1:{}:{}:{}:{}", kdf.m, kdf.t, kdf.p, kdf.salt).into_bytes()
}

/// Un vault abierto: la clave derivada y los parametros para volver a guardar.
pub struct Unlocked {
    key: Key,
    kdf: Kdf,
}

pub fn exists(path: &Path) -> bool {
    path.exists()
}

/// Crea un vault nuevo con la contrasena maestra.
pub fn create(path: &Path, password: &str, mut data: VaultData) -> Result<Unlocked, Error> {
    if password.chars().count() < 8 {
        return Err(Error::Invalid("la contrasena maestra necesita al menos 8 caracteres".into()));
    }
    // 64 MiB y 3 pasadas: ~0.3 s en una PC comun, caro para probar contrasenas.
    let kdf = Kdf { m: 64 * 1024, t: 3, p: 1, salt: B64.encode(random::<16>()) };
    let key = derive(password, &kdf)?;
    if data.audit_key.is_empty() {
        data.audit_key = hex::encode(random::<32>());
    }
    let u = Unlocked { key, kdf };
    u.save(path, &data)?;
    Ok(u)
}

pub fn open(path: &Path, password: &str) -> Result<(Unlocked, VaultData), Error> {
    let raw = std::fs::read_to_string(path)?;
    let f: File = serde_json::from_str(&raw).map_err(|_| Error::Corrupt)?;
    let key = derive(password, &f.kdf)?;
    let nonce = B64.decode(&f.nonce).map_err(|_| Error::Corrupt)?;
    let ct = B64.decode(&f.data).map_err(|_| Error::Corrupt)?;
    if nonce.len() != 12 {
        return Err(Error::Corrupt);
    }
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).unwrap();
    let pt = cipher
        .decrypt(Nonce::from_slice(&nonce), Payload { msg: &ct, aad: &aad(&f.kdf) })
        .map_err(|_| Error::WrongPassword)?;
    let pt = Zeroizing::new(pt);
    let data: VaultData = serde_json::from_slice(&pt).map_err(|_| Error::Corrupt)?;
    Ok((Unlocked { key, kdf: f.kdf }, data))
}

impl Unlocked {
    pub fn save(&self, path: &Path, data: &VaultData) -> Result<(), Error> {
        let pt = Zeroizing::new(serde_json::to_vec(data).unwrap());
        let nonce = random::<12>();
        let cipher = Aes256Gcm::new_from_slice(self.key.as_ref()).unwrap();
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: &pt, aad: &aad(&self.kdf) })
            .map_err(|_| Error::Corrupt)?;
        let f = File { v: 1, kdf: self.kdf.clone(), nonce: B64.encode(nonce), data: B64.encode(ct) };
        // Se escribe al lado y se renombra: un corte de luz no deja el vault a medias.
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&f).unwrap())?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Cambia la contrasena maestra (sal nueva, misma data).
    pub fn rekey(&mut self, path: &Path, password: &str, data: &VaultData) -> Result<(), Error> {
        let fresh = create(path, password, data.clone())?;
        *self = fresh;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_wrong_password() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("vault.json");
        let mut d = VaultData::default();
        d.denylist.push("shutdown".into());
        create(&p, "correct horse", d).unwrap();
        let (_, back) = open(&p, "correct horse").unwrap();
        assert_eq!(back.denylist, vec!["shutdown"]);
        assert!(matches!(open(&p, "wrong pass"), Err(Error::WrongPassword)));
        // El archivo no contiene nada en claro.
        let raw = std::fs::read_to_string(&p).unwrap();
        assert!(!raw.contains("shutdown"));
    }
}
