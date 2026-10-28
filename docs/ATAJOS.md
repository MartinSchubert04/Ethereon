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
- Con fuentes de píxeles (PxPlus y similares) la negrita se marca con brillo y no engrosando: el
  navegador la engrosaría con suavizado y se vería borrosa.
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
