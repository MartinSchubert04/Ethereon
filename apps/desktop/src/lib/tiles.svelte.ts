// Varias terminales en una pestaña, en mosaico (como Hyprland). Un arbol dice como se reparte
// el espacio: cada division parte en dos, en fila o en columna, con una proporcion que se
// cambia arrastrando la separacion. Las hojas son las terminales (claves de `members`).
//
// Mover terminales entre pestañas no corta las sesiones: la que se va suelta el canal
// (term_detach) y el panel nuevo lo toma (term_attach), igual que entre ventanas. La terminal
// que ya estaba en la pestaña conserva su clave, asi no se vuelve a montar.
import { api } from "./api";
import { park } from "./live";
import { app, closeTerminal, leaving, lockedWarn, newKey, openApprovals, toast, type TermTab } from "./state.svelte";

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
  /** El rotulo que sigue al mouse mientras se arrastra (lo dibuja Titlebar, que siempre se ve). */
  ghost: null as null | { x: number; y: number; text: string },
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
    // El mismo bloque de TilePane sigue dibujandola: no se desmonta.
    slot: t.slot,
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

/** Pone `sub` (ya sumado a `members`) al costado `side` del panel `leaf`, o sin `leaf` a la
 *  derecha de todo el mosaico, con la parte que le toca segun cuantos paneles trae. */
function place(dst: TermTab, sub: Tile, leaf: number | undefined, side: Side) {
  if (leaf != null) dst.layout = insert(dst.layout!, leaf, sub, side);
  else {
    const share = leaves(sub).length / dst.members!.length;
    dst.layout = { dir: "row", ratio: 1 - share, a: dst.layout!, b: sub };
  }
}

/** La clave del panel de aprobaciones de una pestaña (la de la pestaña si es solo eso). */
function approvalsIn(t: TermTab): number | undefined {
  return t.layout ? t.members?.find((x) => x.approvals)?.key : t.approvals ? t.key : undefined;
}

/** Una pestaña tiene a lo sumo un panel de aprobaciones: si llega otro, se muestra el que ya
 *  estaba (todas muestran la misma lista). */
function showApprovals(dst: TermTab, key: number) {
  if (dst.layout) dst.focus = key;
  app.activeTerm = dst.key;
  app.view = "terminal";
}

/** Lleva la pestaña `from` (con todos sus paneles) al costado de un panel de otra pestaña (sin
 *  `leaf`: a la derecha de todo). */
export async function mergeInto(from: number, to: number, leaf: number | undefined, side: Side) {
  const src = app.terms.find((t) => t.key === from);
  const dst = app.terms.find((t) => t.key === to);
  if (!src || !dst || src === dst) return;
  if (!splittable(src) || !splittable(dst)) return toast("Only terminals and approvals can share a tab", true);
  const got = await lift(src);
  const had = approvalsIn(dst);
  if (had != null) {
    for (const m of got.members.filter((x) => x.approvals)) {
      got.members.splice(got.members.indexOf(m), 1);
      if (got.members.length) got.layout = remove(got.layout, m.key)!;
    }
    if (!got.members.length) {
      closeTerminal(from);
      return showApprovals(dst, had);
    }
  }
  tile(dst);
  dst.members!.push(...got.members);
  place(dst, got.layout, leaf, side);
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
    // El bloque de la clave de la pestaña era el primer panel (el que se cerro): con otra
    // identidad se desmonta (cierra su sesion) y el nuevo adopta la terminal de `m`. Con la
    // misma, Svelte lo reusaba y seguia mostrando la terminal cerrada.
    t.slot = newKey();
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

/** Lleva un panel de un mosaico a otra pestaña, sin cortar la sesion: al costado `side` de
 *  su panel `leaf`, o (sin `leaf`) a la derecha de todo su mosaico. */
export async function moveLeafTo(t: TermTab, key: number, to: number, leaf?: number, side: Side = "right") {
  const m = t.members?.find((x) => x.key === key);
  const dst = app.terms.find((x) => x.key === to);
  if (!m || !dst || dst === t || (t.members?.length ?? 0) < 2) return;
  if (!splittable(dst)) return toast("Only terminals and approvals can share a tab", true);
  const had = m.approvals ? approvalsIn(dst) : undefined;
  if (had != null) {
    forget(t, key);
    return showApprovals(dst, had);
  }
  const nt = await successor(m);
  forget(t, key);
  tile(dst);
  dst.members!.push(nt);
  place(dst, { leaf: nt.key }, leaf, side);
  dst.focus = nt.key;
  app.activeTerm = dst.key;
  app.view = "terminal";
}

/** La pestaña de la barra que esta bajo el puntero (solo cuenta x: la barra entera es la
 *  fila de pestañas). Usa la posicion de cada una en la fila, sin las animaciones. */
export function barTabAt(x: number): number | null {
  const row = document.querySelector<HTMLElement>(".bar .tabs");
  if (!row) return null;
  const cx = x - row.getBoundingClientRect().left + row.scrollLeft;
  const el = [...row.querySelectorAll<HTMLElement>(".tab[data-key]")].find((e) => cx >= e.offsetLeft && cx < e.offsetLeft + e.offsetWidth);
  return el ? Number(el.dataset.key) : null;
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

/** Las aprobaciones al costado de un panel de la pestaña `to` (sin `leaf`: a la derecha de
 *  todo). Cada mosaico puede tener las suyas: las de otras pestañas se quedan donde estan
 *  (muestran la misma lista). Si esta ya las tiene, se mueve ese panel: dos juntas no sirven. */
export async function dropApprovals(to: number, leaf?: number, side: Side = "right") {
  const dst = app.terms.find((t) => t.key === to);
  if (!dst) return;
  if (!splittable(dst)) return toast("Only terminals and approvals can share a tab", true);
  const show = () => {
    app.activeTerm = dst.key;
    app.view = "terminal";
  };
  // Pestaña de aprobaciones sola. En mosaico `approvals` sigue en la pestaña, pero el panel
  // es uno de los miembros.
  if (dst.approvals && !dst.layout) return show();
  const mine = dst.layout ? dst.members?.find((x) => x.approvals) : undefined;
  if (mine) {
    if (leaf == null) moveToEdge(dst, mine.key, "right");
    else if (leaf !== mine.key) moveLeaf(dst, mine.key, leaf, side);
    dst.focus = mine.key;
    return show();
  }
  tile(dst);
  const m: TermTab = { key: newKey(), host: "Approvals", approvals: true, title: "Approvals", alive: true };
  dst.members!.push(m);
  place(dst, { leaf: m.key }, leaf, side);
  dst.focus = m.key;
  show();
}

/** Ese panel es el de las aprobaciones. */
export function isApprovals(h: { tab: number; leaf: number }) {
  const t = app.terms.find((x) => x.key === h.tab);
  const m = t?.layout ? t.members?.find((x) => x.key === h.leaf) : t;
  return !!m?.approvals;
}

/** Cuanto hay que quedarse encima de una pestaña de la barra para que se muestre. */
export const HOVER_SHOW = 450;

export type Target = { tab: number; leaf?: number; side: Side } | "bar";

/** Arrastrar algo (un panel del mosaico, las aprobaciones de la barra lateral) hacia las
 *  pestañas. Sobre un panel de la que se ve: a ese costado. Sobre una pestaña de la barra: se
 *  suma a esa, y si el mouse se queda un momento se muestra, para bajar y elegir el costado.
 *  En otro lugar de la barra: "bar". Soltado en ningun lado, vuelve lo que se veia. Se sigue
 *  en la ventana y no en el origen: al mostrar otra pestaña, el origen puede quedar oculto. */
export function dragToTabs(
  e: PointerEvent,
  o: {
    /** El nombre de lo que se arrastra, para el rotulo. */
    label: string;
    /** La pestaña de donde sale (soltarlo sobre ella en la barra no lo suma). */
    self?: number;
    /** Paneles sobre los que no se suelta (el mismo que se arrastra). */
    skip?: (h: { tab: number; leaf: number }) => boolean;
    /** Pestañas a las que no se suma. */
    refuse?: (t: TermTab) => boolean;
    /** El rotulo en un lugar de la barra que no suma. */
    barText: string;
    onStart?: () => void;
    onDrop: (to: Target) => void;
  },
) {
  if (e.button !== 0) return;
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  const sx = e.clientX;
  const sy = e.clientY;
  const orig = { view: app.view, active: app.activeTerm };
  let started = false;
  let bar = false;
  /** La pestaña de la barra bajo el puntero, y si se le puede sumar. */
  let over: number | null = null;
  let joins = false;
  let timer = 0;
  const hover = (key: number | null) => {
    if (key === over) return;
    over = key;
    const dst = key != null && key !== o.self ? app.terms.find((t) => t.key === key) : undefined;
    joins = !!dst && splittable(dst) && !o.refuse?.(dst);
    drop.tabOver = joins ? key : null;
    clearTimeout(timer);
    if (key == null) return;
    timer = window.setTimeout(() => {
      if (over !== key || !app.terms.some((t) => t.key === key)) return;
      app.activeTerm = key;
      app.view = "terminal";
    }, HOVER_SHOW);
  };
  const move = (ev: PointerEvent) => {
    if (!started && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 6) return;
    if (!started) o.onStart?.();
    started = true;
    const barEl = document.querySelector(".bar");
    bar = !!barEl && ev.clientY <= barEl.getBoundingClientRect().bottom;
    hover(bar ? barTabAt(ev.clientX) : null);
    const t = bar ? null : tileAt(ev.clientX, ev.clientY);
    drop.hint = t && !o.skip?.(t) ? t : null;
    const name = joins ? app.terms.find((x) => x.key === over)?.title : undefined;
    const text = !bar ? o.label : name ? `Release to add "${o.label}" to ${name}` : o.barText;
    drop.ghost = { x: ev.clientX, y: ev.clientY, text };
  };
  // Con la captura, el WebView manda el clic del soltar al origen: no cuenta como clic.
  const eat = (ev: MouseEvent) => ev.stopPropagation();
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", up);
    clearTimeout(timer);
    drop.tabOver = null;
    const h = drop.hint;
    drop.hint = null;
    drop.ghost = null;
    if (!started) return;
    window.addEventListener("click", eat, true);
    setTimeout(() => window.removeEventListener("click", eat, true), 0);
    if (bar && joins && over != null) o.onDrop({ tab: over, side: "right" });
    else if (bar) o.onDrop("bar");
    else if (h) o.onDrop(h);
    else {
      if (app.terms.some((t) => t.key === orig.active)) app.activeTerm = orig.active;
      app.view = orig.view;
    }
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
}

/** Arrastrar las aprobaciones desde la barra lateral a una pestaña (ver dragToTabs). */
export function dragApprovals(e: PointerEvent) {
  dragToTabs(e, {
    label: "Approvals",
    skip: isApprovals,
    refuse: (t) => !!t.approvals && !t.layout,
    barText: "Release to open Approvals",
    // Con la barra lateral flotando sobre las terminales: se cierra para ver donde cae.
    onStart: () => (app.sidebarOverlay = false),
    onDrop: (to) => (to === "bar" ? openApprovals() : dropApprovals(to.tab, to.leaf, to.side)),
  });
}
