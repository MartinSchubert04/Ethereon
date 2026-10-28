// Animaciones de la interfaz. Todas pasan por aca para respetar "reducir animaciones"
// de Windows: con eso prendido duran 0.
import { cubicOut, backOut } from "svelte/easing";
import { fade, fly, scale } from "svelte/transition";

const reduce = () => typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
const ms = (n: number) => (reduce() ? 0 : n);

/** Entrada de una vista o una tarjeta: sube unos px y aparece. */
export const rise = (node: Element, o: { delay?: number; y?: number } = {}) =>
  fly(node, { y: o.y ?? 8, duration: ms(220), delay: ms(o.delay ?? 0), easing: cubicOut });

/** Cajon lateral: entra desde la derecha con un poco de resorte. */
export const slideIn = (node: Element) => fly(node, { x: 48, duration: ms(260), easing: backOut, opacity: 0 });
export const slideOut = (node: Element) => fly(node, { x: 32, duration: ms(160), easing: cubicOut });

/** Dialogos y desplegables: crecen desde un poco mas chico. */
export const pop = (node: Element, o: { start?: number } = {}) =>
  scale(node, { start: o.start ?? 0.95, duration: ms(160), easing: cubicOut });

export const fadeQuick = (node: Element) => fade(node, { duration: ms(140) });

/** Avisos: entran desde abajo. */
export const toastIn = (node: Element) => fly(node, { y: 16, duration: ms(240), easing: backOut });

export const flipMs = () => ms(200);
