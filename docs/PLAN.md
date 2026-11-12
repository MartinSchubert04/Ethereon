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
| 4 | Interfaz completa con terminal | Svelte 5: hosts por grupos tipo Termius, keychain, reglas con "probar un comando", aprobaciones, agentes, auditoría con verificación y exportación, ajustes. Terminal xterm.js por PTY. Cuatro temas (Default, Warp, Poolsuite FM, Haze) con 25 paletas. Barra de título propia con pestañas al estilo Termius, atajos de teclado, y la ventana vuelve con la misma posición, tamaño y estado maximizado. Importar `~/.ssh/config`, archivos de clave y Bitwarden; exportar OpenSSH (Termius), Bitwarden, CSV, JSON y respaldo cifrado | `svelte-check` limpio. Capturas de cada vista y de los cuatro temas manejando el WebView por DevTools. Terminal probada por el bastión | Hecho |
| 5 | Publicación | Repo en GitHub, CI (`desktop.yml`: tests, tipos y build), Release por tag con instalador firmado y actualización automática (`updater.rs`) | El primer push corre el CI completo | Hecho. El secret `TAURI_SIGNING_PRIVATE_KEY` está cargado y las releases publican `latest.json` |
| 6 | Consola serial | `serial.rs`: segundo transporte (cable de consola de switches, APs, routers). `Host::transport` (SSH o Serial, los vaults viejos se leen como SSH) con puerto, baudios, 8N1, patrón de prompt, timeout y credenciales de login y `enable`. `exec` por prompt (sin exit code: lee hasta el prompt, descarta el eco, contesta `--More--`), un puerto compartido por host entre la terminal y el agente, cerrado al bloquear. Mismas reglas, aprobaciones, enmascarado y auditoría; lista base de solo lectura para equipos de red (`show`, `display`, `get`...), reemplazable por la del host o grupo. Formulario con selector SSH / Serial y lista de puertos; terminal en crudo | Tests de `serial` sobre un puerto falso en memoria (prompt, paginadores, login, `enable`, timeout, truncado) y uno de punta a punta en `broker` (login + `enable` + comando enmascarado, solo lectura, sin puerto en `list_hosts`). **Sin probar con hardware real**: ver "Pendientes" | Hecho el código; falta probarlo con un cable |

### Pendientes

Ordenados por cuánto aportan contra cuánto cuestan.

| Qué | Etapa | Notas |
|---|---|---|
| Probar la consola serial con un equipo real | 6 | Se escribió sin hardware: hay que enchufar un cable a un switch y revisar (a) la lista de puertos y la terminal en crudo, (b) `run` del agente con login, `enable` y `--More--`, (c) que escribir desde la terminal mientras el agente corre un comando no lo rompa (comparten el puerto; el comando del agente toma el turno pero las teclas de la persona también llegan), (d) la demora de las teclas en Windows (el puerto atiende de a una operación: una escritura espera hasta 50 ms a la lectura en curso, `serial::READ_WAIT`). Lo que no se ajusta a un equipo (prompt raro, paginador distinto) se arregla en `serial::PAGERS` o con el patrón de prompt del host |
| Consola serial: que la terminal avise si el login automático falló | 6 | Al conectar, `test_host` intenta el login y `enable` (`serial::ready`) y la consola se abre igual aunque no haya prompt (equipo apagado, arrancando); hoy el motivo queda solo en "Show logs" |
| Consola serial: comandos que piden confirmación (`reload`, `write erase`) | 6 | Vencen el timeout. Si hace falta, una lista de respuestas a prompts (`[confirm]` → Enter) configurable por host |

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
| Terminal: salida por un `Channel` de Tauri | Eventos globales | Va solo a la ventana que la pidió y, si la ventana se cerró, el envío falla y la sesión se corta sola |
| Salida de la terminal juntada cada 8 ms y en bytes crudos (`InvokeResponseBody::Raw`) | Un mensaje en base64 por cada lectura | La ConPTY entrega de a una línea y cada mensaje es un `eval` en el WebView: un `type` de 32 MB eran 440.000 mensajes y 4 minutos (release). Juntada: 8.000 mensajes y 66 s, lo mismo que Windows Terminal del mismo tamaño. Lo que llega tras un rato sin salida (el eco de una tecla) sale en el momento |
| La ConPTY de Windows Terminal (1.25) en vez de la de Windows | La del sistema | Deja pasar Sixel. Con salidas medianas (un `git log` de 90 KB) es ~30% más lenta que la del sistema (medido en noviembre de 2026) |
| Exportar con secretos pide la contraseña maestra otra vez | Exportar con el vault abierto | El que se sentó en la PC desbloqueada no se lleva todo |
| Ventana sin marco con barra propia (pestañas en la barra de título) | Marco de Windows y pestañas abajo | Es lo de Termius: una sola barra arriba. Se copió de Haze, incluido mover y agrandar con `SetWindowPos` (con `set_size` la ventana crecía cada vez que se abría) |
| En una terminal, Ctrl W va a la shell y la pestaña se cierra con Ctrl Shift W; bloquear es Ctrl Shift L | Ctrl W y Ctrl L siempre para la app | En bash, Ctrl W borra la palabra anterior y Ctrl L limpia la pantalla: cerrar la sesión sin querer cuesta más que un modificador |
| Paletas generadas por un script de Python | Escribirlas a mano en CSS | 25 paletas son cientos de variables; en el script cada una es una línea |
| Actualizaciones como Haze: `latest.json` en GitHub Releases, firmadas, de fondo | Avisar y que la persona baje el instalador | Es el flujo que ya funciona en Haze. Diferencia: solo se instala sola con la ventana cerrada, el vault bloqueado y sin comandos del agente, porque reiniciar bloquea el vault |
| Al actualizar a pedido con el vault abierto, la versión nueva arranca abierta: la vieja le pasa la clave a una copia suya en %TEMP%, que se la da por un named pipe solo al `ethereon.exe` instalado (`handoff.rs`) | Pedir la contraseña de nuevo; o guardar la clave en un archivo protegido con DPAPI | Pedirla justo después de estar adentro no tiene sentido. Un archivo DPAPI lo descifra cualquier programa del mismo usuario (el agente también); la clave en la memoria de la copia, unos segundos, no abre nada que la app abierta no tuviera ya |
| Select, Switch y Seg propios | `<select>` y casillas nativas | En Windows el `<select>` nativo no se puede estilar: rompía todos los temas |
| Portapapeles con `tauri-plugin-clipboard-manager` | `navigator.clipboard` | En el WebView leer el portapapeles pide permisos o falla según el origen; el plugin anda siempre |
| Atajos y opciones de la terminal en el almacenamiento del WebView | Dentro del vault | No afectan la seguridad, y así se pueden cambiar con el vault bloqueado |
| Argon2 y AES optimizados también en `dev` | Todo sin optimizar | Sin optimizar, desbloquear el vault con `npm run dev` tardaba varios segundos |
| Free = libertad total y Guarded = todo menos lo prohibido; los vaults viejos con "free" se leen como Guarded | Cambiar el significado de "free" en el lugar | Si no, al actualizar, hosts que estaban protegidos por la lista de prohibidos se quedaban sin ella sin aviso |
| Arrastrar hosts con eventos de mouse propios | Drag and drop del navegador | En Tauri el WebView le pasa el arrastre a Windows (para soltar archivos) y los eventos HTML5 no llegan |
| Tipografía Inter + Geist Mono en todos los temas salvo Athanor | Una fuente pixel para Poolsuite | La personalidad de cada tema va en colores, bordes y sombras; las letras quedan legibles |
| Serial: `Host` gana `transport` y `serial` con `serde(default)`, en vez de un enum `Host { Ssh(..), Serial(..) }` | Un enum etiquetado por host | El enum cambiaba el JSON del vault para todos los hosts (migración); con los campos nuevos un vault viejo se lee tal cual como SSH, y la interfaz sigue tratando a todos los hosts igual (nombre, grupo, reglas, icono) |
| Serial: un puerto por host compartido entre la terminal y el agente (`serial::Conn` con `tap`) | Que cada uno abra el puerto | Un puerto serie es exclusivo: si la terminal lo tiene, el agente no podría abrirlo. Compartido, la persona ve en vivo lo que hace el agente, como un `screen` compartido |
| Serial: el prompt se reconoce en la última línea después de un `\n`, y se espera 200 ms después de verlo | Cortar al primer `#` o `>` | Una línea de salida puede terminar en `#` (`show run` muestra comentarios); el eco del comando puede terminar en `$`. Con la espera, lo que sigue llegando descarta el falso prompt |
| Serial: en `list_hosts` el agente ve `transport: serial` | Que el host se vea idéntico a uno SSH | El agente tiene que saber que no hay shell (nada de pipes de `sh`, sin exit code) para mandar los comandos del equipo; el transporte no es un secreto, el puerto sí y no sale |
| Serial: `sudo` no existe; `enable` lo hace Ethereon antes del comando si el prompt termina en `>` | Que el agente mande `enable` como comando | El agente nunca ve la contraseña de `enable`, igual que la de sudo: Ethereon la escribe cuando el equipo la pide (`serial::ready`) |
| Serial: errores del puerto al agente sin detalle (`details in the connection log in Ethereon`) | Pasar el error del sistema | El texto de un error de apertura puede nombrar al puerto (`COM3`), que para el agente es como la IP; el detalle va al registro de conexión que solo ve la persona |
| Archivos del agente: `download` / `upload` por SFTP, prendido con `Policy::files` (apagado por defecto, `serde(default)`) y con su límite `max_file_kb`; se hereda del grupo como el resto de las reglas | `features` en el `Host` (el plan original) o un interruptor propio por host | Es una regla más de lo que puede hacer el agente: con la política se hereda del grupo, se edita en el mismo lugar (switch en el editor de reglas, del host y del grupo) y un vault viejo se lee apagado sin migración |
| Mismas reglas que `run`: modo del host, reglas `path:` globales y del host (sobre la ruta pedida, la resuelta por el servidor y la forma `~/`), aprobación, frecuencia compartida con `run` y auditoría con tamaño y hash. Solo lectura: solo bajar; Free: sin lista de prohibidos, como en `run` | Reglas propias para archivos | Un archivo es otra forma de leer o escribir: si `cat ~/.ssh/x` está prohibido, bajarlo también, y no hay una segunda lista que mantener |
| Bajar devuelve el texto enmascarado en la respuesta; no escribe en el disco de la PC, y los binarios se rechazan | Bajar a un archivo local (enmascarado o no) | Si se escribe en disco, el agente lo lee crudo y se saltea `mask.rs`; enmascarar al escribir no sirve para un binario, y escribir abre la puerta a pisar archivos de la PC. En la respuesta pasa por el mismo enmascarado que `run` |
| Subir: texto en el pedido o un archivo local; el local se resuelve (`canonicalize`) y no puede estar en la carpeta de Ethereon, en carpetas de credenciales (`~/.ssh`, `~/.aws`, DPAPI, perfiles de navegador...), tener nombre de clave (`id_rsa`, `*.pem`, `.env`) ni ser una ruta de red. Nada con un secreto del vault o una clave privada sale, en ningún modo | Leer cualquier archivo local, o solo el texto del pedido | El agente podría usar a Ethereon para leer lo que su propio sandbox no le deja (confused deputy); una ruta `\\servidor\...` haría que Windows mande la sesión. El chequeo del contenido frena el caso que importa aunque la lista de carpetas no lo cubra |
| Subir escribe un temporal en la carpeta ya resuelta y lo renombra; no sigue un enlace en el destino ni pisa sin `overwrite`, y conserva los permisos | Abrir el destino con `create` | Escribir sobre el archivo seguiría un enlace duro o simbólico hacia otro lado (`authorized_keys`); con el renombrado se reemplaza la entrada de la carpeta y nada más |
| Respuestas sin la ruta resuelta y errores de SFTP y de conexión genéricos | Pasar el error de `russh-sftp` o de `SshError` | La ruta resuelta lleva el usuario (`/home/tester`) y los errores de conexión la dirección; no hacen falta para que el agente corrija el pedido |
| Varias direcciones por host (`alt_addrs`, LAN y Tailscale): se abre el SSH por todas a la vez y gana la primera que muestra la huella confirmada | Medir el ping (ICMP) y elegir la de menor latencia | Muchos servidores no contestan ICMP, en Windows hace falta otra API, y el ping no dice si el puerto SSH anda. La carrera elige la más rápida de las que andan, una muerta no hace esperar, y otra máquina con la misma IP de LAN en otra red pierde porque su huella no coincide |
| La huella se confirma una vez, en la dirección principal, y vale para todas; una dirección puede tener la suya propia si se confirma un cambio desde ella | Una huella por dirección | Es la misma máquina: confirmar dos veces la misma huella no protege más. La propia por dirección cubre el caso en que una ruta presenta otra clave (Tailscale SSH sin acceso a las claves del sistema) |
| Las direcciones alternativas comparten los saltos del host | Saltos distintos por dirección | Con Tailscale no hace falta bastión desde afuera; si aparece ese caso se agrega después sin cambiar el formato |
