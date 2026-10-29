/// <reference types="vite/client" />
// Modo debug: un panel con lo que pasa con las pestañas y las terminales (ver docs/DEBUG.md).
// Solo con `npm run dev`, o en la app instalada con localStorage "ethereon.debug" = "1".
//
// El panel muestra cada terminal que se ve (tamaño, filas, canvas, si dibuja, que elemento
// queda encima del centro) y lo ultimo que paso. F9 marca "aca se ve mal" con una foto del
// estado; Copy copia todo el registro para pegarlo.
import type { Terminal } from "@xterm/xterm";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { app } from "./state.svelte";

function flag(): boolean {
  try {
    return localStorage.getItem("ethereon.debug") === "1";
  } catch {
    return false;
  }
}
export const DEBUG = import.meta.env.DEV || flag();

/** Interruptores para aislar que deja la terminal en negro: cada uno apaga una sospecha.
 *  Se guardan entre recargas. */
export const sw = { capture: true, focus: true, tips: true, webgl: true };
type Sw = keyof typeof sw;
const SW_HELP: Record<Sw, string> = {
  capture: "Off: the tab grabs the mouse only once a drag starts, not on press",
  focus: "Off: pressing a tab doesn't take keyboard focus from the terminal",
  tips: "Off: no tooltips",
  webgl: "Off: new terminals draw without WebGL (open a new one to try)",
};
try {
  Object.assign(sw, JSON.parse(localStorage.getItem("ethereon.dbgsw") ?? "{}"));
} catch {}
if (!DEBUG) Object.assign(sw, { capture: true, focus: true, tips: true, webgl: true });

interface Info {
  key: number;
  renders: number;
  lastRender: number;
  gl: boolean;
}
const terms = new Map<Terminal, Info>();
const lines: string[] = [];
const MAX = 800;

const stamp = () => {
  const d = new Date();
  return `${d.toTimeString().slice(0, 8)}.${String(d.getMilliseconds()).padStart(3, "0")}`;
};

/** Anota algo. Con `snap`, ademas saca fotos del estado un poco despues (lo que pasa al
 *  cuadro siguiente y cuando ya se acomodo). */
export function dbg(what: string, data?: unknown, snap = false) {
  if (!DEBUG) return;
  push(`${what}${data !== undefined ? " " + JSON.stringify(data) : ""}`);
  if (snap) {
    setTimeout(() => shot("  +80ms"), 80);
    setTimeout(() => shot("  +700ms"), 700);
  }
}

function push(line: string) {
  const l = `${stamp()} ${line}`;
  lines.push(l);
  if (lines.length > MAX) lines.splice(0, lines.length - MAX);
  console.debug("[ethereon]", line);
  send(l);
  draw();
}

// Con npm run dev, el registro tambien va a target/ui-debug.log (ver vite.config.ts).
const outbox: string[] = [];
let sendTimer: ReturnType<typeof setTimeout> | undefined;
const from = new URLSearchParams(location.search).get("window") ?? "main";
function send(l: string) {
  if (!import.meta.hot) return;
  outbox.push(l);
  sendTimer ??= setTimeout(() => {
    sendTimer = undefined;
    import.meta.hot?.send("ethereon:debug", { from, lines: outbox.splice(0) });
  }, 300);
}

/** Una terminal (nueva o adoptada por otro panel) ahora es el panel `key`. */
export function dbgTerm(term: Terminal, key: number, gl: boolean) {
  if (!DEBUG) return;
  let i = terms.get(term);
  if (!i) {
    i = { key, renders: 0, lastRender: 0, gl };
    terms.set(term, i);
    const info = i;
    term.onRender(() => {
      info.renders++;
      info.lastRender = performance.now();
    });
  }
  i.key = key;
}
export function dbgGlLost(term: Terminal) {
  const i = terms.get(term);
  if (i) i.gl = false;
  dbg("webgl context lost", { key: i?.key }, true);
}
export function dbgGone(term: Terminal) {
  terms.delete(term);
}

const describe = (e: Element) => {
  const cls = typeof e.className === "string" ? e.className.replace(/svelte-\w+/g, "").trim().split(/\s+/).filter(Boolean).slice(0, 3).join(".") : "";
  return e.tagName.toLowerCase() + (cls ? "." + cls : "");
};

/** Por que no se veria: el primer ancestro oculto, transparente o movido. */
function hiddenBy(x: HTMLElement): string {
  for (let e: HTMLElement | null = x; e && e !== document.body; e = e.parentElement) {
    const s = getComputedStyle(e);
    if (s.display === "none") return `display:none ${describe(e)}`;
    if (Number(s.opacity) < 0.99) return `opacity:${s.opacity} ${describe(e)}`;
    if (s.transform !== "none" && e !== x) return `transform ${describe(e)}`;
  }
  return "";
}

/** El color del medio de un canvas 2D (rgba), para ver si tapa con algo opaco. Los de WebGL
 *  no se pueden leer despues de mostrarse. */
function center(c: HTMLCanvasElement): string {
  if (!c.width || !c.height) return "";
  try {
    const ctx = c.getContext("2d", { willReadFrequently: true });
    if (!ctx) return " (gl)";
    const d = ctx.getImageData(c.width >> 1, c.height >> 1, 1, 1).data;
    return ` medio=${d[0]},${d[1]},${d[2]},${d[3]}`;
  } catch {
    return "";
  }
}

/** Una linea por terminal; `all` incluye las ocultas. */
function state(all: boolean): string[] {
  const out = [`view=${app.view} active=${app.activeTerm} tabs=[${app.terms.map((t) => t.key + (t.layout ? "M" : t.members ? "G" : "")).join(",")}]`];
  for (const [term, i] of terms) {
    const x = term.element;
    if (!x) {
      out.push(`#${i.key} sin elemento`);
      continue;
    }
    const vis = getComputedStyle(x).visibility;
    if (!all && vis !== "visible") continue;
    const r = x.getBoundingClientRect();
    let top = "-";
    if (vis === "visible" && r.width > 0 && r.height > 0) {
      const first = document.elementsFromPoint(r.x + r.width / 2, r.y + r.height / 2).find((e) => !box?.contains(e));
      top = !first ? "nada" : x.contains(first) ? "xterm" : describe(first);
    }
    const canv = [...x.querySelectorAll("canvas")].map((c) => `${c.className || "?"} ${c.width}x${c.height}${getComputedStyle(c).display === "none" ? "(none)" : ""}${center(c)}`).join(" | ");
    const ago = i.lastRender ? `${Math.round((performance.now() - i.lastRender) / 100) / 10}s` : "nunca";
    const hb = hiddenBy(x);
    out.push(
      `#${i.key} ${vis} ${Math.round(r.width)}x${Math.round(r.height)}@${Math.round(r.x)},${Math.round(r.y)} ${term.cols}x${term.rows} ` +
        `gl=${i.gl ? "si" : "no"} dibujos=${i.renders} ultimo=${ago} canvas=[${canv}] encima=${top}${hb ? " oculta:" + hb : ""}`,
    );
  }
  return out;
}

function shot(label: string) {
  push(label);
  for (const l of state(false)) push("    " + l);
}

// ---- el panel

let box: HTMLDivElement | null = null;
let live: HTMLPreElement;
let log: HTMLPreElement;
let small = false;

function draw() {
  if (!box || small) return;
  log.textContent = lines.slice(-16).join("\n");
  log.scrollTop = log.scrollHeight;
}

function tick() {
  if (box && !small) live.textContent = state(false).join("\n");
}

function button(label: string, title: string, f: () => void) {
  const b = document.createElement("button");
  b.textContent = label;
  b.title = title;
  b.onclick = (e) => {
    e.stopPropagation();
    f();
  };
  return b;
}

function mark() {
  push("==== MARCA: se ve mal ====");
  for (const l of state(true)) push("    " + l);
}

function redraw() {
  push("redibujar todas");
  for (const term of terms.keys()) term.refresh(0, term.rows - 1);
  setTimeout(() => shot("  despues de redibujar"), 150);
}

export function installDebug() {
  if (!DEBUG || box) return;
  const css = document.createElement("style");
  css.textContent = `
    .dbg { position: fixed; right: 8px; bottom: 8px; z-index: 9999; width: min(720px, calc(100vw - 16px));
      font: 11px/1.35 ui-monospace, monospace; color: var(--text); background: var(--panel);
      border: 1px solid var(--accent); border-radius: 6px; opacity: 0.94; pointer-events: auto; }
    .dbg.small { width: auto; }
    .dbg .row { display: flex; gap: 6px; align-items: center; padding: 4px 6px; border-bottom: 1px solid var(--line); }
    .dbg .row b { color: var(--accent); margin-right: auto; }
    .dbg button { font: inherit; color: var(--text); background: var(--panel-2, transparent); border: 1px solid var(--line);
      border-radius: 4px; padding: 1px 6px; cursor: pointer; }
    .dbg pre { margin: 0; padding: 4px 6px; white-space: pre-wrap; word-break: break-all; max-height: 170px; overflow: auto; }
    .dbg pre.live { color: var(--text-2); border-bottom: 1px solid var(--line); }
    .dbg.small pre { display: none; }
  `;
  document.head.appendChild(css);
  box = document.createElement("div");
  box.className = "dbg";
  const row = document.createElement("div");
  row.className = "row";
  const title = document.createElement("b");
  title.textContent = "debug";
  row.append(
    title,
    button("Mark (F9)", "Note that it looks wrong right now, with the full state", mark),
    button("Redraw", "Redraw every terminal (if this fixes it, it's a paint problem)", redraw),
    button("Copy", "Copy the whole log", () => {
      writeText(lines.join("\n")).then(
        () => push("copiado"),
        () => push("no se pudo copiar"),
      );
    }),
    button("Clear", "Clear the log", () => {
      lines.length = 0;
      draw();
    }),
    button("_", "Minimize", () => {
      small = !small;
      box!.classList.toggle("small", small);
      draw();
      tick();
    }),
  );
  const swRow = document.createElement("div");
  swRow.className = "row";
  for (const k of Object.keys(sw) as Sw[]) {
    const b = button("", SW_HELP[k], () => {
      sw[k] = !sw[k];
      try {
        localStorage.setItem("ethereon.dbgsw", JSON.stringify(sw));
      } catch {}
      paint();
      dbg("interruptor", { ...sw });
    });
    const paint = () => {
      b.textContent = `${k}: ${sw[k] ? "on" : "OFF"}`;
      b.style.borderColor = sw[k] ? "" : "var(--accent)";
    };
    paint();
    swRow.append(b);
  }
  live = document.createElement("pre");
  live.className = "live";
  log = document.createElement("pre");
  box.append(row, swRow, live, log);
  // Que el panel no le robe el foco ni el arrastre a nada.
  box.addEventListener("pointerdown", (e) => e.stopPropagation());
  document.body.appendChild(box);
  setInterval(tick, 400);

  window.addEventListener(
    "keydown",
    (e) => {
      if (e.key !== "F9") return;
      e.preventDefault();
      e.stopPropagation();
      mark();
    },
    true,
  );
  window.addEventListener("webglcontextlost", (e) => dbg("webglcontextlost (evento)", { el: describe(e.target as Element) }, true), true);
  window.addEventListener("webglcontextrestored", () => dbg("webglcontextrestored"), true);
  // Un error a mitad de un dibujo deja el canvas borrado: que quede anotado con su pila.
  window.addEventListener("error", (e) => dbg("ERROR", { msg: e.message, at: `${e.filename}:${e.lineno}:${e.colno}`, stack: String(e.error?.stack ?? "").slice(0, 1500) }));
  window.addEventListener("unhandledrejection", (e) => dbg("ERROR (promesa)", { msg: String(e.reason?.message ?? e.reason), stack: String(e.reason?.stack ?? "").slice(0, 1500) }));
  const origError = console.error;
  console.error = (...a: unknown[]) => {
    origError(...a);
    dbg("console.error", a.map((x) => (x instanceof Error ? `${x.message} ${x.stack}` : String(x))).join(" ").slice(0, 1500));
  };
  window.addEventListener("focus", () => dbg("ventana: foco", undefined, true));
  // La terminal gana o pierde el foco (se sospecha que perderlo la deja en negro).
  document.addEventListener("focusin", (e) => (e.target as Element).closest?.(".xterm") && dbg("terminal: gana foco", undefined, true));
  document.addEventListener("focusout", (e) => {
    if (!(e.target as Element).closest?.(".xterm")) return;
    dbg("terminal: pierde foco", { hacia: (e.relatedTarget as Element | null)?.className ?? "nada" }, true);
    // Y justo al cuadro siguiente, que es cuando xterm redibuja.
    requestAnimationFrame(() => requestAnimationFrame(() => shot("  +2 cuadros")));
  });
  window.addEventListener("blur", () => dbg("ventana: sin foco"));
  document.addEventListener("visibilitychange", () => dbg("visibilidad", document.visibilityState, true));
  let rt: ReturnType<typeof setTimeout> | undefined;
  window.addEventListener("resize", () => {
    clearTimeout(rt);
    rt = setTimeout(() => dbg("ventana: tamaño", { w: innerWidth, h: innerHeight }, true), 200);
  });

  // Cambios de lo que se ve.
  $effect.root(() => {
    $effect(() => {
      const v = { view: app.view, active: app.activeTerm };
      dbg("se ve", v, true);
    });
  });
  push(`debug listo (${location.search || "principal"}) interruptores ${JSON.stringify(sw)}`);
}
