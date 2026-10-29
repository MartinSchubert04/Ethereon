//! La sesion de la interfaz que se guarda antes de actualizar: vive cifrada en el vault,
//! sobrevive a bloquear y desbloquear, se entrega una sola vez y no sale en los exports.

use ethereon_core::export::{self, Format};
use ethereon_core::{Broker, Notifier};

struct Quiet;
impl Notifier for Quiet {
    fn event(&self, _: &str, _: serde_json::Value) {}
    fn notify(&self, _: &str, _: &str) {}
}

const PW: &str = "tester-pass-123";

#[tokio::test]
async fn ui_session_once_and_not_exported() {
    let dir = tempfile::tempdir().unwrap();
    let b = Broker::new(dir.path().to_path_buf(), std::sync::Arc::new(Quiet));
    b.create(PW).unwrap();

    let screen = "deploy@web-1:~$ cat .env\nTOKEN=abc";
    b.set_ui_session(serde_json::json!([{ "label": "main", "tabs": [{ "replay": screen }] }])).unwrap();

    // Como al actualizar: se bloquea, la app vuelve a abrir y se desbloquea.
    b.lock().await;
    let raw = std::fs::read_to_string(dir.path().join("vault.json")).unwrap();
    assert!(!raw.contains("TOKEN=abc"), "el texto de la terminal tiene que quedar cifrado");
    b.unlock(PW).unwrap();

    // El export JSON no la lleva.
    let out = dir.path().join("export.json");
    b.read(|d| export::write(d, Format::Json, &out)).unwrap().unwrap();
    assert!(!std::fs::read_to_string(&out).unwrap().contains("ui_session"));

    let got = b.take_ui_session().unwrap().expect("la sesion guardada");
    assert_eq!(got[0]["tabs"][0]["replay"], screen);
    assert!(b.take_ui_session().unwrap().is_none(), "se entrega una sola vez");
}
