// Temas y paletas. Los colores base estan en src/styles/themes.css y las paletas en
// scripts/palettes.py (que genera palettes.css y palettes.gen.ts).
import { PALETTES } from "./palettes.gen";

export const THEMES = [
  { id: "default", name: "Default" },
  { id: "warp", name: "Warp" },
  { id: "poolsuite", name: "Poolsuite FM" },
  { id: "haze", name: "Haze" },
  { id: "athanor", name: "Athanor" },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];

export function palettesOf(t: ThemeId) {
  return PALETTES[t] ?? [];
}

const KEY = "ethereon.theme";
const PKEY = (t: ThemeId) => `ethereon.palette.${t}`;

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

export function applyTheme(t: ThemeId, palette = currentPalette(t)) {
  const el = document.documentElement;
  el.dataset.theme = t;
  el.dataset.palette = palette;
  write(KEY, t);
  write(PKEY(t), palette);
  window.dispatchEvent(new CustomEvent("themechange"));
}

/** Lee una variable del tema actual (para la terminal, que no usa CSS). */
export function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}
