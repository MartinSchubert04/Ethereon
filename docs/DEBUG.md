# Modo debug de la interfaz

Un panel para ver qué pasa con las pestañas y las terminales cuando algo se ve mal y no se puede
reproducir en un navegador común (cosas del WebView2). Está en `apps/desktop/src/lib/debug.svelte.ts`
y solo aparece con `npm run dev` (o en la app instalada con `localStorage["ethereon.debug"] = "1"`).
En un release no hace nada.

## Levantarlo al lado de la app instalada

Con `ETHEREON_DATA_DIR`, la copia de prueba usa otro vault y no se pasa a la instancia ya abierta
(se saltea el single-instance, ver `src-tauri/src/lib.rs`). Con `ETHEREON_PORT`, el MCP escucha en
otro puerto. Desde la raíz del repo:

```powershell
$env:ETHEREON_DATA_DIR = "$env:TEMP\ethereon-dev"; $env:ETHEREON_PORT = 47399; npm run dev
```

Pide crear una contraseña (vault nuevo, vacío). Para probar terminales alcanza con "This PC" (`Ctrl T`).

## El panel

Abajo a la derecha:

- **Mark (F9):** anota "se ve mal" con el estado completo de todas las terminales.
- **Redraw:** redibuja todas las terminales. Si con esto se arregla, es un problema de pintado.
- **Copy / Clear:** copia o limpia el registro.
- **Interruptores** `capture`, `focus`, `tips`, `webgl`: cada uno apaga algo para aislar una causa
  (el tooltip de cada botón dice qué hace). Se guardan en `localStorage["ethereon.dbgsw"]`.
- Arriba, en vivo, cada terminal visible: tamaño, filas x columnas, WebGL, cuántas veces dibujó y
  hace cuánto, sus canvas (con el color del medio de los 2D) y qué elemento queda encima del medio.

Se anota solo: apretar, arrastrar, soltar y hacer clic en pestañas; qué pestaña se ve; cuando una
terminal gana o pierde el foco; cambios de filas y columnas; foco, tamaño y visibilidad de la
ventana; pérdida del contexto WebGL; y los errores de JavaScript (`error`, `unhandledrejection`,
`console.error`). Para anotar algo más: `dbg("qué pasó", datos, true)` (con `true` saca fotos del
estado a los 80 ms y a los 700 ms).

## El registro en un archivo

Todo lo que anota el panel también va a `target/ui-debug.log`, por el canal de recarga en vivo de
vite (la CSP no deja hacer `fetch`; ver el plugin `debugLog` en `vite.config.ts`). Así se lee sin
copiar y pegar. Las pruebas de Playwright contra el mismo vite también escriben ahí.

## Casos resueltos

- **Terminal en negro al perder el foco** (clic en la pestaña actual, arrastrar una pestaña):
  pasaba solo con una imagen Sixel en pantalla (el splash de athanor). El addon de imágenes
  dibujaba en un canvas `desynchronized` y WebView2 dejaba la terminal en negro. Arreglado en
  `vite.config.ts` (`imageCanvasSynchronized`). Lo que lo mostró: el interruptor `focus`, y que el
  contador de dibujos no subía al perder el foco solo cuando había una imagen.
