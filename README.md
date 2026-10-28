# Ethereon

Gestor SSH para Windows al estilo Termius, que también le da acceso a Claude Code sin darle ninguna
credencial. Queda en la bandeja y pesa poco con la ventana cerrada.

- **Vault cifrado:** contraseñas, claves privadas (con passphrase), certificados, contraseñas de sudo
  y tokens. AES-256-GCM con una clave derivada de la contraseña maestra (Argon2id). Se bloquea solo.
- **Hosts tipo Termius:** grupos, etiquetas, entorno (prod, test, dev), saltos por bastión
  encadenados y una descripción que el agente lee para tener contexto.
- **Terminal propia:** doble clic en un host y se abre una terminal (xterm.js) en la app.
- **Para Claude Code (MCP):** `list_hosts` y `run(host, comando)`. El agente ve nombres y
  descripciones, nunca IPs ni secretos. La salida vuelve con los secretos tapados con `***`.
- **sudo sin exponer la contraseña:** Ethereon la escribe por la entrada estándar del proceso.
- **Reglas:** por host o grupo, en modo solo lectura, con aprobación, libre o sin agente. La lista de
  prohibidos se aplica a cada parte del comando (pipes, `;`, `$( )`, backticks, `sh -c`, `sudo`...).
- **Aprobaciones:** aprobar, rechazar, editar el comando o permitir un patrón por el resto de la
  sesión. Aviso en el escritorio.
- **Huellas:** la primera conexión pide confirmar la huella del host; si cambia, se bloquea y avisa.
- **Auditoría:** cada pedido con dispositivo, host, comando, resultado y hash de la salida, en una
  cadena firmada que detecta si alguien la edita. Búsqueda y exportación a CSV.
- **Importar y exportar:** `~/.ssh/config`, archivos de clave y Bitwarden. Exporta a OpenSSH (lo que
  importa Termius), Bitwarden, CSV, JSON, o un respaldo cifrado.
- **Temas:** Default, Warp, Poolsuite FM y Haze, cada uno con varias paletas (Nord, Dracula, Tokyo
  Night, Solar Flare, Lavender, Platinum...).
- **Como Termius:** barra de arriba con pestañas (Vault + una por terminal), "New tab" con buscador
  de hosts, y la ventana vuelve con el mismo tamaño y lugar.
- **Atajos:** Ctrl T nueva pestaña, Ctrl W cerrar (Ctrl Shift W dentro de una terminal), Ctrl Tab /
  Ctrl Shift Tab, Ctrl 1…9, Ctrl B barra lateral, Ctrl F buscar, Ctrl , ajustes, Ctrl Shift L bloquear.

## Conectar Claude Code

En **Agents → Connect Claude Code** se crea un token y Ethereon muestra el comando para pegar:

```
claude mcp add --transport http --scope user ethereon http://127.0.0.1:47321/mcp --header "Authorization: Bearer eth_..."
```

El token se ve una sola vez; Ethereon guarda solo su hash. Se puede revocar al instante, y el botón
**Agent access** de la barra lateral (o de la bandeja) corta todo el acceso de golpe.

## Cómo está hecho

| Carpeta | Qué es |
|---|---|
| `crates/ethereon-core/` | El broker, sin interfaz: vault, SSH, reglas, enmascarado, auditoría y servidor MCP. Es lo que va a correr solo en un servidor en la etapa 3 |
| `apps/desktop/src-tauri/` | La app (Rust, Tauri 2): aloja el broker, la bandeja y la terminal |
| `apps/desktop/src/` | La interfaz (Svelte 5). Al cerrar la ventana se destruye el WebView entero |
| `docs/PLAN.md` | Etapas, qué falta y por qué se eligió cada cosa |
| `docs/spec.md` | La spec original |

## Desarrollo

Requiere Rust (con las Build Tools de Visual Studio) y Node 22 o más nuevo.

```
npm install
npm run dev            # la app con recarga en vivo
npm run check          # tipos de la interfaz
cargo test             # tests del núcleo
```

Pruebas de aceptación contra servidores SSH reales (necesita Docker):

```
bash crates/ethereon-core/tests/ssh-fixture/up.sh /tmp/ethereon-fixture
ETHEREON_TEST_KEY=/tmp/ethereon-fixture/key cargo test -p ethereon-core --test acceptance -- --ignored --nocapture
bash crates/ethereon-core/tests/ssh-fixture/down.sh
```

Para probar sin tocar el vault de verdad:

- `ETHEREON_DATA_DIR=<carpeta>` usa otra carpeta de datos.
- `ETHEREON_PORT=<puerto>` cambia el puerto del MCP (por defecto 47321, o lo que diga `config.json`).

Los datos viven en `%APPDATA%\app.ethereon.ssh`: `vault.json` (cifrado), `audit.jsonl` y `config.json`
(puerto y posición de la ventana).

## Publicar una versión

```
git tag v0.2.0
git push origin v0.2.0
```

El tag dispara `.github/workflows/desktop.yml`: compila el instalador, lo firma y lo publica en el
Release junto con `latest.json`. Las apps instaladas miran ese archivo al minuto de arrancar y cada 6
horas, bajan la versión nueva de fondo y la instalan solas cuando la ventana está cerrada, el vault
bloqueado y no hay comandos del agente en curso (actualizar reinicia la app y el vault queda
bloqueado). Si no se da, aparece **Update** en la barra de arriba.

La firma usa la clave de `~/.tauri/ethereon.key` (fuera del repo), cargada en el secret
`TAURI_SIGNING_PRIVATE_KEY` del repo. **Hay que guardarla en un lugar seguro:** si se pierde, las
apps instaladas no aceptan más actualizaciones y hay que reinstalar a mano. La clave pública está en
`tauri.conf.json`.
