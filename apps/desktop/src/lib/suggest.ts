// Sugerencias en gris mientras se escribe un comando, como Warp o fish: lo que falta del
// ultimo comando parecido del historial o, para una ruta, de un archivo o carpeta. Se acepta
// con un atajo (Settings > Keyboard shortcuts, o el clic en la etiqueta al lado: →, Tab, Shift →).
//
// Solo cuando se sabe que la shell espera un comando: las shells locales avisan con OSC 9;9
// en cada prompt (ver report_cwd en terminal.rs), y las de Linux con OSC 7. Desde el Enter
// hasta el proximo prompt (un programa corriendo: Claude Code, vim, ssh) no hay sugerencias.
// Lo escrito se lee de la pantalla, desde donde termino el prompt hasta el cursor: asi vale
// tambien lo que la shell cambia sola (Tab, las flechas del historial, una correccion), y se
// vuelve a buscar con cada cambio. La sugerencia se dibuja encima del xterm, en la posicion
// del cursor, solo con el cursor al final y nada a la derecha (si la shell ya muestra la
// suya, como PSReadLine, no se pisa).

import type { IDisposable, Terminal } from "@xterm/xterm";
import { api, type FileEntry } from "./api";
import { comboOf, prefs, pretty, setBind } from "./prefs.svelte";

// ---- historial: lo que se escribio en Ethereon (por shell o host), y el de la propia shell

const KEY = "ethereon.history";
const MAX = 500;
let mine: Record<string, string[]> | null = null;

function ownHistory(): Record<string, string[]> {
  if (!mine) {
    try {
      mine = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    } catch {
      mine = {};
    }
  }
  return mine!;
}

function remember(profile: string, cmd: string) {
  const h = ownHistory();
  const list = (h[profile] ?? []).filter((c) => c !== cmd);
  list.push(cmd);
  h[profile] = list.slice(-MAX);
  try {
    localStorage.setItem(KEY, JSON.stringify(h));
  } catch {}
}

const fromShell = new Map<string, Promise<string[]>>();
function shellHistory(shell: string | undefined): Promise<string[]> {
  if (!shell) return Promise.resolve([]);
  let p = fromShell.get(shell);
  if (!p) {
    p = api.shellHistory(shell).catch(() => []);
    fromShell.set(shell, p);
  }
  return p;
}

// ---- carpetas: se leen una vez cada tanto

const listed = new Map<string, { at: number; list: Promise<FileEntry[]> }>();
function list(dir: string): Promise<FileEntry[]> {
  const c = listed.get(dir);
  if (c && Date.now() - c.at < 3000) return c.list;
  const l = api.localList(dir).catch(() => [] as FileEntry[]);
  listed.set(dir, { at: Date.now(), list: l });
  return l;
}

/** Una ruta de Git Bash (`/c/Users`) como la de Windows (`C:\Users`). */
function winPath(p: string): string {
  const m = /^\/([a-zA-Z])(\/.*)?$/.exec(p);
  return m ? `${m[1].toUpperCase()}:${(m[2] ?? "\\").replace(/\//g, "\\")}` : p;
}

function join(dir: string, rest: string): string {
  if (!rest) return dir;
  if (/^[a-zA-Z]:[\\/]/.test(rest) || rest.startsWith("\\\\")) return rest;
  if (rest.startsWith("/")) return winPath(rest);
  return `${dir.replace(/[\\/]+$/, "")}\\${rest.replace(/\//g, "\\")}`;
}

const DIR_COMMANDS = new Set(["cd", "pushd", "sl", "set-location", "chdir", "z"]);

export interface SuggestCtx {
  /** De quien es el historial: la shell local ("powershell") o el host. */
  profile(): string;
  /** La shell local, si es una (para su historial y el separador de rutas). */
  shell(): string | undefined;
  /** La carpeta de la shell local, si se sabe (para completar rutas). */
  cwd(): string | undefined;
  /** Escribe en la sesion (lo que falta, al aceptar). */
  send(data: string): void;
  /** "Custom..." en el menu de la etiqueta: llevar a la persona a los atajos. */
  custom(): void;
}

/** Las teclas que ofrece la etiqueta (como Warp). Tab es la de entrada. */
const CHOICES: [string, string][] = [
  ["Tab", "Tab"],
  ["ArrowRight", "→"],
  ["Shift+ArrowRight", "Shift →"],
];

export class Suggester {
  /** Lo escrito en el prompt, leido de la pantalla (ver `current`). */
  private line = "";
  /** Donde termina el prompt (fila absoluta y columna): ahi empieza lo que se escribe. Se
   *  toma con la primera tecla despues del prompt, cuando el cursor todavia esta ahi. */
  private start: { y: number; x: number } | null = null;
  /** Para que linea es `ghost`. */
  private ghostFor = "";
  private ghost = "";
  private seq = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private box: HTMLDivElement;
  private text: HTMLSpanElement;
  private chip: HTMLButtonElement;
  private menu: HTMLDivElement | null = null;
  private subs: IDisposable[] = [];

  constructor(
    private term: Terminal,
    private ctx: SuggestCtx,
  ) {
    this.box = document.createElement("div");
    this.box.className = "eth-suggest";
    this.text = document.createElement("span");
    this.chip = document.createElement("button");
    this.chip.className = "eth-suggest-key";
    this.chip.title = "Key that accepts the suggestion";
    this.chip.addEventListener("mousedown", (e) => e.preventDefault());
    this.chip.addEventListener("click", (e) => {
      e.stopPropagation();
      this.toggleMenu();
    });
    this.box.append(this.text, this.chip);
    // Devuelve false: la carpeta tambien la escucha lib/cwd.ts.
    this.subs.push(
      term.parser.registerOscHandler(9, (d) => (d.startsWith("9;") && this.prompt(), false)),
      term.parser.registerOscHandler(7, () => (this.prompt(), false)),
      // Cada vez que se dibuja (una tecla, Tab, el historial, una correccion) se relee la
      // linea; si cambio, se vuelve a buscar la sugerencia.
      term.onRender(() => this.changed()),
      term.onResize(() => this.render()),
    );
  }

  dispose() {
    clearTimeout(this.timer);
    this.subs.forEach((s) => s.dispose());
    this.closeMenu();
    this.box.remove();
  }

  /** La shell espera un comando (entre su prompt y el Enter). Si no, corre un programa. */
  atPrompt = false;

  /** La shell mostro el prompt: empieza un comando nuevo. */
  private prompt() {
    this.atPrompt = true;
    this.start = null;
    this.line = "";
    this.set("", "");
  }

  /** Lo que la persona escribe (onData de xterm), antes de mandarlo a la sesion. */
  input(d: string) {
    if (!this.atPrompt) return;
    const b = this.term.buffer.active;
    // La primera tecla: el cursor todavia esta donde termino el prompt.
    if (!this.start) this.start = { y: b.baseY + b.cursorY, x: b.cursorX };
    if (d.includes("\r")) {
      const cmd = (this.current() ?? "").trim();
      if (cmd) remember(this.ctx.profile(), cmd);
      this.atPrompt = false;
      this.start = null;
      this.line = "";
      this.set("", "");
    }
  }

  /** Lo escrito desde el final del prompt hasta el cursor (aunque ocupe varias filas), o null
   *  si no se sabe (no hay prompt, se fue a la pantalla alternativa, el cursor quedo antes). */
  private current(): string | null {
    const s = this.start;
    const b = this.term.buffer.active;
    if (!this.atPrompt || !s || b.type !== "normal") return null;
    const cy = b.baseY + b.cursorY;
    if (cy < s.y || (cy === s.y && b.cursorX < s.x)) return null;
    let out = "";
    for (let y = s.y; y <= cy; y++) {
      const row = b.getLine(y);
      if (!row) return null;
      out += row.translateToString(false, y === s.y ? s.x : 0, y === cy ? b.cursorX : this.term.cols);
    }
    return out;
  }

  /** Se dibujo algo: si la linea cambio, la sugerencia vieja ya no va y se busca otra (con
   *  un respiro, para no buscar en cada letra de una rafaga). */
  private changed() {
    const now = this.current() ?? "";
    if (now !== this.line) {
      this.line = now;
      if (this.ghostFor !== now) this.ghost = "";
      clearTimeout(this.timer);
      this.timer = setTimeout(() => this.compute(), 40);
    }
    this.render();
  }

  /** Si `e` es la tecla de aceptar y hay sugerencia, la escribe. True = se uso la tecla. */
  accept(e: KeyboardEvent): boolean {
    if (e.type !== "keydown" || !this.ghost || !this.shown()) return false;
    if (comboOf(e) !== prefs.binds.acceptSuggestion) return false;
    e.preventDefault();
    const g = this.ghost;
    this.set("", "");
    this.ctx.send(g);
    // Cuando la shell la dibuje, `changed` busca la que sigue (la carpeta de adentro).
    return true;
  }

  private async compute() {
    const n = ++this.seq;
    const line = this.line;
    if (!this.atPrompt || !prefs.term.suggest || !line.trim()) return this.set("", line);
    const hist = [...(ownHistory()[this.ctx.profile()] ?? []), ...(await shellHistory(this.ctx.shell()))];
    // `cd algo` del historial solo sirve si `algo` existe aca: el mismo comando escrito en
    // otra carpeta no lleva a ningun lado. En una shell local se revisa; si no, se confia.
    const cmd = line.trimStart().split(/\s+/)[0].toLowerCase();
    const dirCmd = DIR_COMMANDS.has(cmd) && /\s/.test(line.trimStart()) && !!this.ctx.cwd();
    let g = "";
    const seen = new Set<string>();
    for (let i = hist.length - 1; i >= 0 && !g; i--) {
      const h = hist[i];
      if (h.length <= line.length || !h.startsWith(line) || seen.has(h)) continue;
      seen.add(h);
      if (dirCmd && !(await this.isDirHere(h.trimStart().slice(cmd.length).trim()))) continue;
      g = h.slice(line.length);
    }
    if (!g) g = await this.path(line);
    if (n === this.seq && line === this.line) this.set(g, line);
  }

  /** Si `arg` (lo que va despues de `cd`) es una carpeta que existe desde donde esta la shell. */
  private async isDirHere(arg: string): Promise<boolean> {
    const a = arg.replace(/^["']|["']$/g, "").replace(/[\\/]+$/, "");
    if (!a || /^(\.\.?|~|-)$/.test(a) || /\s-/.test(` ${a}`)) return true;
    const cwd = this.ctx.cwd();
    if (!cwd) return true;
    const cut = Math.max(a.lastIndexOf("/"), a.lastIndexOf("\\"));
    const parent = cut < 0 ? "" : a.slice(0, cut + 1);
    const name = a.slice(cut + 1).toLowerCase();
    if (!name || name === "..") return true;
    return (await list(join(winPath(cwd), parent))).some((e) => e.dir && e.name.toLowerCase() === name);
  }

  /** Lo que falta de una ruta en el ultimo argumento (solo en shells locales). */
  private async path(line: string): Promise<string> {
    const cwd = this.ctx.cwd();
    const shell = this.ctx.shell();
    if (!cwd || !shell || shell === "wsl" || !/\s/.test(line.trimStart())) return "";
    const m = /(?:^|\s)(["']?)([^\s"']*)$/.exec(line);
    if (!m) return "";
    const [, quote, token] = m;
    const cmd = line.trimStart().split(/\s+/)[0].toLowerCase();
    const dirsOnly = DIR_COMMANDS.has(cmd);
    const cut = Math.max(token.lastIndexOf("/"), token.lastIndexOf("\\"));
    const dirPart = token.slice(0, cut + 1);
    const base = token.slice(cut + 1);
    const dir = join(winPath(cwd), dirPart);
    const lower = base.toLowerCase();
    const cands = (await list(dir)).filter(
      (e) =>
        (!dirsOnly || e.dir) &&
        e.name.length > base.length &&
        e.name.toLowerCase().startsWith(lower) &&
        (quote || !/\s/.test(e.name)) &&
        (base.startsWith(".") || !e.name.startsWith(".")),
    );
    if (!cands.length) return "";
    // Primero las que coinciden con mayusculas y todo; despues, la tocada mas recientemente
    // (lo mas probable: la carpeta en la que se estuvo trabajando).
    cands.sort((a, b) => Number(b.name.startsWith(base)) - Number(a.name.startsWith(base)) || (b.mtime ?? 0) - (a.mtime ?? 0));
    const e = cands[0];
    const sep = shell === "gitbash" ? "/" : "\\";
    return e.name.slice(base.length) + (e.dir ? sep : quote);
  }

  private set(g: string, forLine: string) {
    this.ghost = g;
    this.ghostFor = forLine;
    this.render();
  }

  /** La sugerencia es de lo que hay escrito ahora, el cursor esta al final y no hay nada a
   *  la derecha (si la shell muestra su propia sugerencia, como PSReadLine, no se pisa). */
  private shown(): boolean {
    const b = this.term.buffer.active;
    if (!this.atPrompt || b.type !== "normal" || b.viewportY !== b.baseY) return false;
    if (this.ghostFor !== this.line || this.current() !== this.line) return false;
    const row = b.getLine(b.baseY + b.cursorY);
    return !!row && row.translateToString(true, b.cursorX).trim() === "";
  }

  private render() {
    const screen = this.term.element?.querySelector(".xterm-screen") as HTMLElement | null;
    if (!this.ghost || !screen || !this.shown()) {
      this.box.remove();
      this.closeMenu();
      return;
    }
    // El tamaño de la celda (privado en xterm; si no esta, se calcula).
    const core = (this.term as unknown as { _core?: { _renderService?: { dimensions?: { css?: { cell?: { width: number; height: number } } } } } })._core;
    const cell = core?._renderService?.dimensions?.css?.cell ?? { width: screen.clientWidth / this.term.cols, height: screen.clientHeight / this.term.rows };
    const b = this.term.buffer.active;
    const room = Math.max(0, this.term.cols - b.cursorX);
    const o = this.term.options;
    Object.assign(this.box.style, {
      left: `${b.cursorX * cell.width}px`,
      top: `${b.cursorY * cell.height}px`,
      height: `${cell.height}px`,
      lineHeight: `${cell.height}px`,
      fontFamily: o.fontFamily ?? "monospace",
      fontSize: `${o.fontSize}px`,
      color: o.theme?.foreground ?? "inherit",
    });
    this.text.textContent = this.ghost.length > room ? this.ghost.slice(0, Math.max(0, room - 1)) + "…" : this.ghost;
    this.chip.textContent = pretty(prefs.binds.acceptSuggestion);
    this.chip.style.display = prefs.binds.acceptSuggestion && b.cursorX + this.ghost.length + 6 < this.term.cols ? "" : "none";
    if (this.box.parentElement !== screen) screen.appendChild(this.box);
  }

  private toggleMenu() {
    if (this.menu) return this.closeMenu();
    const m = document.createElement("div");
    m.className = "eth-suggest-menu";
    const title = document.createElement("div");
    title.className = "eth-suggest-title";
    title.textContent = "Accept with";
    m.appendChild(title);
    const cur = prefs.binds.acceptSuggestion;
    const custom = !CHOICES.some(([c]) => c === cur);
    for (const [combo, label] of [...CHOICES, ["", custom && cur ? `Custom: ${pretty(cur)}` : "Custom..."] as [string, string]]) {
      const btn = document.createElement("button");
      const on = combo ? combo === cur : custom;
      if (on) btn.className = "on";
      if (!combo) btn.classList.add("custom");
      const kbd = document.createElement(combo ? "kbd" : "span");
      kbd.textContent = label;
      const check = document.createElement("span");
      check.className = "check";
      check.textContent = on ? "✓" : "";
      btn.append(kbd, check);
      btn.addEventListener("mousedown", (e) => e.preventDefault());
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        this.closeMenu();
        if (combo) setBind("acceptSuggestion", combo);
        else this.ctx.custom();
        this.render();
        this.term.focus();
      });
      m.appendChild(btn);
    }
    // Apagarlas sin ir a Settings (se vuelven a prender en Settings > Terminal).
    const off = document.createElement("button");
    off.className = "off";
    off.textContent = "Turn off suggestions";
    off.addEventListener("mousedown", (e) => e.preventDefault());
    off.addEventListener("click", (e) => {
      e.stopPropagation();
      this.closeMenu();
      prefs.term.suggest = false;
      this.set("", this.line);
      this.term.focus();
    });
    m.appendChild(off);
    this.box.appendChild(m);
    this.menu = m;
    const away = (e: MouseEvent) => {
      if (!m.contains(e.target as Node)) this.closeMenu();
    };
    setTimeout(() => window.addEventListener("mousedown", away, { once: true }));
  }

  private closeMenu() {
    this.menu?.remove();
    this.menu = null;
  }
}
