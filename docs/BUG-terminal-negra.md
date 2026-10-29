# Bug abierto: la terminal queda en negro

> **Temporal.** Cuando esté arreglado se borra este archivo y todo lo marcado en
> [Qué borrar cuando esté arreglado](#qué-borrar-cuando-esté-arreglado).

## Síntoma

En la app instalada (WebView2), la terminal que se ve queda en negro:

- al hacer clic en la pestaña que ya está activa;
- mientras se arrastra una pestaña, y al soltarla sin ponerla sobre otra;
- al hacer clic en cualquier cosa que le saque el foco (por ejemplo, el texto del panel de debug).

Hacer clic en la terminal la arregla al instante. En un navegador común (Edge con Playwright y el
mock de Tauri) no pasa.

## Lo que se sabe

Lo que muestra el registro del modo debug:

- **Lo que dispara el bug es que la terminal pierde el foco.** La pestaña no tiene nada que ver: con
  el interruptor `focus` apagado, apretar una pestaña ya no le saca el foco a la terminal y deja de
  pasar. Hacer clic en el texto del panel (que también le saca el foco) la deja en negro igual.
- **El estado está bien:** la terminal sigue `visible`, con su tamaño, filas, columnas y canvas de
  siempre. No hay nada encima (`elementsFromPoint` en el medio da la propia `.xterm`) y no se
  perdió el contexto WebGL.
- **xterm no dibuja nada al perder el foco** en la app (el contador de dibujos no sube). En el
  navegador común sí dibuja al perder el foco (`handleBlur` del addon WebGL pide redibujar todo).
- No se registró ningún error de JavaScript (se capturan `error`, `unhandledrejection` y
  `console.error`).
- En la app aparece un canvas más que en el navegador: el del addon de imágenes (`ImageAddon`,
  ~1476x810), por el splash Sixel de PowerShell (athanor).

## Descartado

| Probado | Resultado |
|---|---|
| Redibujar la terminal después de cada clic o soltar en la barra | La arregla, pero se nota el parpadeo a negro |
| `new WebglAddon(true)` (`preserveDrawingBuffer`) | Sigue en negro |
| Interruptor `capture` (capturar el mouse recién al empezar a arrastrar) | Sigue en negro |
| Interruptor `tips` (sin tooltips) | Sigue en negro |
| Interruptor `webgl` (renderer DOM) | Sigue en negro (sin confirmar si se probó con una terminal nueva) |

## Próximos pasos

1. Ver el registro de "terminal: pierde foco" con el color del medio de cada canvas 2D. Si el del
   `ImageAddon` es opaco y negro, la capa de imágenes está tapando el texto.
2. Probar con una terminal sin splash (sin imagen Sixel) para confirmar o descartar el `ImageAddon`.
3. Revisar los parches de vite.config.ts sobre `addon-webgl` y `addon-image` (`pixelBold`,
   `legacyGlyphs`, `sixelLikeWindowsTerminal`).
4. Si es WebView2 y no tiene arreglo: que la pestaña no le saque el foco a la terminal (lo que hace
   hoy el interruptor `focus`) y redibujar al perder el foco.

## Modo debug

Solo existe con `npm run dev` (o con `localStorage["ethereon.debug"] = "1"`).

### Levantarlo al lado de la app instalada

Con `ETHEREON_DATA_DIR`, la copia de prueba usa otro vault y no se pasa a la instancia ya abierta
(se saltea el single-instance, ver `src-tauri/src/lib.rs`). Con `ETHEREON_PORT`, el MCP escucha en
otro puerto. Desde la raíz del repo:

```powershell
$env:ETHEREON_DATA_DIR = "$env:TEMP\ethereon-dev"; $env:ETHEREON_PORT = 47399; npm run dev
```

Pide crear una contraseña (vault nuevo, vacío). Para probar alcanza con terminales "This PC"
(`Ctrl T`).

### El panel

Abajo a la derecha:

- **Mark (F9):** anota "se ve mal" con el estado completo de todas las terminales.
- **Redraw:** redibuja todas las terminales. Si con esto vuelve, es un problema de pintado.
- **Copy / Clear:** copia o limpia el registro.
- **Interruptores** `capture`, `focus`, `tips`, `webgl`: cada uno apaga una sospecha (el tooltip de
  cada botón dice qué hace). Se guardan en `localStorage["ethereon.dbgsw"]`.
- Arriba se ve en vivo cada terminal visible: tamaño, filas x columnas, WebGL, cuántas veces dibujó
  y hace cuánto, los canvas y qué elemento queda encima del medio.

### El registro en un archivo

Todo lo que anota el panel también va a `target/ui-debug.log` (por el canal de recarga en vivo de
vite: la CSP no deja hacer `fetch`). Se puede leer sin copiar y pegar. Si hay pruebas de Playwright
contra el mismo vite, también escriben ahí con la etiqueta `[main]`.

## Qué borrar cuando esté arreglado

- `apps/desktop/src/lib/debug.svelte.ts` entero.
- `apps/desktop/src/main.ts`: `installDebug()` y su import.
- `apps/desktop/src/components/TermPane.svelte`: las llamadas `dbg*`, el import y `if (sw.webgl)`.
- `apps/desktop/src/components/Titlebar.svelte`: las llamadas `dbg`, el import y los `sw.focus` /
  `sw.capture` (dejar `setPointerCapture` en `tabDown` como estaba, salvo que el arreglo sea ese).
- `apps/desktop/src/lib/tooltip.ts`: `if (!sw.tips) return;` y su import.
- `apps/desktop/vite.config.ts`: el plugin `debugLog` y `appendFileSync`.
- Este archivo.

Se puede quedar: que `ETHEREON_DATA_DIR` saltee el single-instance (sirve para cualquier prueba al
lado de la app instalada).
