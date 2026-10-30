<script lang="ts">
  // Texto que, al cambiar, "decodifica" el nuevo: cada letra gira por letras al azar (como un
  // rodillo de tragamonedas) y se va fijando de izquierda a derecha. La primera vez se muestra
  // tal cual. Dura lo que diga Settings > Terminal (0 = sin animacion); con "reducir
  // movimiento" del sistema tampoco anima.
  import { untrack } from "svelte";
  import { prefs } from "../lib/prefs.svelte";

  let { text }: { text: string } = $props();

  const GLYPHS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
  const glyph = () => GLYPHS[Math.floor(Math.random() * GLYPHS.length)];
  const reduce = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

  /** Mientras gira: cada letra y si todavia esta girando (va mas apagada). */
  let chars = $state<{ c: string; spin: boolean }[] | null>(null);
  let prev = untrack(() => text);

  $effect(() => {
    const to = text;
    if (to === prev) return;
    // Si cambia en medio de una animacion, arranca desde lo que se esta viendo.
    const from = untrack(() => chars?.map((x) => x.c).join("") ?? prev);
    prev = to;
    const duration = untrack(() => prefs.term.scrambleMs);
    if (reduce() || duration <= 0) return void (chars = null);
    const n = Math.max(from.length, to.length);
    // Por letra, en fraccion de la duracion: cuando empieza a girar y cuando queda fija. De
    // izquierda a derecha, con algo de azar; la ultima termina justo al final.
    const plan = Array.from({ length: n }, (_, i) => {
      const start = (i / Math.max(1, n)) * 0.45 + Math.random() * 0.15;
      return { start, end: start + 0.25 + Math.random() * 0.15, c: glyph() };
    });
    const last = Math.max(...plan.map((p) => p.end));
    const t0 = performance.now();
    let raf = 0;
    const tick = (now: number) => {
      const f = ((now - t0) / duration) * last;
      chars = plan.map((p, i) => {
        if (f >= p.end) return { c: to[i] ?? "", spin: false };
        if (f < p.start) return { c: from[i] ?? "", spin: false };
        if (to[i] === " " || Math.random() < 0.35) p.c = to[i] === " " ? " " : glyph();
        return { c: p.c, spin: true };
      });
      if (f >= last) return void (chars = null);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });
</script>

{#if chars}{#each chars as x}<span class:spin={x.spin}>{x.c}</span>{/each}{:else}{text}{/if}

<style>
  .spin {
    color: var(--text-3);
  }
</style>
