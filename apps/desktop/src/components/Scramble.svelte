<script lang="ts">
  // Texto que, al cambiar, "decodifica" el nuevo: cada letra gira por letras al azar (como un
  // rodillo de tragamonedas) y se va fijando de izquierda a derecha. La primera vez se muestra
  // tal cual. Con "reducir movimiento" del sistema cambia sin animar.
  import { untrack } from "svelte";

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
    if (reduce()) return void (chars = null);
    const n = Math.max(from.length, to.length);
    // Por letra: en que cuadro empieza a girar y en cuál queda fija.
    const plan = Array.from({ length: n }, (_, i) => {
      const start = Math.floor(i * 1.5 + Math.random() * 8);
      return { start, end: start + 10 + Math.floor(Math.random() * 14), c: glyph() };
    });
    const last = Math.max(...plan.map((p) => p.end));
    let frame = 0;
    let raf = 0;
    const tick = () => {
      chars = plan.map((p, i) => {
        if (frame >= p.end) return { c: to[i] ?? "", spin: false };
        if (frame < p.start) return { c: from[i] ?? "", spin: false };
        if (to[i] === " " || Math.random() < 0.35) p.c = to[i] === " " ? " " : glyph();
        return { c: p.c, spin: true };
      });
      if (++frame > last) return void (chars = null);
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
