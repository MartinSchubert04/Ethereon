# CLAUDE.md

Ethereon: gestor SSH tipo Termius con vault, que le da a Claude Code acceso por MCP sin darle nunca una
credencial. Tauri 2 + Svelte 5, con el broker en una crate de Rust aparte.

## Dónde está cada decisión

| Pregunta | Archivo |
|---|---|
| Qué pidió el usuario | `docs/spec.md` |
| Etapas, qué falta, alternativas descartadas | `docs/PLAN.md` |
| Cómo tiene que verse la interfaz | `DESIGN.md` |
| Qué hace cada módulo del broker | comentario de cabecera de cada archivo en `crates/ethereon-core/src/` |
| Cómo depurar la interfaz (modo debug, copia de prueba) | `docs/DEBUG.md` |

## Comandos

```powershell
npm run dev     # la app con recarga en vivo
npm run check   # svelte-check
cargo test      # tests del núcleo
```

Antes de dar un cambio por terminado: `cargo test` y `npm run check` sin errores ni avisos. Si el
cambio toca `ssh`, `broker`, `policy` o `mcp`, también las pruebas de aceptación (ver README).

## Versiones: commit, tag y push

Cada fix o feature terminado (tests y check en verde) se publica sin esperar a que lo pidan:
subir la versión, commitear, tagear y pushear `main` y el tag.

- **Qué número sube** (semver, `MAYOR.MENOR.PARCHE`):
  - `x.y.Z` (parche): arreglos y retoques chicos de algo que ya existía (un texto, un ancho,
    un bug).
  - `x.Y.0` (menor): algo nuevo que se puede usar (una opción, un ícono, una pantalla), sin
    romper nada.
  - `X.0.0` (mayor): rompe algo de lo que ya existe (el formato del vault sin migración, la
    API del MCP, quitar una función). Antes de subirla, preguntar.
- **Dónde está la versión** (las cinco iguales): `apps/desktop/package.json`,
  `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/Cargo.toml`,
  `crates/ethereon-core/Cargo.toml`, y las entradas de `ethereon` / `ethereon-core` en
  `Cargo.lock` y de `@ethereon/desktop` en `package-lock.json` (ojo con reemplazar a ciegas:
  otras dependencias pueden tener el mismo número).
- **Commit**: `vX.Y.Z: <resumen>` y en el cuerpo una línea por cambio, en español, terminando
  con la versión (ver `git log`). Tag `vX.Y.Z` sobre ese commit.
- Cambios que no son ni fix ni feature (docs, este archivo) se commitean y pushean sin tag.

## Estructura

```
crates/ethereon-core/src/   broker sin interfaz
  vault.rs                archivo cifrado (AES-256-GCM + Argon2id)
  model.rs                lo que hay dentro del vault
  shell.rs, policy.rs     parser de comandos y reglas
  mask.rs                 enmascarado de la salida
  audit.rs                registro con HMAC encadenado
  ssh.rs                  conexiones, saltos, huellas, exec, PTY
  broker.rs               une todo; lo único que usan la interfaz y el MCP
  mcp.rs                  servidor MCP (HTTP, JSON-RPC a mano)
  import.rs, export.rs    ssh config, claves, Bitwarden; OpenSSH, Bitwarden, CSV, JSON
  files.rs                SFTP y disco local para la persona (no para el agente)
crates/ethereon-core/tests/ pruebas de aceptación y los contenedores SSH de prueba
apps/desktop/src-tauri/   bandeja, ventana, comandos de la interfaz (commands.rs), terminal (SSH y local), files.rs (SFTP),
                          hotkey.rs (atajo global), conpty/ (ConPTY de Windows Terminal, ver su README)
apps/desktop/src/         interfaz: views/ (una por sección), components/, lib/, styles/
```

## Reglas del proyecto

- **El agente nunca llega a un secreto.** Nada de lo que devuelve `mcp.rs` o `Broker::agent_*` puede
  incluir IPs, usuarios, credenciales ni salida sin enmascarar. Lo que desbloquea, edita o exporta
  solo existe como comando de Tauri.
- **Todo lo que afecta la seguridad va dentro del vault** (reglas, huellas, dispositivos, ajustes),
  nunca en un archivo plano: el agente corre en la misma PC y podría editarlo.
- **Los comandos de Tauri son finitos.** La lógica va en `ethereon-core`, así la etapa 3 los reemplaza
  por HTTP sin reescribir nada.
- **Temas solo con variables.** Los colores viven en `src/styles/themes.css`; los componentes no
  tienen colores fijos. Un tema nuevo = un bloque ahí + una línea en `src/lib/themes.ts`.
- Docs y comentarios en español; textos de la interfaz en inglés, literales (ver `DESIGN.md`).
