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
  /** Carpeta donde arranca una shell local. */
  cwd?: string;
  /** Pestaña de archivos (SFTP) en vez de terminal. */
  sftp?: boolean;
  /** Las aprobaciones pendientes en vez de una terminal (pestaña o panel del mosaico). */
  approvals?: boolean;
  /** Bloqueada: no se puede cerrar (ni con la X, ni Ctrl W, ni el boton del medio). */
  locked?: boolean;
  /** Colapsada: en la barra solo se ve el icono (y el candado si esta bloqueada). */
  collapsed?: boolean;
  title: string;
  alive: boolean;
  /** Conectando todavia (la lista de un grupo lo muestra). */
  connecting?: boolean;
  /** Por que fallo la ultima conexion y en que host de la cadena (lo muestra ConnState). */
  error?: string;
  failedAt?: string;
  /** Id de la sesion en el backend, cuando ya conecto. */
  sid?: number;
  /** Viene de otra ventana: tomar esta sesion en vez de abrir una nueva. */
  attach?: number;
  /** Viene de otro panel de esta ventana: tomar su terminal viva (clave de ese panel). */
  adopt?: number;
  /** Con `attach`: el tamaño que tenia, para repetir su historial igual antes de ajustarla. */
  size?: { cols: number; rows: number };
  /** Se esta pasando a otra ventana: al desmontarse no se cierra la sesion. */
  moving?: boolean;
  /** Pestaña de grupo (como Termius): sus terminales, en una lista a la izquierda. Con
   *  `layout`, en cambio, van en mosaico (ver tiles.svelte.ts). */
  members?: TermTab[];
  activeMember?: number;
  /** Como se reparten los paneles de una pestaña en mosaico. */
  layout?: import("./tiles.svelte").Tile;
  /** El panel con el foco, en mosaico. */
  focus?: number;
}

/** Sesiones que se estan mudando (a otra ventana, pestaña o panel): al desmontarse su
 *  terminal no se cierran, las toma la nueva. Por id de sesion, no por objeto. */
export const leaving = new Set<number>();

let lastKey = Date.now();
/** Claves unicas aunque se creen varias en el mismo milisegundo. */
export const newKey = () => ++lastKey;

export interface ConnLine {
  ts: string;
  line: string;
}

export const app = $state({
  status: null as Status | null,
  snap: null as Snapshot | null,
  approvals: [] as Approval[],
  alerts: [] as Alert[],
  view: "hosts" as View,
  terms: [] as TermTab[],
  /** Ventana de terminales sacada de la principal: sin vault, solo pestañas. */
  detached: new URLSearchParams(location.search).get("window") === "term",
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
  /** Los pasos de la ultima conexion a cada host (evento "connlog"), para "Show logs". */
  connLogs: {} as Record<string, ConnLine[]>,
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

/** Pedido de abrir PowerShell en una carpeta; espera a que el vault este abierto. */
let openHere: { cwd: string; reuse: boolean } | null = null;

async function takeOpenHere() {
  const r = await api.takeOpenHere().catch(() => null);
  if (r) openHere = r;
  flushOpenHere();
}

async function flushOpenHere() {
  if (!openHere || !app.status?.unlocked) return;
  const { cwd, reuse } = openHere;
  openHere = null;
  // Atajo sin carpeta y ya hay una terminal local: se va a esa.
  const existing = reuse ? app.terms.findIndex((t) => t.local) : -1;
  if (existing >= 0) return goToTab(existing + 1);
  const shells = await api.localShells().catch(() => []);
  const sh = shells.find((s) => s.id === "pwsh") ?? shells[0];
  if (!sh) return toast("No local shell found", true);
  const folder = cwd.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || cwd;
  openTerminal(`${sh.label} · ${folder}`, sh.id, cwd);
}

export async function refresh() {
  app.status = await api.status();
  flushOpenHere();
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
  listen<Approval[]>("approvals", (e) => {
    app.approvals = e.payload;
  });
  listen<Alert[]>("alerts", (e) => {
    app.alerts = e.payload;
  });
  listen<ConnLine & { host: string }>("connlog", (e) => {
    const { host, ts, line } = e.payload;
    const log = (app.connLogs[host] ??= []);
    log.push({ ts, line });
    if (log.length > 200) log.splice(0, log.length - 200);
  });
  // El atajo global (Ctrl Shift Alt T) con una carpeta del Explorador activa.
  listen("open-here", () => takeOpenHere());
  takeOpenHere();
  listen<UpdateInfo | null>("update", (e) => {
    app.update = e.payload;
  });
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

export function openTerminal(host: string, local?: string, cwd?: string) {
  rememberVaultView(app.view);
  const key = newKey();
  const n = app.terms.filter((t) => t.host === host).length;
  app.terms.push({ key, host, local, cwd, title: n ? `${host} (${n + 1})` : host, alive: true });
  app.activeTerm = key;
  app.view = "terminal";
}

export function openSftp(host: string) {
  rememberVaultView(app.view);
  const key = newKey();
  app.terms.push({ key, host, sftp: true, title: `${host} · SFTP`, alive: true });
  app.activeTerm = key;
  app.view = "terminal";
}

/** Muestra las aprobaciones en una pestaña. Si esta ventana ya las tiene (sola o como panel
 *  de un mosaico), va a esa en vez de abrir otra. */
export function openApprovals() {
  rememberVaultView(app.view);
  for (const t of app.terms) {
    const m = t.approvals ? t : t.members?.find((x) => x.approvals);
    if (!m) continue;
    if (t.layout && m !== t) t.focus = m.key;
    else if (t.members && m !== t) t.activeMember = m.key;
    app.activeTerm = t.key;
    app.view = "terminal";
    return;
  }
  const key = newKey();
  app.terms.push({ key, host: "Approvals", approvals: true, title: "Approvals", alive: true });
  app.activeTerm = key;
  app.view = "terminal";
}

/** Abre todos los hosts de un grupo en una sola pestaña, con la lista a la izquierda. */
export function openGroup(group: string, hosts: string[]) {
  if (!hosts.length) return;
  rememberVaultView(app.view);
  const members: TermTab[] = hosts.map((h) => ({ key: newKey(), host: h, title: h, alive: true, connecting: true }));
  const key = newKey();
  app.terms.push({ key, host: group, title: group, alive: true, members, activeMember: members[0].key });
  app.activeTerm = key;
  app.view = "terminal";
}

/** Cierra una terminal de un grupo; si era la ultima, se cierra la pestaña. */
export function closeMember(ws: TermTab, key: number) {
  const list = ws.members ?? [];
  const i = list.findIndex((m) => m.key === key);
  if (i < 0) return;
  if (list.length === 1 && lockedWarn(ws)) return;
  list.splice(i, 1);
  if (!list.length) return closeTerminal(ws.key);
  if (ws.activeMember === key) ws.activeMember = list[Math.max(0, i - 1)].key;
}

/** Avisa (y devuelve true) si la pestaña esta bloqueada y no se puede cerrar. */
export function lockedWarn(t: TermTab | undefined): boolean {
  if (!t?.locked) return false;
  toast("This tab is locked. Unlock it from its right-click menu to close it.");
  return true;
}

/** La persona cierra una pestaña: si esta bloqueada, no. */
export function closeTab(key: number) {
  if (!lockedWarn(app.terms.find((t) => t.key === key))) closeTerminal(key);
}

/** Saca la pestaña de la lista (cerrandola o porque se va a otra ventana). */
export function closeTerminal(key: number) {
  const i = app.terms.findIndex((t) => t.key === key);
  if (i < 0) return;
  app.terms.splice(i, 1);
  if (app.activeTerm === key) {
    const next = app.terms[Math.max(0, i - 1)];
    app.activeTerm = next?.key ?? 0;
    if (!next) app.view = lastVaultView;
  }
  // Una ventana de terminales sin pestañas se cierra.
  if (app.detached && !app.terms.length) import("@tauri-apps/api/window").then((w) => w.getCurrentWindow().close());
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
  if (app.detached && index <= 0) index = 1; // sin pestaña Vault
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
