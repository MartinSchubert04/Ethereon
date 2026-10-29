// La fuente de la terminal. La elige la terminal, no la shell: el tema de PowerShell no la
// puede cambiar. Por defecto es la del tema de Ethereon; se puede elegir otra, la misma de
// Windows Terminal, o cualquiera instalada en Windows.
import { invoke } from "@tauri-apps/api/core";
import { prefs } from "./prefs.svelte";
import { cssVar } from "./themes";

const NERD = '"Symbols Nerd Font Mono"';

/** Incluidas en Ethereon: no hace falta tenerlas instaladas. */
export const INCLUDED: { family: string; note: string }[] = [
  { family: "Geist Mono Variable", note: "Default, Warp, Poolsuite and Haze themes" },
  { family: "PxPlus IBM VGA 8x16", note: "Athanor theme" },
  { family: "JetBrains Mono", note: "Neovim theme" },
  { family: "Fira Code", note: "included" },
  { family: "IBM Plex Mono", note: "included" },
  { family: "Source Code Pro", note: "included" },
  { family: "Cascadia Mono", note: "included" },
];

const quote = (f: string) => {
  const t = f.trim().replace(/^["']|["']$/g, "");
  return /^(monospace|ui-monospace)$/.test(t) ? t : `"${t}"`;
};

/** La lista de fuentes para xterm: la elegida (o la del tema), los iconos y los respaldos. */
export function termFont(): string {
  const theme = cssVar("--mono");
  const chosen = prefs.term.fontFamily?.trim();
  if (!chosen) {
    // Los iconos van despues de la primera fuente del tema.
    const [first, ...rest] = theme.split(",");
    return [first.trim(), NERD, ...rest.map((r) => r.trim())].join(", ");
  }
  return [...chosen.split(",").map(quote), NERD, theme].join(", ");
}

/** Fuentes de pixeles (PxPlus, Px437...): no tienen negrita y el navegador la inventaria
 *  engrosando con suavizado, borrosa. Ahi va "normal" y el atlas de WebGL la engrosa un
 *  pixel, nitida, como Windows Terminal (plugin pixelBold en vite.config.ts). */
export function boldWeight(): "bold" | "normal" {
  const first = termFont().split(",")[0];
  return /\b(PxPlus|Px437|Mx437|PxIBM|Ac437)\b/i.test(first) ? "normal" : "bold";
}

export interface WtFont {
  face: string;
  size: number;
}

/** pt de Windows Terminal a px (12 pt = 16 px). */
export const ptToPx = (pt: number) => Math.round((pt * 96) / 72);

let installed: Promise<string[]> | null = null;
export const systemFonts = () => (installed ??= invoke<string[]>("system_fonts").catch(() => []));
export const wtFont = () => invoke<WtFont | null>("wt_font").catch(() => null);
