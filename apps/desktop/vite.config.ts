import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { readFileSync } from "node:fs";

// xterm.js dibuja a mano los bloques (█ ▀ ▄) y cuadrantes, pero no los sextantes ni los
// octantes ("Symbols for Legacy Computing"), que usan chafa y los MOTD con imagenes. Sin
// esto salen de otra fuente y se deforman. Se suman a su tabla de bloques, que dibuja
// rectangulos en una grilla de 8x8 dentro de la celda. La tabla sale de
// scripts/legacy_glyphs.py.
function legacyGlyphs(): Plugin {
  const glyphs = readFileSync(new URL("./src/lib/legacy-glyphs.gen.json", import.meta.url), "utf8");
  // El primer elemento de la tabla de bloques de xterm, escrito como "▀" o "▀".
  const first = /\{("(?:\\u2580|▀)":\[\{x:0,y:0,w:8,h:4\}\])/;
  return {
    name: "ethereon-legacy-glyphs",
    enforce: "pre",
    transform(code, id) {
      if (!/@xterm[\\/]addon-webgl[\\/]lib[\\/]addon-webgl\.m?js/.test(id)) return;
      if (!first.test(code)) {
        this.warn("xterm addon-webgl changed: sextants and octants will come from the font");
        return;
      }
      return { code: code.replace(first, (_m, rest) => `{...${glyphs},${rest}`), map: null };
    },
  };
}

// Windows Terminal (y conhost) ubican las imagenes Sixel como si cada celda midiera 10x20
// pixeles; xterm.js las pone a tamaño real. Las herramientas pensadas para Windows Terminal
// (el splash de athanor, por ejemplo) salen enormes. Antes de ubicar cada imagen se llama a
// globalThis.__ethSixel, que la reescala (src/lib/sixel.ts).
function sixelLikeWindowsTerminal(): Plugin {
  const call = /(this\._dec\.release\(\),)this\._storage\.addImage\((\w+)\)/;
  return {
    name: "ethereon-sixel-scale",
    enforce: "pre",
    transform(code, id) {
      if (!/@xterm[\\/]addon-image[\\/]lib[\\/]addon-image\.m?js/.test(id)) return;
      if (!call.test(code)) {
        this.warn("xterm addon-image changed: Sixel images will not be scaled like Windows Terminal");
        return;
      }
      const hook = (v: string) => `(globalThis.__ethSixel?globalThis.__ethSixel(${v},this._storage._renderer.cellSize):${v})`;
      return { code: code.replace(call, (_m, pre, v) => `${pre}this._storage.addImage(${hook(v)})`), map: null };
    },
  };
}

export default defineConfig({
  plugins: [legacyGlyphs(), sixelLikeWindowsTerminal(), svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  // Sin pre-empaquetar, para que el plugin de arriba tambien corra en `npm run dev`.
  optimizeDeps: { exclude: ["@xterm/addon-webgl", "@xterm/addon-image"] },
  build: { target: "es2022", chunkSizeWarningLimit: 800 },
});
