// Estado global de la interfaz. El backend avisa con eventos y aca se vuelve a pedir.
import { listen } from "@tauri-apps/api/event";
import { api, errText, type Alert, type Approval, type Snapshot, type Status, type UpdateInfo } from "./api";

export type View = "hosts" | "keychain" | "rules" | "approvals" | "agents" | "audit" | "settings" | "terminal";

export interface TermTab {
  key: number;
  /** Host del vault, o el nombre de la shell si es local. */
  host: string;
  /** Id de la shell local (pwsh, cmd, wsl...); vacio para SSH. */
  local?: string;
  /** Pestaña de archivos (SFTP) en vez de terminal. */
  sftp?: boolean;
  title: string;
  alive: boolean;
}

export const app = $state({
  status: null as Status | null,
  snap: null as Snapshot | null,
  approvals: [] as Approval[],
  alerts: [] as Alert[],
  view: "hosts" as View,
  terms: [] as TermTab[],
  activeTerm: 0,
  /** Buscador de hosts de "New tab" (Ctrl T). */
  quickOpen: false,
  /** En una terminal, ≡ muestra la barra lateral flotando encima. */
  sidebarOverlay: false,
  sidebarCollapsed: (() => {
    try {
      return localStorage.getItem("ethereon.sidebar") === "collapsed";
    } catch {
      return false;
    }
  })(),
  /** Ctrl N pide un formulario nuevo; la vista que corresponde lo abre y lo limpia. */
  pendingNew: null as "host" | "credential" | null,
  update: null as UpdateInfo | null,
  toasts: [] as { id: number; text: string; bad: boolean }[],
});

let toastId = 0;
export function toast(text: string, bad = false) {
  const id = ++toastId;
  app.toasts.push({ id, text, bad });
  setTimeout(() => (app.toasts = app.toasts.filter((t) => t.id !== id)), bad ? 6000 : 3000);
}

/** Corre `f` y muestra el error si falla. Devuelve si salio bien. */
export async function attempt(f: () => Promise<unknown>, ok?: string): Promise<boolean> {
  try {
    await f();
    if (ok) toast(ok);
    return true;
  } catch (e) {
    toast(errText(e), true);
    return false;
  }
}

export async function refresh() {
  app.status = await api.status();
  if (app.status.unlocked) {
    const [snap, approvals, alerts] = await Promise.all([api.snapshot(), api.approvals(), api.alerts()]);
    app.snap = snap;
    app.approvals = approvals;
    app.alerts = alerts;
  } else {
    app.snap = null;
    app.approvals = [];
  }
}

let started = false;
export function start() {
  if (started) return;
  started = true;
  listen("vault", () => refresh());
  listen("data", () => refresh());
  listen<Approval[]>("approvals", (e) => (app.approvals = e.payload));
  listen<Alert[]>("alerts", (e) => (app.alerts = e.payload));
  listen<UpdateInfo | null>("update", (e) => (app.update = e.payload));
  api.updateStatus().then((u) => (app.update = u));
  // Usar la interfaz posterga el autobloqueo (como mucho un aviso cada 30 s).
  let last = 0;
  const touch = () => {
    const now = Date.now();
    if (app.status?.unlocked && now - last > 30_000) {
      last = now;
      api.touch();
    }
  };
  window.addEventListener("pointerdown", touch);
  window.addEventListener("keydown", touch);
  refresh();
}

export function openTerminal(host: string, local?: string) {
  rememberVaultView(app.view);
  const key = Date.now();
  const n = app.terms.filter((t) => t.host === host).length;
  app.terms.push({ key, host, local, title: n ? `${host} (${n + 1})` : host, alive: true });
  app.activeTerm = key;
  app.view = "terminal";
}

export function openSftp(host: string) {
  rememberVaultView(app.view);
  const key = Date.now();
  app.terms.push({ key, host, sftp: true, title: `${host} · SFTP`, alive: true });
  app.activeTerm = key;
  app.view = "terminal";
}

export function closeTerminal(key: number) {
  const i = app.terms.findIndex((t) => t.key === key);
  if (i < 0) return;
  app.terms.splice(i, 1);
  if (app.activeTerm === key) {
    const next = app.terms[Math.max(0, i - 1)];
    app.activeTerm = next?.key ?? 0;
    if (!next) app.view = lastVaultView;
  }
}

export function toggleSidebar() {
  if (app.view === "terminal") {
    app.sidebarOverlay = !app.sidebarOverlay;
    return;
  }
  app.sidebarCollapsed = !app.sidebarCollapsed;
  try {
    localStorage.setItem("ethereon.sidebar", app.sidebarCollapsed ? "collapsed" : "open");
  } catch {}
}

/** Pestañas en orden: la del vault (0) y despues las terminales. */
export function goToTab(index: number) {
  if (index <= 0) {
    if (app.view === "terminal") app.view = lastVaultView;
    return;
  }
  const t = app.terms[index - 1];
  if (t) {
    if (app.view !== "terminal") lastVaultView = app.view;
    app.activeTerm = t.key;
    app.view = "terminal";
  }
}

export function currentTabIndex(): number {
  if (app.view !== "terminal") return 0;
  return app.terms.findIndex((t) => t.key === app.activeTerm) + 1;
}

/** La ultima vista del vault, para volver a ella desde una terminal. */
let lastVaultView: View = "hosts";
export function rememberVaultView(v: View) {
  if (v !== "terminal") lastVaultView = v;
}
