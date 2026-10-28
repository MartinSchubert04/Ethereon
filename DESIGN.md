# DESIGN.md

Sistema de diseño de Ethereon. Formato: [awesome-design-md](https://github.com/VoltAgent/awesome-design-md)
(nueve secciones). Leer antes de tocar la interfaz.

## 1. Tema visual y atmósfera

Un cliente SSH de escritorio, tranquilo y denso como Termius: barra lateral a la izquierda, tarjetas
de hosts agrupadas, y la terminal como protagonista cuando está abierta. La seguridad se nota en los
estados (entorno, modo de reglas, aprobaciones pendientes, acceso del agente) y no en adornos.

- **Densidad:** media. Tarjetas de 250 px, tablas para listas largas (keychain, auditoría, dispositivos).
- **Voz:** literal. "Approve", "Reject", "Agent access", "Lock vault". Sin metáforas.
- **Cuatro temas, una estructura.** Cambian colores, bordes, sombras y fondos; nunca el layout ni las
  letras.

## 2. Colores y roles

Todos los colores son variables de `apps/desktop/src/styles/themes.css`.

| Variable | Rol |
|---|---|
| `--bg`, `--bg-image` | Fondo de la ventana (Warp y Haze usan gradientes, Poolsuite un atardecer) |
| `--sidebar`, `--panel`, `--panel-2` | Superficies, de más atrás a más adelante |
| `--line`, `--line-strong`, `--border` | Bordes y su grosor (2 px en Poolsuite) |
| `--text`, `--text-2`, `--text-3` | Texto principal, secundario y terciario |
| `--accent`, `--accent-ink`, `--primary-bg` | Acento, texto sobre el acento, botón principal (puede ser gradiente) |
| `--danger`, `--warn`, `--ok` | Rechazado o bloqueado; pendiente; permitido o activo |
| `--env-prod`, `--env-test`, `--env-dev` | Entorno del host (insignia y chip) |
| `--term-*` | Fondo, texto, cursor y selección de la terminal |

Cada tema tiene paletas (Settings → Palette). Se definen en `apps/desktop/scripts/palettes.py`, que
genera `styles/palettes.css` y `lib/palettes.gen.ts`; cada tema recuerda la suya.

| Tema | Paletas |
|---|---|
| Default | Mint, Nord, Dracula, Tokyo Night, Catppuccin, Gruvbox, Paper (clara) |
| Warp | Violet, Solar Flare, Pink City, Jellyfish, Leafy, Red Rock |
| Poolsuite FM | Sunset, Mint Pool, Tropicana, Lavender, Beach, Platinum (Mac OS 9), Monochrome (la app nueva de Poolsuite: crema y negro sobre papel tramado), Vellum |
| Haze | Cream, Peach, Rose, Mauve, Sky, Sage, Amber (acentos de Catppuccin, con la bruma teñida a juego) |
| Athanor | Srcery, Umber, Cinnabar, Vellum (clara), Orpiment (clara): las de portfolio-athanor |

Cada tema tiene además una paleta "a la athanor" (Srcery, Ember, Vellum, Amber): negro cálido,
crema y un solo acento ámbar.

| Tema | Fondo | Acento | Rasgo |
|---|---|---|---|
| Default | `#0d0f14` | menta `#6ee7b7` | Plano, sobrio |
| Warp | negro con luces violeta y cian | gradiente `#8b5cf6 → #22d3ee` | Paneles translúcidos con desenfoque |
| Poolsuite FM | atardecer pastel (cielo, rosa, durazno) | rosa `#ff4f9a` | Claro, bordes negros de 2 px, sombras duras, barra de ventana rayada como el Mac clásico |
| Haze | bruma cálida y fría sobre gris azulado | crema `#f9e2af` | Vidrio, igual que la app Haze |
| Athanor | `#1C1B19` (Srcery) | ámbar `#FBB829` | Pixel clay: esquinas en escalera, biseles duros, sombras tramadas en damero; fuente bitmap |

## 3. Tipografía

Inter Variable para todo y Geist Mono Variable para comandos, direcciones, huellas y la terminal.
Las dos vienen empaquetadas (sin red).

**Excepción: Athanor**, pedido así a propósito para que sea igual al portfolio. Todo el texto en
PxPlus IBM VGA 8x16 a 16 px exactos y sin suavizado (es bitmap: a otro tamaño se ve borrosa), sin
negrita ni cursiva; los títulos en Jacquard 24 a 48 px. Ver `styles/athanor.css` y `src/fonts/`.

| Uso | Tamaño | Peso |
|---|---|---|
| Título de página | 21 px | 650 |
| Título de cajón | 15 px | 650 |
| Texto | 13,5 px | 400 a 550 |
| Etiquetas de sección | 11,5 px mayúsculas | 650 |
| Mono | 12,5 px (13 px en la terminal) | 400 |

## 4. Componentes

- **Tarjeta de host:** insignia del color del entorno, nombre, `usuario@host` en mono, descripción en
  dos líneas, chips (entorno, modo, salto, etiquetas) y botón Connect. Los iconos de probar y editar
  aparecen al pasar el mouse.
- **Cajón (Drawer):** formularios a la derecha, sobre un velo. Pie con Delete a la izquierda y
  Cancel / Save a la derecha.
- **Chip:** pastilla de 20 px. Variantes `prod`, `test`, `dev`, `ok`, `warn`, `bad`.
- **Control segmentado:** modo de reglas y entorno.
- **Tarjeta de aprobación:** host + entorno + sudo, motivo en cursiva, advertencias, el comando
  editable en mono, y las acciones.
- **Alerta de huella:** franja arriba del contenido; roja si la clave cambió.
- **Botón Agent access:** siempre visible abajo en la barra lateral; verde encendido, rojo apagado.
- **Select** (`components/Select.svelte`): reemplaza al `<select>` nativo. Lista flotante con
  check en el elegido, pista en gris por opción, flechas / Enter / Esc / escribir para saltar, y
  buscador si hay más de 7 opciones.
- **Switch**: reemplaza a las casillas de un solo ajuste. **Seg**: control segmentado con píldora
  que se desliza. Las casillas que quedan (listas) son propias, con tilde animada.
- **Campos**: la ayuda va debajo del campo (`.field .hint`), así los campos de una fila quedan
  alineados aunque uno tenga ayuda y otro no.

## 5. Layout

Como Termius. La ventana no tiene marco de Windows: arriba va una barra de 46 px que es a la vez
barra de título y tira de pestañas (`components/Titlebar.svelte`):

`≡` (plegar la barra lateral) · pestaña **Vault** · una pestaña por terminal · `+ New tab` · hueco
para arrastrar · aprobaciones pendientes · minimizar, maximizar, cerrar (a la bandeja).

Debajo, en la pestaña Vault, la barra lateral (216 px, o 58 px plegada con solo iconos) y el
contenido. En una pestaña de terminal no hay barra lateral: la terminal usa todo el ancho. Las
terminales quedan montadas aunque se cambie de pestaña (no se pierde la sesión).

"New tab" (Ctrl T) abre un buscador de hosts; Enter abre la terminal. Los atajos están en
`lib/shortcuts.ts` y se listan en Settings.

## 5b. Movimiento

Todas las animaciones pasan por `lib/motion.ts` y duran 0 con "reducir animaciones" de Windows.

| Qué | Cómo |
|---|---|
| Cambiar de vista | sube 6 px y aparece (220 ms) |
| Cajón | entra desde la derecha con un poco de resorte; el fondo se desenfoca |
| Diálogos, buscador, desplegables | crecen desde 95 % (160 ms) |
| Tarjetas de hosts | aparecen escalonadas (35 ms entre una y otra) y se elevan 2 px al pasar |
| Botones | suben 1 px al pasar, se hunden al apretar; el principal tiene un brillo que lo cruza |
| Interruptor | el pulgar rebota y se estira al apretar |
| Contraseña incorrecta | la tarjeta tiembla |
| Avisos | entran desde abajo y se reacomodan (flip) |

En Athanor el movimiento es por pasos (`steps()`), como el pixel art.

## 6. Profundidad

Default, Warp y Haze: sin sombras en tarjetas; profundidad por superficies y bordes. Poolsuite:
sombras duras desplazadas (`3px 3px 0`) en tarjetas, botones principales y el ítem activo.

## 7. Qué sí y qué no

- Sí: estados con color y texto a la vez (un chip rojo dice "denied").
- Sí: secretos nunca visibles; los campos guardados muestran "Stored. Type to replace."
- No: colores fijos en componentes. Si hace falta uno, es una variable nueva en los cuatro temas.
- No: animaciones largas. Solo la entrada del cajón (180 ms) y hovers.

## 8. Comportamiento responsive

Ventana mínima de 860 x 560. La grilla de hosts se acomoda sola (`auto-fill`, 250 px).

## 9. Guía para agentes

Antes de cambiar la interfaz: leer este archivo y `styles/themes.css`. Probar en los cuatro temas
(Settings → Theme). `npm run check` sin avisos.
