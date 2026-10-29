// Atajos de teclado de la app. Las combinaciones salen de prefs (se cambian en Settings);
// los de copiar y pegar de la terminal los maneja TermPane.
import { api } from "./api";
import { comboOf, matches, prefs } from "./prefs.svelte";
import { app, closeMember, closeTerminal, currentTabIndex, goToTab, openTerminal, refresh, toast, toggleSidebar } from "./state.svelte";
import { cssVar } from "./themes";
import { closeLeaf } from "./tiles.svelte";

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
    const down = (top?.members ?? []).filter((m) => !m.sftp && !m.alive && !m.connecting);
    if (down.length) {
      for (const m of down) window.dispatchEvent(new CustomEvent("ethereon:refresh", { detail: m.key }));
      return true;
    }
    // En una pestaña de grupo, cuenta la terminal elegida de la lista.
    const t = top?.members ? top.members.find((m) => m.key === (top.layout ? top.focus : top.activeMember)) : top;
    if (!t) return true;
    if (!t.sftp && t.alive && inTerminal(e)) return false; // la shell la usa (buscar en el historial)
    window.dispatchEvent(new CustomEvent("ethereon:refresh", { detail: t.key }));
    return true;
  }
  refresh();
  return true;
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
      else closeTerminal(app.activeTerm);
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
}
