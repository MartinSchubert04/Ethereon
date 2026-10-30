// La carpeta en la que esta una shell local, para el nombre de su pestaña. Ethereon lanza las
// shells para que la avisen en cada prompt (OSC 9;9, ver report_cwd en terminal.rs); tambien
// se entiende OSC 7 (file://host/ruta), que mandan muchas shells de Linux.
//
// La pestaña no cambia de nombre con cada `cd`: solo despues de quedarse un rato en la misma
// carpeta (Settings > Terminal), asi pasar de largo por varias no la hace parpadear.
import type { Terminal } from "@xterm/xterm";
import { prefs } from "./prefs.svelte";
import { app, type TermTab } from "./state.svelte";

/** El nombre de la carpeta: lo ultimo de la ruta, o la unidad ("C:") si es la raiz. */
export function folderName(path: string): string {
  const p = path.replace(/[\\/]+$/, "");
  return p.split(/[\\/]/).pop() || p || "/";
}

function fromOsc9(data: string): string | null {
  if (!data.startsWith("9;")) return null; // OSC 9 solo es tambien una notificacion
  return data.slice(2).replace(/^"|"$/g, "") || null;
}

function fromOsc7(data: string): string | null {
  try {
    const u = new URL(data);
    if (u.protocol !== "file:") return null;
    const p = decodeURIComponent(u.pathname);
    // file:///C:/Users -> C:\Users
    return /^\/[A-Za-z]:/.test(p) ? p.slice(1).replace(/\//g, "\\") : p;
  } catch {
    return null;
  }
}

/** Escucha la carpeta de `t`. `me` da el registro actual de la terminal (cambia si se mueve). */
export function watchCwd(t: Terminal, me: () => TermTab) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let last: string | undefined;
  const seen = (path: string | null) => {
    const m = me();
    // Cada prompt la repite: la espera solo vuelve a empezar si cambio de carpeta.
    if (!path || !m.local || path === last) return;
    last = path;
    m.cwd = path;
    clearTimeout(timer);
    const delay = prefs.term.folderTitleDelay;
    if (delay < 0) return;
    timer = setTimeout(() => rename(me(), folderName(path)), delay * 1000);
  };
  t.parser.registerOscHandler(9, (d) => {
    const p = fromOsc9(d);
    seen(p);
    return p != null;
  });
  t.parser.registerOscHandler(7, (d) => (seen(fromOsc7(d)), true));
}

function rename(m: TermTab, name: string) {
  if (!m.local || m.title === name) return;
  m.title = name;
  // En un mosaico la pestaña lleva el nombre de su primer panel.
  const parent = app.terms.find((t) => t.layout && t.members?.[0]?.key === m.key);
  if (parent) parent.title = name;
}
