// Terminales vivas de esta ventana. Cada una (xterm + canal + sesion) vive aparte del panel
// que la muestra: al mover una terminal a otra pestaña o panel de la misma ventana, el panel
// viejo la estaciona y el nuevo la adopta, moviendo el mismo elemento. No se reconecta ni se
// repite nada: sigue igual, con todo su historial.
//
// Entre ventanas no se puede (cada ventana es otro WebView): ahi se usa term_detach/attach.
import type { FitAddon } from "@xterm/addon-fit";
import type { Terminal } from "@xterm/xterm";
import { api, type TermEvent } from "./api";
import type { TermTab } from "./state.svelte";

export interface Live {
  term: Terminal;
  fit: FitAddon;
  /** Sesion del backend; 0 sin conexion. */
  id: number;
  /** Repitiendo lo ultimo que se vio al retomar desde otra ventana. */
  replaying: boolean;
  /** Lo que hace con cada mensaje el panel que la muestra ahora. */
  handle: (m: TermEvent) => void;
  /** El registro (pestaña o panel) que la representa ahora. */
  me: () => TermTab;
  /** Teclas y seleccion: los maneja el panel actual (portapapeles, atajos). */
  keys: (e: KeyboardEvent) => boolean;
  selection: () => void;
  parked: boolean;
}

/** Por clave del panel que la muestra. */
const byKey = new Map<number, Live>();

export function own(key: number, l: Live) {
  byKey.set(key, l);
}

export function drop(key: number, l: Live) {
  if (byKey.get(key) === l) byKey.delete(key);
}

/** El panel `key` se va a desmontar y otro va a tomar su terminal. Si nadie la toma en unos
 *  segundos (algo fallo), se cierra para no dejar sesiones colgadas. */
export function park(key: number): boolean {
  const l = byKey.get(key);
  if (!l) return false;
  l.parked = true;
  setTimeout(() => {
    if (!l.parked) return;
    byKey.delete(key);
    if (l.id) api.termClose(l.id).catch(() => {});
    l.term.dispose();
  }, 5000);
  return true;
}

/** Filas y columnas de la terminal del panel `key` (para repetir su historial con el mismo
 *  tamaño en otra ventana). */
export function sizeOf(key: number): { cols: number; rows: number } | undefined {
  const t = byKey.get(key)?.term;
  return t ? { cols: t.cols, rows: t.rows } : undefined;
}

/** Toma la terminal que estaciono el panel `from`. */
export function adopt(from: number, key: number): Live | null {
  const l = byKey.get(from);
  if (!l?.parked) return null;
  l.parked = false;
  byKey.delete(from);
  byKey.set(key, l);
  return l;
}
