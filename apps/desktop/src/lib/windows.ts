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
import { app, closeTerminal, leaving, newKey, toast, type TermTab, type View } from "./state.svelte";
import { drop, remap, type Tile } from "./tiles.svelte";
import { sizeOf, textOf } from "./live";

/** Lo que viaja de una ventana a otra. */
interface Moved {
  title: string;
  host: string;
  local?: string;
  cwd?: string;
  sftp?: boolean;
  approvals?: boolean;
  locked?: boolean;
  collapsed?: boolean;
  /** Al volver de una actualizacion: lo que mostraba la terminal, y si era la pestaña activa. */
  replay?: string;
  current?: boolean;
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
  locked: t.locked,
  collapsed: t.collapsed,
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
    locked: m.locked,
    collapsed: m.collapsed,
    replay: m.replay,
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

// ---- una pestaña de otra ventana pasando por encima de esta
//
// La ventana que arrastra le va mandando donde esta el cursor ("drag-hover"). Aca se marca
// la pestaña de abajo y, si el cursor se queda, se muestra (como en el navegador), para ver
// que hay antes de soltar. Si se va sin soltar, vuelve la que se veia.

/** Cuanto hay que quedarse encima de una pestaña para que se muestre. */
const HOVER_SHOW = 450;
let remote: { orig: { view: View; active: number }; key?: number; timer?: number } | null = null;
/** Donde poner lo que llegue: al lado de la pestaña sobre la que se solto. */
let dropAt: number | null = null;

function remoteHover(p: { x: number; y: number } | null) {
  if (!p) {
    if (remote) {
      clearTimeout(remote.timer);
      if (app.terms.some((t) => t.key === remote!.orig.active)) app.activeTerm = remote.orig.active;
      app.view = remote.orig.view;
    }
    remote = null;
    drop.tabOver = null;
    dropAt = null;
    return;
  }
  remote ??= { orig: { view: app.view, active: app.activeTerm } };
  const bar = document.querySelector(".bar")?.getBoundingClientRect();
  const inBar = !!bar && p.y >= bar.top - 8 && p.y <= bar.bottom + 8 && p.x >= 0 && p.x <= innerWidth;
  const el = inBar
    ? (document
        .elementsFromPoint(p.x, (bar!.top + bar!.bottom) / 2)
        .find((e) => e instanceof HTMLElement && e.classList.contains("tab") && e.dataset.key) as HTMLElement | undefined)
    : undefined;
  const key = el ? Number(el.dataset.key) : null;
  drop.tabOver = key;
  if (el) {
    const r = el.getBoundingClientRect();
    const i = app.terms.findIndex((t) => t.key === key);
    dropAt = p.x < r.left + r.width / 2 ? i : i + 1;
  } else dropAt = null;
  if (key === (remote.key ?? null)) return;
  clearTimeout(remote.timer);
  remote.key = key ?? undefined;
  if (key == null) return;
  remote.timer = window.setTimeout(() => {
    if (remote?.key !== key || !app.terms.some((t) => t.key === key)) return;
    app.activeTerm = key;
    app.view = "terminal";
  }, HOVER_SHOW);
}

async function take() {
  const tabs = await invoke<Moved[] | null>("term_window_take").catch(() => null);
  // Se solto aca: lo que se mostro al pasar queda como esta (no se vuelve atras).
  const at = dropAt;
  if (remote) clearTimeout(remote.timer);
  remote = null;
  drop.tabOver = null;
  dropAt = null;
  if (!tabs?.length) return;
  let current = 0;
  let i = at != null && at >= 0 && at <= app.terms.length ? at : app.terms.length;
  for (const m of tabs) {
    const t = unpack(m);
    app.terms.splice(i++, 0, t);
    app.activeTerm = t.key;
    if (m.current) current = t.key;
  }
  if (current) app.activeTerm = current;
  app.view = "terminal";
  const w = getCurrentWindow();
  await w.show();
  await w.setFocus();
}

// ---- sesion de antes de actualizar
//
// Al actualizar se cortan todas las conexiones. Antes, cada ventana cuenta que tiene (el
// backend la guarda en el vault) y al desbloquear la principal vuelve a abrir todo igual:
// las mismas ventanas en el mismo lugar, las pestañas, mosaicos y grupos, y en cada
// terminal lo que se veia, apagado, antes de reconectar.

/** Lo que cuenta cada ventana. */
interface Part {
  label: string;
  view: string;
  tabs: Moved[];
  box?: { x: number; y: number; width: number; height: number; maximized: boolean };
}

/** Como `pack`, pero para despues de reiniciar: sin sesion que tomar y con el texto. */
const packSaved = (t: TermTab): Moved => ({
  ...pack(t),
  attach: undefined,
  size: undefined,
  replay: t.sftp || t.approvals || t.members ? undefined : textOf(t.key),
  members: t.members?.map(packSaved),
});

async function saveSession() {
  const w = getCurrentWindow();
  const tabs = app.terms.map((t) => ({ ...packSaved(t), current: t.key === app.activeTerm || undefined }));
  const [pos, size, maximized] = await Promise.all([w.outerPosition(), w.outerSize(), w.isMaximized()]).catch(() => [null, null, false] as const);
  const box = pos && size ? { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized } : undefined;
  const part: Part = { label: w.label, view: app.view, tabs, box };
  await invoke("session_part", { part }).catch(() => {});
}

/** En la principal, al desbloquear: vuelve a abrir lo que habia antes de actualizar. */
export async function restoreSession() {
  const parts = await invoke<Part[] | null>("session_take").catch(() => null);
  if (!parts?.length) return;
  // Si la instalacion fallo, la app no se reinicio y todo sigue abierto: no duplicar.
  const others = (await invoke<string[]>("term_windows").catch(() => [] as string[])).filter((l) => l !== "main");
  if (app.terms.length || others.length) return;
  let n = 0;
  for (const p of parts) {
    if (!p.tabs.length) continue;
    n += p.tabs.length;
    if (p.label !== "main") {
      await invoke("term_window_open", { tabs: p.tabs, x: null, y: null, place: p.box ?? null }).catch(() => {});
      continue;
    }
    for (const m of p.tabs) {
      const t = unpack(m);
      app.terms.push(t);
      if (m.current) app.activeTerm = t.key;
    }
    if (p.view === "terminal" && !app.activeTerm) app.activeTerm = app.terms[0].key;
    if (p.view === "terminal") app.view = "terminal";
  }
  if (n) toast(n === 1 ? "Reopened the tab you had before the update" : `Reopened the ${n} tabs you had before the update`);
}

let started = false;
/** Toma las pestañas que esperan a esta ventana, ahora y cada vez que le avisen. */
export function initWindows() {
  if (started) return;
  started = true;
  listen("take-tabs", () => take());
  listen<{ x: number; y: number } | null>("drag-hover", (e) => remoteHover(e.payload));
  listen("session-save", () => saveSession());
  take();
}
