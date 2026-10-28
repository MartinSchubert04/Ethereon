// Atajos de teclado de la app. Las combinaciones salen de prefs (se cambian en Settings);
// los de copiar y pegar de la terminal los maneja TermPane.
import { api } from "./api";
import { comboOf, matches, prefs } from "./prefs.svelte";
import { app, closeTerminal, currentTabIndex, goToTab, toggleSidebar } from "./state.svelte";

function inTerminal(e: KeyboardEvent) {
  return !!(e.target as HTMLElement | null)?.closest?.(".xterm");
}

function typing(e: KeyboardEvent) {
  const t = e.target as HTMLElement | null;
  return !!t?.closest?.("input, textarea, [contenteditable]");
}

function tabCount() {
  return app.terms.length + 1;
}

function onKey(e: KeyboardEvent) {
  if (!app.status?.unlocked || prefs.recording) return;
  const combo = comboOf(e);
  if (!combo) return;
  // Un atajo sin Ctrl/Alt no corre mientras se escribe en un campo.
  if (!e.ctrlKey && !e.altKey && !e.metaKey && typing(e)) return;
  const term = inTerminal(e);
  let handled = true;

  if (matches(e, "newTab")) app.quickOpen = true;
  else if (matches(e, "closeTabAnywhere") || (matches(e, "closeTab") && !(term && prefs.term.ctrlWToShell))) {
    if (app.view === "terminal" && app.activeTerm) closeTerminal(app.activeTerm);
    else handled = false;
  } else if (matches(e, "nextTab")) goToTab((currentTabIndex() + 1) % tabCount());
  else if (matches(e, "prevTab")) goToTab((currentTabIndex() + tabCount() - 1) % tabCount());
  else if (e.ctrlKey && !e.shiftKey && !e.altKey && /^Digit[1-9]$/.test(e.code))
    goToTab(Math.min(Number(e.code.slice(5)), tabCount()) - 1);
  else if (matches(e, "sidebar") && !term) toggleSidebar();
  else if (matches(e, "settings") && !term) {
    goToTab(0);
    app.view = "settings";
  } else if (matches(e, "search") && !term) {
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
