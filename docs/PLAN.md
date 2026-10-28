# Plan

Plan de construcción de Ethereon. Cada etapa tiene qué entrega, cómo se verifica y en qué estado está.
La spec original está en [spec.md](spec.md).

## Objetivo

Un gestor SSH al estilo Termius que también sirve de puerta para agentes. La app es la única que tiene
las credenciales y la que abre las conexiones. El agente (Claude Code por MCP) pide "corré este comando
en el host X" y recibe solo la salida, ya enmascarada. La persona usa la misma app como cliente SSH,
con terminal propia.

**Hecho significa:** `cargo test` pasa, las pruebas de aceptación de la spec pasan contra servidores
SSH reales (`tests/ssh-fixture`), `npm run check` no da errores ni avisos, y con la ventana cerrada
queda un solo proceso chico en la bandeja que sigue atendiendo al agente.

## Etapas

| # | Etapa | Entrega | Verificación | Estado |
|---|---|---|---|---|
| 1 | Vault, hosts, `run`, enmascarado y auditoría | `crates/ethereon-core`: `vault`, `model`, `ssh`, `mask`, `audit`, `mcp`, `broker`. Saltos por bastión, huellas TOFU, sudo por stdin, pool de conexiones, autobloqueo, interruptor general, tokens por dispositivo | 20 tests unitarios. `tests/acceptance.rs` contra dos contenedores (bastión + interno): las 5 pruebas de la spec, más MCP por HTTP real | Hecho |
| 2 | Reglas y aprobaciones | `shell` (parser de pipes, `;`, `$( )`, backticks, `sh -c`, `eval`, `find -exec`, `sudo`, `xargs`, llaves), `policy` (solo lectura, aprobación, libre, sin agente; lista global; límites de tiempo, salida y comandos por minuto), cola de aprobación con aprobar, rechazar, patrón por sesión y editar | Tests de `shell` y `policy`. Aprobación editada probada desde la interfaz con sudo y salto | Hecho |
| 3 | Modo servidor para varias PCs | Binario `ethereon-server` sin interfaz con el mismo `Broker`; HTTPS (rustls) con certificado propio o de Let's Encrypt; interfaz web mínima o la de escritorio conectada en remoto para desbloquear y aprobar; push al celular (ntfy) | Ver "Etapa 3" abajo | Pendiente. Ya está: tokens por dispositivo con vencimiento, hosts permitidos y revocación inmediata; el transporte MCP es HTTP |
| 4 | Interfaz completa con terminal | Svelte 5: hosts por grupos tipo Termius, keychain, reglas con "probar un comando", aprobaciones, agentes, auditoría con verificación y exportación, ajustes. Terminal xterm.js por PTY. Cuatro temas (Default, Warp, Poolsuite FM, Haze) con 25 paletas. Barra de título propia con pestañas al estilo Termius, atajos de teclado, y la ventana vuelve con la misma posición, tamaño y estado maximizado. Importar `~/.ssh/config`, archivos de clave y Bitwarden; exportar OpenSSH (Termius), Bitwarden, CSV, JSON y respaldo cifrado | `svelte-check` limpio. Capturas de cada vista y de los cuatro temas manejando el WebView por DevTools. Terminal probada por el bastión | Hecho lo básico. Falta lo de "Pendientes" |
| 5 | Publicación | Repo en GitHub, CI (`desktop.yml`: tests, tipos y build), Release por tag con instalador firmado y actualización automática (`updater.rs`) | El primer push corre el CI completo | Hecho; falta cargar el secret de la clave de firma |

### Pendientes

Ordenados por cuánto aportan contra cuánto cuestan.

| Qué | Etapa | Notas |
|---|---|---|
| Opcionales por host: sesión persistente, leer archivos, subir/bajar, túneles | 1 | Agregar `features` al `Host` y una herramienta MCP por cada una. Subir/bajar por SFTP (`russh-sftp`). Sesión persistente: una shell por sesión MCP que guarda `cwd` y variables |
| Proteger el vault con DPAPI (Windows) o Keychain (Mac) | 1 | Envolver la clave derivada con DPAPI para desbloquear sin escribir la contraseña; la contraseña maestra sigue siendo la llave de respaldo |
| Notificación al celular | 2 | POST a un tema de ntfy configurado en Ajustes |
| Grabar las sesiones de terminal | 4 | Guardar el flujo de salida (asciicast v2) junto a la auditoría, cifrado con la clave del vault |
| Importar desde Termius directamente | 4 | Termius no tiene un formato de exportación documentado; hoy se pasa por "Export → SSH config" o por el `~/.ssh/config` |
| Pestañas de terminal divididas y snippets | 4 | Lo que tiene Termius y no hace falta para la v1 |

### Etapa 3, cómo encararla

1. `apps/server`: `main.rs` que crea un `Broker` con un `Notifier` que loguea y manda push, y sirve
   `mcp::router` detrás de `axum-server` con rustls. Nada nuevo en `ethereon-core`.
2. Desbloqueo remoto: un endpoint `/ui` separado del de MCP, con su propio token de administrador y
   solo desde la red interna o un túnel (Tailscale). Nunca desde el token de un agente.
3. La app de escritorio gana un modo "conectarse a un broker": las mismas vistas, pero los comandos
   van por HTTPS en vez de por IPC. Por eso los comandos de la interfaz son finitos y sin lógica.
4. En el servidor, el vault puede usar la contraseña maestra al arrancar (pedida por la interfaz
   remota) en vez de DPAPI.

## Decisiones

| Decisión | Alternativa descartada | Motivo |
|---|---|---|
| Tauri 2 + Svelte 5 | Tauri + React (la spec) | Es la pila de Haze: misma forma de trabajar, menos JavaScript, y la ventana se destruye al cerrarla igual que allá |
| Rust con `russh` | Go con `x/crypto/ssh` | Un solo lenguaje con Tauri: el broker es una crate que la app enlaza, sin un segundo proceso |
| `russh` con `ring` | `aws-lc-rs` (el default) | `aws-lc-rs` necesita NASM para compilar en Windows |
| AES-256-GCM + Argon2id de RustCrypto | libsodium (la spec) | Puro Rust, sin bibliotecas de C en Windows. Mismos algoritmos que pide la spec |
| Hosts, reglas, huellas y ajustes dentro del vault cifrado | Un JSON en claro y solo los secretos cifrados | El agente corre en la misma PC: si las reglas fueran un archivo plano podría editarlas. AES-GCM además detecta cualquier cambio |
| MCP por HTTP local (127.0.0.1) con token | MCP por stdio con un proceso puente | Claude Code habla HTTP directo, no hay un segundo binario, y la etapa 3 es el mismo transporte por HTTPS |
| Protocolo MCP escrito a mano (`mcp.rs`, ~200 líneas) | SDK oficial (`rmcp`) | Son cuatro métodos JSON-RPC; el SDK de Rust cambia de API seguido y sumaría peso |
| Con el vault bloqueado, el MCP contesta "bloqueado" como error de herramienta | Responder 401 | Un 401 hace que Claude Code intente un login OAuth que no existe |
| sudo con `sudo -k -S -p ''` y la contraseña por stdin | `SUDO_ASKPASS`, o la contraseña en el comando | Nunca aparece en la línea de comando ni en variables. `-k` obliga a pedirla siempre, así nunca queda como entrada del comando |
| Reglas como líneas de texto (`rm -r\|-R -f`, `path:.ssh`) | Expresiones regulares o un JSON por regla | Se leen y se editan en un textarea; la sintaxis entra en seis líneas de ayuda |
| Lo que no se puede analizar (programa en una variable, pipe a una shell, heredocs) se rechaza en modo libre y solo lectura, y se marca en aprobación | Dejarlo pasar | Es la forma de esconder un comando prohibido |
| Auditoría en JSONL con HMAC encadenado | SQLite | Se lee con cualquier editor, se exporta copiando el archivo, y la cadena detecta líneas borradas o cambiadas |
| El autobloqueo cuenta solo el uso de la interfaz | Contar también los pedidos del agente | Si no, un agente podría mantener el vault abierto para siempre |
| Terminal: salida en base64 por un `Channel` de Tauri | Eventos globales | Va solo a la ventana que la pidió y, si la ventana se cerró, el envío falla y la sesión se corta sola |
| Exportar con secretos pide la contraseña maestra otra vez | Exportar con el vault abierto | El que se sentó en la PC desbloqueada no se lleva todo |
| Ventana sin marco con barra propia (pestañas en la barra de título) | Marco de Windows y pestañas abajo | Es lo de Termius: una sola barra arriba. Se copió de Haze, incluido mover y agrandar con `SetWindowPos` (con `set_size` la ventana crecía cada vez que se abría) |
| En una terminal, Ctrl W va a la shell y la pestaña se cierra con Ctrl Shift W; bloquear es Ctrl Shift L | Ctrl W y Ctrl L siempre para la app | En bash, Ctrl W borra la palabra anterior y Ctrl L limpia la pantalla: cerrar la sesión sin querer cuesta más que un modificador |
| Paletas generadas por un script de Python | Escribirlas a mano en CSS | 25 paletas son cientos de variables; en el script cada una es una línea |
| Actualizaciones como Haze: `latest.json` en GitHub Releases, firmadas, de fondo | Avisar y que la persona baje el instalador | Es el flujo que ya funciona en Haze. Diferencia: solo se instala sola con la ventana cerrada, el vault bloqueado y sin comandos del agente, porque reiniciar bloquea el vault |
| Tipografía Inter + Geist Mono en todos los temas | Una fuente pixel para Poolsuite | La personalidad de cada tema va en colores, bordes y sombras; las letras quedan legibles |
