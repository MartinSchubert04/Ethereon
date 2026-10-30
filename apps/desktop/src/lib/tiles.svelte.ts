// Varias terminales en una pestaña, en mosaico (como Hyprland). Un arbol dice como se reparte
// el espacio: cada division parte en dos, en fila o en columna, con una proporcion que se
// cambia arrastrando la separacion. Las hojas son las terminales (claves de `members`).
//
// Mover terminales entre pestañas no corta las sesiones: la que se va suelta el canal
// (term_detach) y el panel nuevo lo toma (term_attach), igual que entre ventanas. La terminal
// que ya estaba en la pestaña conserva su clave, asi no se vuelve a montar.
import { api } from "./api";
import { park } from "./live";
import { app, closeTerminal, leaving, lockedWarn, newKey, toast, type TermTab } from "./state.svelte";

export type Side = "left" | "right" | "top" | "bottom";
export type Split = { dir: "row" | "col"; ratio: number; a: Tile; b: Tile };
export type Tile = { leaf: number } | Split;

export const isLeaf = (t: Tile): t is { leaf: number } => "leaf" in t;

/** Donde caeria lo que se esta arrastrando (lo dibuja TilePane). */
export const drop = $state({
  hint: null as null | { tab: number; leaf: number; side: Side },
  /** La pestaña de la barra sobre la que se esta arrastrando otra (de esta ventana o de otra). */
  tabOver: null as number | null,
  /** Mover con el teclado: la terminal entera con la que se intercambia (o la misma: se queda). */
  swap: null as null | { tab: number; leaf: number },
  /** Mover con el teclado: un borde de todo el mosaico (ocupa ese lado entero). */
  edge: null as null | { tab: number; side: Side },
});

export function leaves(t: Tile): number[] {
  return isLeaf(t) ? [t.leaf] : [...leaves(t.a), ...leaves(t.b)];
}

/** Pone `sub` al costado `side` de la hoja `target`. */
export function insert(t: Tile, target: number, sub: Tile, side: Side): Tile {
  if (isLeaf(t)) {
    if (t.leaf !== target) return t;
    const dir = side === "left" || side === "right" ? "row" : "col";
    const first = side === "left" || side === "top";
    return { dir, ratio: 0.5, a: first ? sub : t, b: first ? t : sub };
  }
  return { ...t, a: insert(t.a, target, sub, side), b: insert(t.b, target, sub, side) };
}

/** Saca una hoja; su hermana ocupa el lugar de la division. */
export function remove(t: Tile, key: number): Tile | null {
  if (isLeaf(t)) return t.leaf === key ? null : t;
  const a = remove(t.a, key);
  const b = remove(t.b, key);
  if (!a) return b;
  if (!b) return a;
  return { ...t, a, b };
}

export function remap(t: Tile, f: (k: number) => number): Tile {
  return isLeaf(t) ? { leaf: f(t.leaf) } : { ...t, a: remap(t.a, f), b: remap(t.b, f) };
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Rectangulos (de 0 a 1) de cada hoja y de cada division, para dibujar y redimensionar. */
export function geometry(t: Tile) {
  const panes = new Map<number, Rect>();
  const splits: { node: Split; r: Rect }[] = [];
  const walk = (n: Tile, r: Rect) => {
    if (isLeaf(n)) return void panes.set(n.leaf, r);
    splits.push({ node: n, r });
    if (n.dir === "row") {
      const w = r.w * n.ratio;
      walk(n.a, { ...r, w });
      walk(n.b, { ...r, x: r.x + w, w: r.w - w });
    } else {
      const h = r.h * n.ratio;
      walk(n.a, { ...r, h });
      walk(n.b, { ...r, y: r.y + h, h: r.h - h });
    }
  };
  walk(t, { x: 0, y: 0, w: 1, h: 1 });
  return { panes, splits };
}

/** El costado de un panel mas cercano al puntero. */
export function sideAt(r: DOMRect, x: number, y: number): Side {
  const fx = (x - r.left) / r.width;
  const fy = (y - r.top) / r.height;
  const d: [Side, number][] = [
    ["left", fx],
    ["right", 1 - fx],
    ["top", fy],
    ["bottom", 1 - fy],
  ];
  return d.sort((p, q) => p[1] - q[1])[0][0];
}

/** El panel visible bajo el puntero, si hay. */
export function tileAt(x: number, y: number): { tab: number; leaf: number; side: Side } | null {
  const el = document
    .elementsFromPoint(x, y)
    .find((e) => e instanceof HTMLElement && e.dataset.tileLeaf) as HTMLElement | undefined;
  if (!el) return null;
  return { tab: Number(el.dataset.tileTab), leaf: Number(el.dataset.tileLeaf), side: sideAt(el.getBoundingClientRect(), x, y) };
}

/** Se puede partir: una terminal o las aprobaciones (no SFTP ni la lista de un grupo). */
export const splittable = (t: TermTab) => !t.sftp && !(t.members && !t.layout);

/** Una pestaña comun pasa a mosaico. Su terminal queda como primer panel con la misma clave. */
function tile(t: TermTab) {
  if (t.layout) return;
  const first: TermTab = {
    key: t.key,
    host: t.host,
    local: t.local,
    cwd: t.cwd,
    title: t.title,
    alive: t.alive,
    sid: t.sid,
    connecting: t.connecting,
    approvals: t.approvals,
  };
  t.members = [first];
  t.layout = { leaf: t.key };
  t.focus = t.key;
}

/** Un panel nuevo que se queda con la terminal de `m` (la misma, viva). Si `m` no tiene
 *  terminal en esta ventana, retoma su sesion desde el backend. */
async function successor(m: TermTab): Promise<TermTab> {
  const next: TermTab = { key: newKey(), host: m.host, local: m.local, cwd: m.cwd, title: m.title, alive: m.alive, sid: m.sid };
  m.moving = true;
  // Las aprobaciones no tienen sesion: alcanza con un panel nuevo.
  if (m.approvals) return { ...next, approvals: true, sid: undefined, alive: true };
  if (park(m.key)) next.adopt = m.key;
  else if (m.sid) {
    leaving.add(m.sid);
    await api.termDetach(m.sid).catch(() => {});
    next.attach = m.sid;
  }
  return next;
}

/** Los paneles de una pestaña, listos para mudarse a otra. */
async function lift(t: TermTab): Promise<{ members: TermTab[]; layout: Tile }> {
  const src = t.layout ? (t.members ?? []) : [t];
  const keys = new Map<number, number>();
  const members: TermTab[] = [];
  for (const m of src) {
    const next = await successor(m);
    keys.set(m.key, next.key);
    members.push(next);
  }
  t.moving = true;
  return { members, layout: remap(t.layout ?? { leaf: t.key }, (k) => keys.get(k) ?? k) };
}

/** Lleva la pestaña `from` (con todos sus paneles) al costado de un panel de otra pestaña. */
export async function mergeInto(from: number, to: number, leaf: number, side: Side) {
  const src = app.terms.find((t) => t.key === from);
  const dst = app.terms.find((t) => t.key === to);
  if (!src || !dst || src === dst) return;
  if (!splittable(src) || !splittable(dst)) return toast("Only terminals and approvals can share a tab", true);
  const got = await lift(src);
  tile(dst);
  dst.members!.push(...got.members);
  dst.layout = insert(dst.layout!, leaf, got.layout, side);
  dst.focus = got.members[0].key;
  closeTerminal(from);
  app.activeTerm = dst.key;
  app.view = "terminal";
}

/** Intercambia dos paneles de lugar (cada uno queda con el tamaño del otro). */
export function swapLeaves(t: TermTab, a: number, b: number) {
  if (!t.layout || a === b) return;
  t.layout = remap(t.layout, (k) => (k === a ? b : k === b ? a : k));
}

/** Lleva un panel a un borde de todo el mosaico: ocupa ese lado entero, con su parte. */
export function moveToEdge(t: TermTab, key: number, side: Side) {
  if (!t.layout) return;
  const rest = remove(t.layout, key);
  if (!rest) return;
  const n = leaves(t.layout).length;
  const first = side === "left" || side === "top";
  const dir = side === "left" || side === "right" ? "row" : "col";
  t.layout = { dir, ratio: first ? 1 / n : 1 - 1 / n, a: first ? { leaf: key } : rest, b: first ? rest : { leaf: key } };
}

/** Mueve un panel al costado de otro, dentro de la misma pestaña. */
export function moveLeaf(t: TermTab, key: number, target: number, side: Side) {
  if (!t.layout || key === target) return;
  const rest = remove(t.layout, key);
  if (rest) t.layout = insert(rest, target, { leaf: key }, side);
}

function forget(t: TermTab, key: number) {
  const rest = t.layout ? remove(t.layout, key) : null;
  t.members = (t.members ?? []).filter((m) => m.key !== key);
  if (!rest || !t.members.length) return closeTerminal(t.key);
  if (t.members.length === 1) return untile(t, t.members[0]);
  t.layout = rest;
  if (t.focus === key) t.focus = leaves(rest)[0];
  t.title = t.members[0].title;
}

/** Al mosaico le queda un solo panel: vuelve a ser una pestaña comun, con esa terminal viva.
 *  Si el panel tiene otra clave que la pestaña, la pestaña adopta su terminal (como popOut);
 *  si es la misma (el primer panel), TermPane ya la encuentra por clave y no se desmonta. */
function untile(t: TermTab, m: TermTab) {
  if (m.key !== t.key) {
    if (!m.approvals && park(m.key)) t.adopt = m.key;
    // Todavia no montada: lo que iba a tomar el panel lo toma la pestaña.
    else [t.adopt, t.attach, t.size] = [m.adopt, m.attach, m.size];
  }
  t.host = m.host;
  t.local = m.local;
  t.cwd = m.cwd;
  t.title = m.title;
  t.alive = m.alive;
  t.sid = m.sid;
  t.connecting = m.connecting;
  t.error = m.error;
  t.failedAt = m.failedAt;
  t.approvals = m.approvals;
  t.members = undefined;
  t.layout = undefined;
  t.focus = undefined;
}

/** Cierra un panel (y su sesion). Si era el ultimo, la pestaña. */
export function closeLeaf(t: TermTab, key: number) {
  // El ultimo panel de una pestaña bloqueada no se cierra (se iria la pestaña).
  if ((t.members?.length ?? 1) <= 1 && lockedWarn(t)) return;
  forget(t, key);
}

/** Saca un panel a una pestaña propia, sin cortar la sesion. */
export async function popOut(t: TermTab, key: number) {
  const m = t.members?.find((x) => x.key === key);
  if (!m || (t.members?.length ?? 0) < 2) return;
  const nt = await successor(m);
  forget(t, key);
  app.terms.splice(app.terms.indexOf(t) + 1, 0, nt);
  app.activeTerm = nt.key;
}
