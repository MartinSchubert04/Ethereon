// Pestañas entre ventanas: sacar una terminal a su propia ventana, pasarla a otra o volverla
// a la principal. Las sesiones no se cortan: la ventana que la tenia la suelta
// (term_detach) y la nueva la toma (term_attach) con lo ultimo que se vio.
//
// Cada ventana tiene una bandeja de pestañas en el backend: se dejan ahi (term_window_give o
// al crear la ventana) y la ventana las toma al abrirse o cuando le avisan ("take-tabs").
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { app, closeTerminal, leaving, newKey, toast, type TermTab } from "./state.svelte";
import { remap, type Tile } from "./tiles.svelte";
import { sizeOf } from "./live";

/** Lo que viaja de una ventana a otra. */
interface Moved {
  title: string;
  host: string;
  local?: string;
  cwd?: string;
  sftp?: boolean;
  approvals?: boolean;
  attach?: number;
  size?: { cols: number; rows: number };
  members?: Moved[];
  active?: number;
  /** Mosaico: el arbol con el indice de cada miembro en vez de su clave. */
  layout?: Tile;
  focus?: number;
}

const pack = (t: TermTab): Moved => ({
  title: t.title,
  host: t.host,
  local: t.local,
  cwd: t.cwd,
  sftp: t.sftp,
  approvals: t.approvals,
  attach: t.sftp || t.approvals ? undefined : t.sid,
  size: t.sftp || t.approvals || t.members ? undefined : sizeOf(t.key),
  members: t.members?.map(pack),
  active: t.members ? Math.max(0, t.members.findIndex((m) => m.key === t.activeMember)) : undefined,
  layout: t.layout && t.members ? remap(t.layout, (k) => t.members!.findIndex((m) => m.key === k)) : undefined,
  focus: t.layout && t.members ? Math.max(0, t.members.findIndex((m) => m.key === t.focus)) : undefined,
});

function unpack(m: Moved): TermTab {
  const members = m.members?.map(unpack);
  return {
    key: newKey(),
    host: m.host,
    local: m.local,
    cwd: m.cwd,
    sftp: m.sftp,
    approvals: m.approvals,
    title: m.title,
    alive: true,
    attach: m.attach,
    size: m.size,
    members,
    activeMember: members ? members[m.active ?? 0]?.key : undefined,
    layout: m.layout && members ? remap(m.layout, (i) => members[i]?.key ?? i) : undefined,
    focus: m.layout && members ? members[m.focus ?? 0]?.key : undefined,
  };
}

/** Suelta las sesiones de la pestaña (y de sus miembros) para que otra ventana las tome. */
async function release(t: TermTab) {
  for (const m of t.members ?? [t]) {
    m.moving = true;
    if (!m.sftp && m.sid) {
      leaving.add(m.sid);
      await api.termDetach(m.sid).catch(() => {});
    }
  }
  t.moving = true;
}

export const currentLabel = () => getCurrentWindow().label;

/** Las otras ventanas a las que se puede mandar una pestaña. */
export async function otherWindows(): Promise<{ label: string; name: string }[]> {
  const all = await invoke<string[]>("term_windows").catch(() => [] as string[]);
  return all
    .filter((l) => l !== currentLabel())
    .map((l) => ({ label: l, name: l === "main" ? "Main window" : `Window ${l.replace("term-", "")}` }));
}

/** La otra ventana de Ethereon bajo el cursor, si hay (para soltar ahi una pestaña). */
export async function windowAtCursor(): Promise<{ label: string; name: string } | null> {
  const label = await invoke<string | null>("window_at", { except: currentLabel() }).catch(() => null);
  return label ? { label, name: label === "main" ? "the main window" : `window ${label.replace("term-", "")}` } : null;
}

/** Manda una pestaña a una ventana nueva (en `at`, coordenadas de pantalla) o a otra. */
export async function moveTab(key: number, target: "new" | string, at?: { x: number; y: number }) {
  const t = app.terms.find((x) => x.key === key);
  if (!t) return;
  const tabs = [pack(t)];
  await release(t);
  // Primero se entrega y despues se saca de aca: si era la ultima pestaña de una ventana de
  // terminales, la ventana se cierra al sacarla.
  try {
    if (target === "new") {
      const small = t.approvals ? APPROVALS_SIZE : {};
      await invoke("term_window_open", { tabs, x: at?.x ?? null, y: at?.y ?? null, ...small });
    } else {
      await invoke("term_window_give", { label: target, tabs });
      if (target === "main") await invoke("main_window_show");
      await emitTo(target, "take-tabs");
    }
  } catch (e) {
    toast(String(e), true);
  }
  closeTerminal(key);
}

/** Las aprobaciones solas van en una ventana mas chica que una de terminales. */
const APPROVALS_SIZE = { width: 620, height: 720 };

/** Las aprobaciones en una ventana propia. */
export async function approvalsWindow() {
  const tabs: Moved[] = [{ title: "Approvals", host: "Approvals", approvals: true }];
  await invoke("term_window_open", { tabs, x: null, y: null, ...APPROVALS_SIZE }).catch((e) => toast(String(e), true));
}

async function take() {
  const tabs = await invoke<Moved[] | null>("term_window_take").catch(() => null);
  if (!tabs?.length) return;
  for (const m of tabs) {
    const t = unpack(m);
    app.terms.push(t);
    app.activeTerm = t.key;
  }
  app.view = "terminal";
  const w = getCurrentWindow();
  await w.show();
  await w.setFocus();
}

let started = false;
/** Toma las pestañas que esperan a esta ventana, ahora y cada vez que le avisen. */
export function initWindows() {
  if (started) return;
  started = true;
  listen("take-tabs", () => take());
  take();
}
