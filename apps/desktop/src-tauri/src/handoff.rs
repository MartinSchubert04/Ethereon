//! Al actualizar con el vault abierto, la version nueva arranca abierta: la vieja le pasa la
//! clave del vault, sin pedir la contrasena otra vez y sin escribirla nunca en disco.
//!
//! 1. `updater::apply` copia el ejecutable a %TEMP% (el de la instalacion tiene que quedar libre
//!    para que el instalador lo reemplace, y con otro nombre: el instalador cierra los
//!    `ethereon.exe` que encuentre) y lo arranca con `--handoff <tubo> <exe instalado>`. La
//!    clave va por su stdin, que solo ve el padre.
//! 2. Esa copia (`keeper`) espera en un named pipe hasta `WAIT` y se la entrega a un solo
//!    proceso: el que se conecte con la imagen del exe instalado, o sea la Ethereon nueva. A
//!    cualquier otro lo corta sin mandarle nada. Despues sale.
//! 3. La version nueva lee el nombre del tubo de relaunch.json, se conecta (`take`) y
//!    desbloquea con esa clave.
//!
//! Un programa del mismo usuario podria leer la memoria de la copia, igual que la de Ethereon
//! abierta: no es una puerta nueva. Un archivo protegido con DPAPI si lo seria (lo descifra
//! cualquier programa del mismo usuario, el agente incluido), por eso no se usa.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use ethereon_core::vault::Key;

/// Cuanto espera la copia a la version nueva (instalar tarda unos segundos; con margen).
const WAIT: Duration = Duration::from_secs(180);
const PREFIX: &str = "ethereon-handoff-";

fn pipe_path(pipe: &str) -> String {
    format!(r"\\.\pipe\{pipe}")
}

/// Arranca la copia que guarda la clave mientras se instala. Devuelve el nombre del tubo
/// (va en relaunch.json) y el proceso, para cerrarlo si la instalacion falla.
#[cfg(windows)]
pub fn spawn(key: Key) -> Option<(String, std::process::Child)> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    let exe = std::env::current_exe().ok()?;
    let id = hex(&ethereon_core::vault::random::<8>());
    let copy = std::env::temp_dir().join(format!("{PREFIX}{id}.exe"));
    std::fs::copy(&exe, &copy).ok()?;
    let pipe = format!("{PREFIX}{id}");
    let mut child = Command::new(&copy)
        .arg("--handoff")
        .arg(&pipe)
        .arg(&exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .ok()?;
    let sent = child.stdin.take().map(|mut s| s.write_all(key.as_ref()).is_ok()).unwrap_or(false);
    // El instalador cierra esta app enseguida: hay que dejar el tubo listo antes de seguir.
    if !sent || !wait_ready(&pipe, Duration::from_secs(5)) {
        let _ = child.kill();
        return None;
    }
    Some((pipe, child))
}

#[cfg(not(windows))]
pub fn spawn(_key: Key) -> Option<(String, std::process::Child)> {
    None
}

/// Espera a que el tubo exista sin conectarse (conectarse desde aca le daria la clave a esta
/// misma app, que tiene la imagen permitida).
#[cfg(windows)]
fn wait_ready(pipe: &str, max: Duration) -> bool {
    use windows::core::HSTRING;
    use windows::Win32::System::Pipes::WaitNamedPipeW;
    let name = HSTRING::from(pipe_path(pipe));
    let start = std::time::Instant::now();
    while start.elapsed() < max {
        // 0 = NMPWAIT_USE_DEFAULT_WAIT; falla enseguida si el tubo todavia no existe.
        if unsafe { WaitNamedPipeW(&name, 0) }.as_bool() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// `ethereon.exe --handoff <tubo> <exe permitido>`: lee la clave de stdin y se la entrega al
/// exe permitido. No arranca nada de la app.
pub fn keeper(args: &[String]) {
    let (Some(pipe), Some(allowed)) = (args.get(2), args.get(3)) else { return };
    let mut key = zeroize::Zeroizing::new([0u8; 32]);
    if std::io::stdin().read_exact(key.as_mut()).is_err() {
        return;
    }
    std::thread::spawn(|| {
        std::thread::sleep(WAIT);
        std::process::exit(0);
    });
    let _ = serve(pipe, Path::new(allowed), &key);
}

/// Atiende el tubo hasta que se conecte el exe permitido; le manda la clave y termina. A los
/// demas los corta sin mandar nada. Ok(true) si la entrego.
#[cfg(windows)]
pub fn serve(pipe: &str, allowed: &Path, key: &[u8; 32]) -> std::io::Result<bool> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE};
    use windows::Win32::Storage::FileSystem::{FlushFileBuffers, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_OUTBOUND};
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_WAIT,
    };

    let name = HSTRING::from(pipe_path(pipe));
    // FIRST_PIPE_INSTANCE: si otro ya creo un tubo con ese nombre, falla en vez de compartirlo.
    let h = unsafe {
        CreateNamedPipeW(
            &name,
            PIPE_ACCESS_OUTBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            64,
            64,
            0,
            None,
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    let out = loop {
        if let Err(e) = unsafe { ConnectNamedPipe(h, None) } {
            if e.code() != ERROR_PIPE_CONNECTED.to_hresult() {
                break Err(std::io::Error::other(e));
            }
        }
        let mut pid = 0u32;
        let ok = unsafe { GetNamedPipeClientProcessId(h, &mut pid) }.is_ok() && image_of(pid).is_some_and(|p| same_file(&p, allowed));
        if ok {
            let mut n = 0u32;
            let sent = unsafe { WriteFile(h, Some(key), Some(&mut n), None) }.is_ok() && n == 32;
            let _ = unsafe { FlushFileBuffers(h) };
            let _ = unsafe { DisconnectNamedPipe(h) };
            break Ok(sent);
        }
        let _ = unsafe { DisconnectNamedPipe(h) };
    };
    let _ = unsafe { CloseHandle(h) };
    out
}

#[cfg(not(windows))]
pub fn serve(_pipe: &str, _allowed: &Path, _key: &[u8; 32]) -> std::io::Result<bool> {
    Ok(false)
}

/// La ruta del ejecutable de un proceso.
#[cfg(windows)]
fn image_of(pid: u32) -> Option<PathBuf> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    let p = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let r = unsafe { QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) };
    let _ = unsafe { CloseHandle(p) };
    r.ok()?;
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

/// En la version nueva: toma la clave que dejo la anterior. None si no esta o no la dan.
pub fn take(pipe: &str) -> Option<Key> {
    let path = pipe_path(pipe);
    for _ in 0..30 {
        match std::fs::OpenOptions::new().read(true).open(&path) {
            Ok(mut f) => {
                let mut key = zeroize::Zeroizing::new([0u8; 32]);
                return f.read_exact(key.as_mut()).ok().map(|_| key);
            }
            // Ocupado o todavia no esta: se reintenta un rato.
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    None
}

/// Borra las copias de %TEMP% que ya terminaron (la que sigue corriendo no se deja borrar).
pub fn clean_copies() {
    let Ok(dir) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in dir.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(PREFIX) && name.ends_with(".exe") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn run(allowed: PathBuf) -> (bool, Option<Key>) {
        let pipe = format!("{PREFIX}test-{}", hex(&ethereon_core::vault::random::<6>()));
        let p = pipe.clone();
        let server = std::thread::spawn(move || serve(&p, &allowed, &[9u8; 32]));
        assert!(wait_ready(&pipe, Duration::from_secs(5)));
        let got = take(&pipe);
        if got.is_some() {
            return (server.join().unwrap().unwrap(), got);
        }
        (false, got)
    }

    /// Se la da al exe permitido (aca, el de la prueba) y a otro no.
    #[test]
    fn only_the_allowed_exe_gets_the_key() {
        let me = std::env::current_exe().unwrap();
        let (sent, got) = run(me);
        assert!(sent);
        assert_eq!(*got.unwrap(), [9u8; 32]);

        let (_, got) = run(PathBuf::from(r"C:\Program Files\Nope\ethereon.exe"));
        assert!(got.is_none(), "another program must not get the key");
    }
}
