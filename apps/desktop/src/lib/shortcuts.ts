// Atajos de teclado de la app. Las combinaciones salen de prefs (se cambian en Settings);
// los de copiar y pegar de la terminal los maneja TermPane.
import { api } from "./api";
import { comboOf, matches, prefs } from "./prefs.svelte";
import { app, closeMember, closeTab, currentTabIndex, goToTab, openTerminal, refresh, toast, toggleSidebar } from "./state.svelte";
import { cssVar } from "./themes";
import { closeLeaf, drop, geometry, leaves, moveLeaf, moveToEdge, swapLeaves, type Rect, type Side } from "./tiles.svelte";

function zoom(d: number) {
  const base = parseInt(cssVar("--term-font-size")) || 13;
  const now = prefs.term.fontSize || base;
  const next = d === 0 ? 0 : Math.min(28, Math.max(9, now + d));
  prefs.term.fontSize = next;
  toast(`Terminal text: ${next || base} px`);
}

async function newLocal() {
  const [first] = await api.localShells().catch(() => []);
  if (first) openTerminal(first.label, first.id);
  else toast("No local shell found", true);
}

function inTerminal(e: KeyboardEvent) {
  return !!(e.target as HTMLElement | null)?.closest?.(".xterm");
}

/** Dentro de un panel de archivos (SFTP), que tiene sus propias teclas. */
function inFiles(e: KeyboardEvent) {
  return !!(e.target as HTMLElement | null)?.closest?.(".fp");
}

function typing(e: KeyboardEvent) {
  const t = e.target as HTMLElement | null;
  return !!t?.closest?.("input, textarea, [contenteditable]");
}

function tabCount() {
  return app.terms.length + 1;
}

/** Ctrl R: refresca la pestaña actual. Si es una terminal conectada, la tecla es de la shell. */
function refreshTab(e: KeyboardEvent): boolean {
  if (app.view === "terminal") {
    const top = app.terms.find((x) => x.key === app.activeTerm);
    // Con varias terminales en la pestaña se reintentan las caidas, y solo esas: las que
    // estan conectadas siguen como estan. Si no hay ninguna caida, Ctrl R es de la shell.
    const down = (top?.members ?? []).filter((m) => !m.sftp && !m.approvals && !m.alive && !m.connecting);
    if (down.length) {
      for (const m of down) window.dispatchEvent(new CustomEvent("ethereon:refresh", { detail: m.key }));
      return true;
    }
    // En una pestaña de grupo, cuenta la terminal elegida de la lista.
    const t = top?.members ? top.members.find((m) => m.key === (top.layout ? top.focus : top.activeMember)) : top;
    if (!t) return true;
    // Las aprobaciones no tienen conexion: Ctrl R vuelve a pedir la lista.
    if (t.approvals) {
      refresh();
      return true;
    }
    if (!t.sftp && t.alive && inTerminal(e)) return false; // la shell la usa (buscar en el historial)
    window.dispatchEvent(new CustomEvent("ethereon:refresh", { detail: t.key }));
    return true;
  }
  refresh();
  return true;
}

/** Pasa el foco a la siguiente terminal de la pestaña (mosaico o grupo). Solo si la
 *  pestaña tiene varias: si no, la tecla sigue siendo de la shell (Shift Tab completa hacia
 *  atras en PowerShell). */
function nextPane(): boolean {
  if (app.view !== "terminal") return false;
  const top = app.terms.find((x) => x.key === app.activeTerm);
  if (!top?.members || top.members.length < 2) return false;
  const order = top.layout ? leaves(top.layout) : top.members.map((m) => m.key);
  const cur = top.layout ? top.focus : top.activeMember;
  const next = order[(order.indexOf(cur ?? -1) + 1) % order.length];
  if (top.layout) top.focus = next;
  else top.activeMember = next;
  return true;
}

// ---- mover la terminal del mosaico con el teclado
//
// Con los modificadores del atajo apretados (Ctrl Shift), las flechas mueven un cursor y la
// marca muestra a donde iria la terminal con el foco. Se mueve recien al soltar los
// modificadores; Esc cancela. Lo que muestra la marca:
//   - otra terminal entera: se intercambian (la propia, punteada: se queda donde esta).
//   - la mitad de una terminal: va a ese costado de esa (se parte su lugar).
//   - un lado de todo el mosaico: lo ocupa entero.
// Flecha hacia otra terminal: el cursor salta a la mas cercana de ese lado, recordando la
// altura o la columna en la que venia (de una sola a la derecha, ← va a la de arriba de las
// dos de la izquierda; desde ahi ↓ va a la de abajo). Flecha sin nada mas de ese lado: pasa
// a ese costado de la terminal; la misma otra vez, al lado del mosaico entero. La flecha
// contraria deshace ese paso.

const ARROW_SIDE: Record<string, Side> = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "top", ArrowDown: "bottom" };
const OPPOSITE: Record<Side, Side> = { left: "right", right: "left", top: "bottom", bottom: "top" };
const EPS = 1e-6;

interface Moving {
  tab: number;
  /** La terminal que se mueve (la del foco). */
  key: number;
  /** Donde esta el cursor. */
  leaf: number;
  mode: "swap" | "side" | "edge";
  side: Side;
  /** La altura y la columna recordadas, para elegir entre varias vecinas. */
  ax: number;
  ay: number;
}
let moving: Moving | null = null;

/** Los modificadores del atajo ("Ctrl+Shift+Arrows" -> Ctrl, Shift): la tecla no cuenta. */
function moveMods(): string[] {
  const b = prefs.binds.movePane;
  return b ? b.split("+").slice(0, -1) : [];
}
const heldMods = (e: KeyboardEvent) => [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Win"].filter(Boolean);

/** La terminal vecina de `from` hacia `side`: la que esta pegada (o la mas cercana) y, entre
 *  esas, la que contiene la altura o columna recordada; si ninguna, la de arriba o izquierda. */
function neighbor(panes: Map<number, Rect>, from: number, side: Side, ax: number, ay: number): number | undefined {
  const r = panes.get(from);
  if (!r) return undefined;
  const across = side === "left" || side === "right";
  const found: { k: number; gap: number; holds: boolean; start: number }[] = [];
  for (const [k, q] of panes) {
    if (k === from) continue;
    const gap =
      side === "left" ? r.x - (q.x + q.w) : side === "right" ? q.x - (r.x + r.w) : side === "top" ? r.y - (q.y + q.h) : q.y - (r.y + r.h);
    if (gap < -EPS) continue; // no esta de ese lado
    const overlap = across ? Math.min(q.y + q.h, r.y + r.h) - Math.max(q.y, r.y) : Math.min(q.x + q.w, r.x + r.w) - Math.max(q.x, r.x);
    if (overlap <= EPS) continue; // de ese lado, pero en diagonal
    // Un poco antes del valor recordado: en un borde justo gana la de arriba o la de la izquierda.
    const a = (across ? ay : ax) - EPS;
    const start = across ? q.y : q.x;
    const end = across ? q.y + q.h : q.x + q.w;
    found.push({ k, gap: Math.max(0, gap), holds: a >= start && a < end, start });
  }
  found.sort((p, q) => p.gap - q.gap || Number(q.holds) - Number(p.holds) || p.start - q.start);
  return found[0]?.k;
}

function movePaneKey(e: KeyboardEvent): boolean {
  const side = ARROW_SIDE[e.key];
  const mods = moveMods();
  if (!side || !mods.length || heldMods(e).join("+") !== mods.join("+") || app.view !== "terminal") return false;
  const top = app.terms.find((x) => x.key === app.activeTerm);
  // Solo en un mosaico con varias: si no, la tecla es de la shell (en PowerShell selecciona
  // por palabras).
  if (!top?.layout || !top.focus || (top.members?.length ?? 0) < 2) return false;
  const panes = geometry(top.layout).panes;
  if (!moving || moving.tab !== top.key || moving.key !== top.focus) {
    const r = panes.get(top.focus);
    if (!r) return true;
    moving = { tab: top.key, key: top.focus, leaf: top.focus, mode: "swap", side, ax: r.x + r.w / 2, ay: r.y + r.h / 2 };
  }
  const m = moving;
  const go = (k: number) => {
    const q = panes.get(k)!;
    m.leaf = k;
    m.mode = "swap";
    // Lo que se recorre se actualiza; lo otro se recuerda, dentro de la nueva terminal.
    if (side === "left" || side === "right") {
      m.ax = q.x + q.w / 2;
      m.ay = Math.min(Math.max(m.ay, q.y), q.y + q.h);
    } else {
      m.ay = q.y + q.h / 2;
      m.ax = Math.min(Math.max(m.ax, q.x), q.x + q.w);
    }
  };
  if (m.mode === "edge") {
    // Desde el borde del mosaico: la contraria vuelve atras; otra flecha, otro borde.
    if (side === OPPOSITE[m.side]) m.mode = m.leaf === m.key ? "swap" : "side";
    else m.side = side;
  } else if (m.mode === "side") {
    if (side === OPPOSITE[m.side]) m.mode = "swap";
    else if (side === m.side) m.mode = "edge";
    else {
      const n = neighbor(panes, m.leaf, side, m.ax, m.ay);
      if (n != null) go(n);
      else m.side = side;
    }
  } else {
    const n = neighbor(panes, m.leaf, side, m.ax, m.ay);
    if (n != null) go(n);
    // Desde la propia terminal, al costado de si misma no tiene sentido: directo al borde.
    else if (m.leaf === m.key) {
      m.mode = "edge";
      m.side = side;
    } else {
      m.mode = "side";
      m.side = side;
    }
  }
  drop.swap = m.mode === "swap" ? { tab: m.tab, leaf: m.leaf } : null;
  drop.hint = m.mode === "side" ? { tab: m.tab, leaf: m.leaf, side: m.side } : null;
  drop.edge = m.mode === "edge" ? { tab: m.tab, side: m.side } : null;
  return true;
}

function cancelMove() {
  if (!moving) return;
  moving = null;
  drop.hint = null;
  drop.swap = null;
  drop.edge = null;
}

/** Se soltaron los modificadores: ahi recien se mueve. */
function onKeyUp(e: KeyboardEvent) {
  if (!moving) return;
  if (heldMods(e).join("+") === moveMods().join("+")) return;
  const m = moving;
  cancelMove();
  const t = app.terms.find((x) => x.key === m.tab);
  if (!t?.members?.some((x) => x.key === m.leaf) || !t.members.some((x) => x.key === m.key)) return;
  if (m.mode === "swap") swapLeaves(t, m.key, m.leaf);
  else if (m.mode === "side" && m.leaf !== m.key) moveLeaf(t, m.key, m.leaf, m.side);
  else if (m.mode === "edge") moveToEdge(t, m.key, m.side);
  t.focus = m.key;
}

/** Teclas con las que el WebView recargaria la pagina y cerraria todas las pestañas. */
function isReload(e: KeyboardEvent) {
  return e.key === "F5" || ((e.ctrlKey || e.metaKey) && e.code === "KeyR");
}

function onKey(e: KeyboardEvent) {
  if (prefs.recording) return;
  if (app.status?.unlocked && matches(e, "refresh") && !document.querySelector(".drawer, [role=alertdialog]")) {
    if (refreshTab(e)) {
      e.preventDefault();
      e.stopPropagation();
    }
    return;
  }
  // Mover la terminal del mosaico: tambien antes del filtro de "escribiendo".
  if (moving && e.key === "Escape") {
    cancelMove();
    e.preventDefault();
    e.stopPropagation();
    return;
  }
  if (app.status?.unlocked && !document.querySelector(".drawer, [role=alertdialog]") && movePaneKey(e)) {
    e.preventDefault();
    e.stopPropagation();
    return;
  }
  // Antes del filtro de "escribiendo": desde una terminal es justamente donde se usa.
  if (app.status?.unlocked && matches(e, "nextPane") && !document.querySelector(".drawer, [role=alertdialog]") && nextPane()) {
    e.preventDefault();
    e.stopPropagation();
    return;
  }
  // Nunca recargar. Sin stopPropagation: F5 sigue llegando al panel de archivos.
  if (isReload(e) && !inTerminal(e)) e.preventDefault();
  if (!app.status?.unlocked) return;
  const combo = comboOf(e);
  if (!combo) return;
  // Un atajo sin Ctrl/Alt no corre mientras se escribe en un campo.
  if (!e.ctrlKey && !e.altKey && !e.metaKey && typing(e)) return;
  const term = inTerminal(e);
  let handled = true;

  // Con un formulario o un dialogo abierto, los atajos de navegacion no corren (el
  // formulario maneja Ctrl S y Esc por su cuenta).
  const modal = !!document.querySelector(".drawer, [role=alertdialog]");

  if (matches(e, "newTab") && !modal) app.quickOpen = true;
  else if (matches(e, "newLocal") && !modal) newLocal();
  else if (matches(e, "newItem") && !modal && !term && !inFiles(e)) {
    goToTab(0);
    app.pendingNew = app.view === "keychain" ? "credential" : "host";
    if (app.view !== "keychain") app.view = "hosts";
  } else if (matches(e, "zoomIn") && app.view === "terminal") zoom(1);
  else if (matches(e, "zoomOut") && app.view === "terminal") zoom(-1);
  else if (matches(e, "zoomReset") && app.view === "terminal") zoom(0);
  else if (matches(e, "closeTabAnywhere") || (matches(e, "closeTab") && !(term && prefs.term.ctrlWToShell))) {
    if (app.view === "terminal" && app.activeTerm) {
      // En una pestaña de grupo se cierra la terminal elegida, no todo el grupo.
      const top = app.terms.find((x) => x.key === app.activeTerm);
      if (top?.layout && top.focus) closeLeaf(top, top.focus);
      else if (top?.members && top.activeMember) closeMember(top, top.activeMember);
      else closeTab(app.activeTerm);
    } else handled = false;
  } else if (matches(e, "nextTab")) goToTab((currentTabIndex() + 1) % tabCount());
  else if (matches(e, "prevTab")) goToTab((currentTabIndex() + tabCount() - 1) % tabCount());
  else if (e.ctrlKey && !e.shiftKey && !e.altKey && /^Digit[1-9]$/.test(e.code))
    goToTab(Math.min(Number(e.code.slice(5)), tabCount()) - 1);
  else if (matches(e, "sidebar") && !term) toggleSidebar();
  else if (matches(e, "settings") && !term) {
    goToTab(0);
    app.view = "settings";
  } else if (matches(e, "search") && !term && !inFiles(e)) {
    goToTab(0);
    app.view = "hosts";
    setTimeout(() => document.querySelector<HTMLInputElement>("[data-host-search]")?.focus(), 30);
  } else if (matches(e, "lock")) api.lock();
  else handled = false;

  if (handled) {
    e.preventDefault();
    e.stopPropagation();
  }
}

let installed = false;
export function installShortcuts() {
  if (installed) return;
  installed = true;
  // En captura: llega antes que xterm, que si no se queda con las teclas.
  window.addEventListener("keydown", onKey, true);
  window.addEventListener("keyup", onKeyUp, true);
  // Si la ventana pierde el foco con los modificadores apretados, el soltar no llega aca.
  window.addEventListener("blur", cancelMove);
}
