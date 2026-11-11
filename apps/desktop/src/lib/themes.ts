// Temas, paletas y opacidad. Los colores base estan en src/styles/themes.css y las paletas
// en scripts/palettes.py (que genera palettes.css y palettes.gen.ts).
//
// Opacidad: cada tema recuerda la suya (100 = como siempre). Por debajo, el fondo y las
// superficies se vuelven translucidas y la ventana deja ver el escritorio desenfocado (el
// vidrio lo pone Windows, ver window.rs). Los temas estan definidos en <html>; las
// versiones translucidas de sus colores se escriben como estilo en <body>, que es lo que
// heredan los componentes. Asi ningun tema ni paleta tiene que saber de la opacidad.
import { api } from "./api";
import { PALETTES } from "./palettes.gen";
import { SCHEMES } from "./termschemes";

export const THEMES = [
  { id: "default", name: "Default" },
  { id: "warp", name: "Warp" },
  { id: "poolsuite", name: "Poolsuite FM" },
  { id: "haze", name: "Haze" },
  { id: "athanor", name: "Athanor" },
  { id: "neovim", name: "Neovim" },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];

export function palettesOf(t: ThemeId) {
  return PALETTES[t] ?? [];
}

/** Limites del deslizador de opacidad (en %). */
export const OPACITY_MIN = 30;
export const OPACITY_MAX = 100;

/** Las superficies que se vuelven translucidas. --bg lo pinta body::before (base.css). */
const SURFACES = ["--bg", "--sidebar", "--panel", "--panel-2", "--term-bg"];

const KEY = "ethereon.theme";
const PKEY = (t: ThemeId) => `ethereon.palette.${t}`;
const OKEY = (t: ThemeId) => `ethereon.opacity.${t}`;
/** Los colores de la terminal (Settings → Terminal → Colors). Vacio = los del tema. Uno solo
 *  para todos los temas, como en Termius. */
const SKEY = "ethereon.termscheme";

function read(k: string): string | null {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
}
function write(k: string, v: string) {
  try {
    localStorage.setItem(k, v);
  } catch {}
}

export function currentTheme(): ThemeId {
  const t = read(KEY) as ThemeId | null;
  return t && THEMES.some((x) => x.id === t) ? t : "default";
}

/** La paleta elegida para un tema (cada tema recuerda la suya). */
export function currentPalette(t: ThemeId): string {
  const p = read(PKEY(t));
  const list = palettesOf(t);
  return p && list.some((x) => x.id === p) ? p : list[0]?.id ?? "";
}

/** El esquema de colores de la terminal elegido ("" = los del tema). */
export function currentScheme(): string {
  const s = read(SKEY);
  return s && SCHEMES.some((x) => x.id === s) ? s : "";
}

/** Las variables de la terminal que pisa un esquema. */
const TERM_VARS = ["--term-bg", "--term-fg", "--term-cursor", "--term-selection", "--term-selection-fg", "--term-ansi"];

/** Pone los colores del esquema en <html>, encima de los del tema y la paleta (un estilo en
 *  el elemento gana a sus reglas). Asi los ve todo lo que usa --term-*: xterm, el marco de
 *  los paneles en mosaico y la opacidad, que los vuelve translucidos como los del tema. */
function applyScheme(id: string) {
  const st = document.documentElement.style;
  for (const v of TERM_VARS) st.removeProperty(v);
  const s = SCHEMES.find((x) => x.id === id);
  if (!s) return;
  st.setProperty("--term-bg", s.bg);
  st.setProperty("--term-fg", s.fg);
  st.setProperty("--term-cursor", s.cursor);
  st.setProperty("--term-selection", s.selection);
  if (s.selectionFg) st.setProperty("--term-selection-fg", s.selectionFg);
  st.setProperty("--term-ansi", s.ansi.join(" "));
}

/** La opacidad elegida para un tema, en % (cada tema recuerda la suya). */
export function currentOpacity(t: ThemeId): number {
  const n = Number(read(OKEY(t)));
  return Number.isFinite(n) && n >= OPACITY_MIN && n <= OPACITY_MAX ? Math.round(n) : OPACITY_MAX;
}

export function applyTheme(t: ThemeId, palette = currentPalette(t), opacity = currentOpacity(t), scheme = currentScheme()) {
  const el = document.documentElement;
  el.dataset.theme = t;
  el.dataset.palette = palette;
  applyScheme(scheme);
  write(KEY, t);
  write(PKEY(t), palette);
  write(OKEY(t), String(opacity));
  write(SKEY, scheme);
  applyOpacity(opacity);
  window.dispatchEvent(new CustomEvent("themechange"));
}

/** Lee una variable del tema actual (para la terminal, que no usa CSS). Desde <body>, que
 *  es donde estan los colores ya translucidos. */
export function cssVar(name: string): string {
  return getComputedStyle(document.body).getPropertyValue(name).trim();
}

let glassOn: boolean | null = null;

function applyOpacity(pct: number) {
  const a = Math.min(OPACITY_MAX, Math.max(OPACITY_MIN, pct)) / 100;
  const body = document.body.style;
  body.setProperty("--opacity", String(a));
  if (a >= 1) {
    for (const v of SURFACES) body.removeProperty(v);
    body.removeProperty("--bg-solid");
  } else {
    // Los valores del tema, resueltos a rgba y con el alfa multiplicado. Resueltos porque
    // la terminal (xterm) no entiende color-mix().
    const theme = getComputedStyle(document.documentElement);
    body.setProperty("--bg-solid", theme.getPropertyValue("--bg"));
    for (const v of SURFACES) {
      const c = resolve(theme.getPropertyValue(v));
      if (c) body.setProperty(v, `rgba(${c[0]}, ${c[1]}, ${c[2]}, ${+(c[3] * a).toFixed(3)})`);
      else body.removeProperty(v);
    }
  }
  const on = a < 1;
  if (on !== glassOn) {
    glassOn = on;
    api.windowGlass(on).catch(() => {});
  }
}

/** Cualquier color CSS a [r, g, b, a], via el motor del navegador. null si no es un color. */
function resolve(value: string): [number, number, number, number] | null {
  const probe = document.createElement("span");
  probe.style.color = value.trim();
  if (!probe.style.color) return null;
  document.body.appendChild(probe);
  const m = getComputedStyle(probe).color.match(/rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:[,\s/]+([\d.]+))?/);
  probe.remove();
  return m ? [+m[1], +m[2], +m[3], m[4] === undefined ? 1 : +m[4]] : null;
}
