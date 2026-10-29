# Atajos y comodidades

Todo lo que se puede hacer con el teclado (y algunos gestos con el mouse) en Ethereon.
Los atajos marcados con ✎ se cambian en **Settings → Keyboard shortcuts**; los demás son fijos.

## Desde cualquier lado de Windows

| Atajo | Qué hace |
|---|---|
| `Ctrl Shift Alt T` | Si la ventana activa es una carpeta del Explorador (o el Escritorio), abre una terminal en esa carpeta, como "Abrir en Terminal". Si no, trae Ethereon al frente (aunque esté cerrado en la bandeja) con PowerShell en tu Escritorio; si ya hay una terminal local abierta, va a esa en vez de abrir otra |

Se cambia o se apaga en **Settings → Startup and updates**. Si otra app ya usa la combinación,
avisa y queda la anterior. Ahí mismo se elige dónde se abre la carpeta:

- **Windows Terminal** (por defecto): con tu perfil, tu tema y tu fuente. Ethereon queda en la
  bandeja.
- **Ethereon**: una pestaña de PowerShell 7. Con el vault bloqueado, se abre al desbloquearlo.

## En toda la app

| Atajo | Qué hace |
|---|---|
| `Ctrl T` ✎ | Pestaña nueva: buscar un host o una shell de esta PC |
| `Ctrl Shift T` ✎ | Terminal local nueva (la primera shell de la lista, normalmente PowerShell 7) |
| `Ctrl R` ✎ | Refrescar la pestaña actual: en SFTP recarga las carpetas; en una pestaña que no conectó o se cortó, reconecta ahí mismo. En una terminal conectada, `Ctrl R` es de la shell (buscar en el historial) |
| `Ctrl W` ✎ | Cerrar la pestaña (dentro de una terminal va a la shell si así está configurado) |
| `Ctrl Shift W` ✎ | Cerrar la pestaña, también desde una terminal |
| `Ctrl Tab` / `Ctrl Shift Tab` ✎ | Pestaña siguiente / anterior |
| `Ctrl 1` … `Ctrl 9` | Ir a la pestaña N (`Ctrl 1` es el vault) |
| `Ctrl B` ✎ | Mostrar u ocultar la barra lateral (en una terminal, flota encima) |
| `Ctrl F` ✎ | Buscar hosts |
| `Ctrl N` ✎ | Host nuevo. En Keychain: credencial nueva |
| `Ctrl ,` ✎ | Settings |
| `Ctrl Shift L` ✎ | Bloquear el vault |

La app nunca se recarga: `F5`, `Ctrl R` y `Ctrl Shift R` no cierran las pestañas como en un
navegador.

## Pestañas y ventanas

- **Reordenar:** arrastrar una pestaña sobre otra.
- **Sacarla a su propia ventana:** arrastrarla fuera de la barra (hacia abajo o fuera de la
  ventana) y soltarla; la ventana nueva aparece donde se suelta. También con clic derecho →
  **Move to new window**.
- **Pasarla a otra ventana o volverla a la principal:** clic derecho → **Move to …**. Una ventana
  de terminales puede tener varias pestañas, y se cierra sola cuando se queda sin ninguna.
- La sesión no se corta al mudarse: la terminal sigue siendo la misma, con lo que se veía.
  (Las de SFTP se vuelven a abrir en la otra ventana.)

## Varias terminales en una pestaña (mosaico)

- **Juntar:** arrastrar una pestaña hacia abajo, sobre una terminal, y soltarla sobre uno de sus
  bordes (izquierda, derecha, arriba o abajo): ese espacio se parte en dos. Para sumarla a otra
  pestaña, quedarse medio segundo encima de esa pestaña mientras se arrastra: se muestra, y ahí
  se baja a su terminal. Funciona también entre ventanas. Una pestaña que ya tiene varias
  terminales se suma entera.
- **Ancho y alto:** arrastrar la separación entre dos terminales.
- **Reacomodar:** arrastrar una terminal por su nombre (arriba a la izquierda, sobre el borde) a
  otro costado de otra. **Volverla una pestaña sola:** soltarla sobre la barra de pestañas.
- **Reacomodar con el teclado:** con `Ctrl Shift` apretados, las flechas mueven un cursor por el
  mosaico y una marca muestra a dónde iría la terminal con el foco. Se mueve recién al soltar
  `Ctrl Shift`; `Esc` cancela.
  - Hacia otra terminal: el cursor salta a la más cercana de ese lado, recordando la altura o la
    columna en la que venía. Con dos a la izquierda (arriba y abajo) y una a la derecha, desde la
    derecha `←` va a la de arriba y desde ahí `↓` a la de abajo. Marca entera = se intercambian;
    punteada sobre la propia = se queda donde está.
  - Sin nada más de ese lado: la marca pasa a la mitad de esa terminal (va a ese costado, se
    parte su lugar). La misma flecha otra vez: todo ese lado del mosaico.
  - La flecha contraria deshace el último paso.

  Solo en un mosaico: si no, `Ctrl Shift ←/→` sigue siendo de la shell (seleccionar por
  palabras). Se cambia en Settings → Shortcuts (cuentan los modificadores).
- **Pasar de una a otra:** `Shift Tab` (en orden, y de la última vuelve a la primera). También
  en las pestañas de grupo. Con una sola terminal en la pestaña, `Shift Tab` sigue siendo de la
  shell (en PowerShell completa hacia atrás). Se cambia en Settings → Shortcuts.
- **Cerrar una:** la `×` de su borde, o `Ctrl Shift W` en la que tiene el foco.
- Si una conexión se cae, las demás siguen. La caída muestra el camino (esta PC, los saltos, el
  host) con el tramo cortado y un botón **Retry**. `Ctrl R` reintenta todas las caídas de la
  pestaña y deja las conectadas como están; si no hay ninguna caída, `Ctrl R` es de la shell.
- Mover terminales dentro de la ventana no reconecta nada: es la misma terminal, viva, en otro
  lugar. Al angostarse, xterm reacomoda lo que ya estaba escrito (como cualquier terminal);
  una imagen que había en ese historial puede quedar partida.

## Grupos

- **Open all** (en el título de cada grupo de Hosts) abre todos sus hosts en una sola pestaña,
  como Termius: a la izquierda la lista "Terminals" con el estado de cada una (Connecting,
  Connected, Disconnected) y a la derecha la elegida.
- En esa lista: clic o `↑` `↓` para cambiar, `×` o clic del medio para cerrar una.
  `Ctrl Shift W` cierra solo la terminal elegida; con la última se cierra la pestaña.
- **Reordenar hosts:** arrastrar la tarjeta; una línea marca dónde cae. Soltarla en otro grupo la
  mueve ahí (si el agente pasa a correr sin preguntar, pide confirmación).

## Buscador de pestaña nueva (`Ctrl T`)

| Atajo | Qué hace |
|---|---|
| Escribir | Filtra por nombre, dirección, grupo, tags o descripción |
| `↑` `↓` | Elegir |
| `Enter` | Abrir la terminal del host (o la shell local) |
| `Shift Enter` / `Shift` + clic | Abrir los archivos del host (SFTP) |
| `Esc` | Cerrar |

## Formularios (host, grupo, credencial)

| Atajo | Qué hace |
|---|---|
| `Ctrl S` ✎ o `Ctrl Enter` | Guardar |
| `Esc` | Cerrar sin guardar |

- Al crear un host se puede escribir la contraseña ahí mismo (**Login → Password**). Se guarda
  cifrada en el vault como `<host>-password`; no hace falta crearla antes en Keychain.
- Si la credencial tiene usuario, el campo **User** del host puede quedar vacío y se usa el de la
  credencial.
- **Environment** arranca en **None**. Es solo una etiqueta (prod, test, dev): pinta el host y le
  dice al agente que una máquina es de producción. No cambia ninguna regla.
- En **Settings**, `Ctrl S` guarda la sección Security (lo demás se guarda solo).
- `Esc` en un diálogo de confirmación cierra solo el diálogo, no el formulario de atrás.

## Terminal

| Atajo | Qué hace |
|---|---|
| `Ctrl Shift C` / `Ctrl Shift V` ✎ | Copiar / pegar |
| `Ctrl C` | Copia si hay texto elegido; si no, corta el proceso (configurable) |
| `Ctrl V` | Pega (configurable) |
| Clic derecho | Copia lo elegido o pega si no hay nada (configurable) |
| `Ctrl =` / `Ctrl -` ✎ | Letra más grande / más chica |
| `Ctrl 0` ✎ | Volver al tamaño del tema |

- La fuente se elige en **Settings → Terminal → Font**: la del tema, la misma de Windows Terminal
  (copia fuente y tamaño), una de las incluidas (JetBrains Mono, Fira Code, IBM Plex Mono, Source
  Code Pro, Cascadia Mono, y las de los temas) o cualquier instalada. Los iconos de Nerd Font se ven
  con cualquiera. La fuente la pone la terminal, no la shell: el tema de PowerShell no la cambia.
- Las terminales locales no heredan variables de la terminal desde la que se abrió Ethereon
  (`WT_SESSION`, `NO_COLOR`…) y se presentan como `TERM_PROGRAM=Ethereon`, para que el perfil de
  PowerShell sepa dónde está.
- Muestran imágenes Sixel (`chafa -f sixel`, `img2sixel`…). Para eso Ethereon usa la ConPTY de
  Windows Terminal (`apps/desktop/src-tauri/conpty`): la de Windows descarta las imágenes. Por
  defecto las escala como Windows Terminal (10×20 píxeles por celda), así lo pensado para Windows
  Terminal se ve igual; se cambia en **Settings → Terminal**.
  El decodificador Sixel es WebAssembly: la CSP de `tauri.conf.json` necesita `'wasm-unsafe-eval'`
  (sin eso, en la app instalada las imágenes no salen; en `npm run dev` no hay CSP y sí salen).
- Con fuentes de píxeles (PxPlus y similares) la negrita se dibuja como en Windows Terminal: la
  letra repetida un píxel a la derecha, nítida. El navegador la engrosaría con suavizado y se
  vería borrosa (plugin `pixelBold` en `vite.config.ts`).
- Las terminales locales usan la shell real de Windows (PowerShell 7, Windows PowerShell, CMD, WSL,
  Git Bash, las que estén instaladas), con tu perfil, oh-my-posh incluido.
- Los bloques y las líneas de caja (`█ ▀ ▄ ─ │`), y también los sextantes y octantes que usan
  `chafa` y los MOTD con imágenes, se dibujan pixel exacto, sin rayas entre filas ni letras de otra
  fuente. La tabla de sextantes y octantes sale de `apps/desktop/scripts/legacy_glyphs.py`.

## Archivos (SFTP)

Se abre con el botón **SFTP** de cada host o con `Shift Enter` en el buscador. Arranca con esta PC a
la izquierda y el servidor a la derecha, pero **cada lado se cambia desde su título**: esta PC o
cualquier host. Así se sube, se baja, se copia de un servidor a otro (directo, sin pasar por el disco
de esta PC) o dentro de esta PC. Si un lado no conecta, el otro sigue andando y desde el mismo título
se elige otro.

| Atajo | Qué hace |
|---|---|
| Escribir letras | Saltar al primer elemento que empieza así; la misma letra otra vez va al siguiente |
| `↑` `↓` `Inicio` `Fin` | Moverse (con `Shift`, elegir varios) |
| Clic / `Ctrl` clic / `Shift` clic | Elegir uno / sumar o sacar / elegir un rango |
| `Ctrl A` | Elegir todo |
| `Enter` / doble clic | Abrir la carpeta; en un archivo, mandarlo al otro lado |
| `Espacio` o `Ctrl Enter` | Copiar lo elegido al otro lado |
| `Backspace` / `Alt ←` | Subir una carpeta |
| `F2` | Renombrar |
| `Supr` | Borrar (pregunta antes; las carpetas se borran con todo) |
| `Ctrl Shift N` | Carpeta nueva (queda lista para ponerle nombre) |
| `F5` o `Ctrl R` | Recargar |
| `Ctrl F` | Filtrar la carpeta |
| `Ctrl H` | Mostrar u ocultar archivos ocultos (los que empiezan con punto) |
| `Esc` | Soltar la selección |
| `Tab` | Pasar al otro panel |

- **Arrastrar** archivos de un panel al otro los transfiere. Soltarlos sobre una carpeta los mete
  ahí adentro.
- También se pueden **arrastrar desde el Explorador de Windows** a cualquiera de los dos paneles.
- Clic derecho abre un menú con todas las acciones.
- El panel de esta PC arranca en el Escritorio. Se cambia en **Settings → Files (SFTP)**: Escritorio,
  carpeta del usuario u otra carpeta. El botón de la casita vuelve ahí.
- Clic en la ruta de arriba para escribir una a mano (`Enter` para ir). Las migas de pan llevan a
  cada carpeta; en esta PC, el ícono del disco muestra las unidades.
- Antes de pisar algo que ya existe, pregunta.
- Las transferencias corren en segundo plano con progreso, velocidad y botón para cancelar. Un
  archivo cancelado a mitad se borra.
- Cada acción en el servidor queda en la auditoría (`[sftp] upload …`, `delete …`, etc.).
- Con el vault bloqueado no se toca nada del servidor. Al desbloquear, la pestaña se reconecta sola.

## Importar

**Hosts → Import → A file...** (o **Settings → Import and export**), o soltar el archivo sobre la
página de Hosts. El formato se reconoce solo, mirando el contenido:

- Respaldo cifrado de Ethereon (pide la contraseña maestra de ese respaldo).
- Export de Ethereon en JSON (el formato por defecto al exportar) o CSV.
- Carpeta exportada en formato OpenSSH, o una carpeta `.ssh` (**A folder...**).
- SSH config (también lo que exporta Termius), claves privadas y JSON de Bitwarden.

Siempre suma al vault actual: lo que ya existe con el mismo nombre se deja como está, y las huellas
de servidores ya confirmadas no se pisan. Los tokens de los agentes no se importan.

## Settings

Las secciones se abren y cierran con un clic; Ethereon recuerda cuáles dejaste abiertas.
