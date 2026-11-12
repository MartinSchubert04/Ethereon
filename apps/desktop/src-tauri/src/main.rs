// Sin consola en release: es una app de bandeja.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // La copia que guarda la clave del vault mientras se instala una actualizacion (ver
    // handoff.rs): no arranca la app.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--handoff") {
        return ethereon_lib::handoff::keeper(&args);
    }
    ethereon_lib::run()
}
