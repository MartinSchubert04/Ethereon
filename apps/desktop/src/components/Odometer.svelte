<script lang="ts">
  // Texto cuyos digitos giran como el rodillo de una tragamonedas (u odometro) cuando
  // cambian: cada digito es una tira 0-9 que baja hasta el nuevo valor, siempre hacia
  // adelante (de 5 a 0 pasa por 6, 7, 8, 9) y con un pequeno rebote al final. Lo que no es
  // digito se muestra tal cual. Los lugares se cuentan desde la derecha, asi las unidades
  // siguen siendo las unidades cuando aparece un digito nuevo a la izquierda. Con "reducir
  // movimiento" del sistema cambia sin animar.
  let { text }: { text: string } = $props();

  const chars = $derived([...text]);
  const isDigit = (c: string) => c >= "0" && c <= "9";
  const reduce = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
  // Tres vueltas: la segunda es para cuando da la vuelta (de 9 a 0) y la tercera cubre el rebote.
  const STRIP = Array.from({ length: 30 }, (_, i) => i % 10);
  const at = (n: number) => `translateY(${-n}lh)`;

  function roll(node: HTMLElement, d: number) {
    let cur = d;
    node.style.transform = at(d);
    return {
      update(to: number) {
        if (to === cur) return;
        // Al terminar queda en `to`, que se ve igual que `to + 10` (la tira se repite).
        const end = to > cur ? to : to + 10;
        node.style.transform = at(to);
        if (!reduce())
          node.animate([{ transform: at(cur) }, { transform: at(end) }], {
            duration: 700,
            easing: "cubic-bezier(.25, 1.3, .5, 1)",
          });
        cur = to;
      },
    };
  }
</script>

<span class="odo"
  ><span class="sr">{text}</span
  ><span aria-hidden="true"
    >{#each chars as c, i (chars.length - i)}{#if isDigit(c)}<span class="cell"
          ><span class="strip" use:roll={Number(c)}>{#each STRIP as n}<span>{n}</span>{/each}</span></span
        >{:else}{c}{/if}{/each}</span
  ></span
>

<style>
  .odo {
    font-variant-numeric: tabular-nums;
    white-space: pre;
  }
  .cell {
    display: inline-block;
    height: 1lh;
    overflow: hidden;
    vertical-align: top;
  }
  .strip {
    display: flex;
    flex-direction: column;
  }
  .strip span {
    height: 1lh;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
