// Arreglo para un bug del renderer WebGL de xterm (@xterm/addon-webgl 0.19) con fondo
// transparente (los temas con vidrio, ver theme() en TermPane).
//
// El renderer pinta un rectangulo de fondo en cada celda cuyos atributos de fondo no son 0. El
// fondo de una celda tambien guarda marcas que no son color (atenuado, cursiva...): un texto
// atenuado con el fondo por defecto (`chalk.dim`, el recap y el "Resume this session" de
// Claude Code) recibia un rectangulo con el color de fondo del tema y la opacidad forzada a 1.
// Con el fondo transparente (rgba 0,0,0,0) eso es negro: el texto salia con fondo negro.
//
// Una celda con el fondo por defecto no necesita rectangulo (el fondo ya esta pintado, o es
// transparente a proposito): se lo deja de tamaño 0. Se cambia el prototipo una vez, asi
// vale tambien para los renderers que se crean de nuevo al perder el contexto WebGL.

import type { WebglAddon } from "@xterm/addon-webgl";

/** Bits del fondo de una celda (ver BufferLine de xterm). */
const COLOR_MODE = 0x3000000;
/** En el primer plano: video inverso (el fondo es el color de las letras). */
const INVERSE = 0x4000000;

let patched = false;

type Rects = {
  _updateRectangle: (t: { attributes: Float32Array }, n: number, fg: number, bg: number, x1: number, x2: number, y: number) => void;
  _addRectangle: (a: Float32Array, n: number, ...rest: number[]) => void;
};

export function fixDefaultBackgrounds(addon: WebglAddon) {
  if (patched) return;
  // Campos privados: si una version nueva los cambia, no se toca nada.
  const rr = (addon as unknown as { _renderer?: { _rectangleRenderer?: { value?: Rects } } })._renderer?._rectangleRenderer?.value;
  const proto = rr && (Object.getPrototypeOf(rr) as Rects);
  if (!proto || typeof proto._updateRectangle !== "function" || typeof proto._addRectangle !== "function") return;
  patched = true;
  const original = proto._updateRectangle;
  proto._updateRectangle = function (this: Rects, t, n, fg, bg, x1, x2, y) {
    // Primero el original: agranda el arreglo si hace falta.
    original.call(this, t, n, fg, bg, x1, x2, y);
    if (!(fg & INVERSE) && (bg & COLOR_MODE) === 0) this._addRectangle(t.attributes, n, 0, 0, 0, 0, 0, 0, 0, 0);
  };
}
