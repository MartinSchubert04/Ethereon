// Atajos de teclado. La lista de SHORTCUTS es lo que muestra Ajustes: si se cambia un
// atajo, se cambia ahi y en `onKey`.
import { api } from "./api";
import { app, closeTerminal, currentTabIndex, goToTab, toggleSidebar } from "./state.svelte";

export const SHORTCUTS: [string, string][] = [
  ["Ctrl T", "New tab (pick a host)"],
  ["Ctrl W", "Close tab (in a terminal: Ctrl Shift W, Ctrl W goes to the shell)"],
  ["Ctrl Tab / Ctrl Shift Tab", "Next / previous tab"],
  ["Ctrl 1 … 9", "Go to tab (1 is the vault)"],
  ["Ctrl B", "Show or hide the sidebar"],
  ["Ctrl F", "Search hosts"],
  ["Ctrl ,", "Settings"],
  ["Ctrl Shift L", "Lock the vault"],
  ["Esc", "Close a panel or dialog"],
];

function inTerminal(e: KeyboardEvent) {
  return !!(e.target as HTMLElement | null)?.closest?.(".xterm");
}

function tabCount() {
  return app.terms.length + 1;
}

function onKey(e: KeyboardEvent) {
  if (!app.status?.unlocked || !e.ctrlKey || e.altKey) return;
  const k = e.key.toLowerCase();
  const term = inTerminal(e);
  let handled = true;

  if (k === "t" && !e.shiftKey) app.quickOpen = true;
  else if (k === "w" && (e.shiftKey || !term)) {
    if (app.view === "terminal" && app.activeTerm) closeTerminal(app.activeTerm);
    else handled = false;
  } else if (e.key === "Tab") {
    const n = tabCount();
    goToTab((currentTabIndex() + (e.shiftKey ? n - 1 : 1)) % n);
  } else if (/^[1-9]$/.test(e.key) && !e.shiftKey) goToTab(Math.min(Number(e.key), tabCount()) - 1);
  else if (k === "b" && !e.shiftKey && !term) toggleSidebar();
  else if (k === "," && !term) {
    goToTab(0);
    app.view = "settings";
  } else if (k === "f" && !e.shiftKey && !term) {
    goToTab(0);
    app.view = "hosts";
    queueMicrotask(() => document.querySelector<HTMLInputElement>("[data-host-search]")?.focus());
  } else if (k === "l" && e.shiftKey) api.lock();
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
