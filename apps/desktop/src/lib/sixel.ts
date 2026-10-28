// Escala de las imagenes Sixel (el gancho lo pone vite.config.ts). Como Windows Terminal:
// cada celda vale 10x20 pixeles de la imagen, sin suavizar, asi el tramado queda nitido.
import { prefs } from "./prefs.svelte";

type Cell = { width: number; height: number };

function scale(img: HTMLCanvasElement, cell: Cell): HTMLCanvasElement {
  if (!prefs.term.sixelLikeWt || !(cell.width > 0) || !(cell.height > 0)) return img;
  const w = Math.max(1, Math.round((img.width * cell.width) / 10));
  const h = Math.max(1, Math.round((img.height * cell.height) / 20));
  if (w === img.width && h === img.height) return img;
  const out = document.createElement("canvas");
  out.width = w;
  out.height = h;
  const g = out.getContext("2d");
  if (!g) return img;
  g.imageSmoothingEnabled = false;
  g.drawImage(img, 0, 0, w, h);
  return out;
}

(globalThis as unknown as { __ethSixel: typeof scale }).__ethSixel = scale;
