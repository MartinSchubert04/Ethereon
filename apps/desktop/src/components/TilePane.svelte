<script lang="ts">
  // Una pestaña de terminal: una sola, o varias en mosaico (ver lib/tiles.svelte.ts). Los
  // paneles van con posicion absoluta segun el arbol: cambiar la distribucion no mueve nada
  // en el DOM, y ninguna terminal se vuelve a montar.
  import { X } from "@lucide/svelte";
  import TermPane from "./TermPane.svelte";
  import Scramble from "./Scramble.svelte";
  import ConnState from "./ConnState.svelte";
  import ApprovalsPane from "./ApprovalsPane.svelte";
  import { fadeQuick, pop } from "../lib/motion";
  import { addrOf } from "../lib/api";
  import { tabStyle } from "../lib/hostcolors.svelte";
  import { app, type TermTab } from "../lib/state.svelte";
  import { closeLeaf, dragToTabs, drop, geometry, moveLeaf, moveLeafTo, popOut, type Split, type Rect } from "../lib/tiles.svelte";

  let { tab, active }: { tab: TermTab; active: boolean } = $props();

  const list = $derived(tab.layout ? (tab.members ?? []) : [tab]);
  const tiled = $derived(list.length > 1);
  const geo = $derived(geometry(tab.layout ?? { leaf: tab.key }));

  let box: HTMLDivElement;
  /** Arrastrando una separacion: sin animar las posiciones (iria detras del mouse). */
  let resizing = $state(false);

  /** Reintenta esa terminal (TermPane escucha el mismo evento que Ctrl R). */
  const retry = (key: number) => window.dispatchEvent(new CustomEvent("ethereon:refresh", { detail: key }));
  const pct = (r: Rect | undefined) =>
    r ? `left:${r.x * 100}%;top:${r.y * 100}%;width:${r.w * 100}%;height:${r.h * 100}%` : "display:none";

  /** Debajo del nombre: el host (usuario@direccion) o "this PC" para una shell local. */
  function where(m: TermTab) {
    if (m.approvals) return app.approvals.length ? `${app.approvals.length} waiting` : "nothing waiting";
    if (m.local) return m.cwd ? `this PC · ${m.cwd}` : "this PC";
    const h = app.snap?.hosts.find((x) => x.name === m.host);
    return h ? addrOf(h, app.snap?.credentials) : m.host;
  }

  // ---- separaciones: arrastrar cambia la proporcion de esa division

  function gutterDown(e: PointerEvent, node: Split, r: Rect) {
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    resizing = true;
    const b = box.getBoundingClientRect();
    const move = (ev: PointerEvent) => {
      const v =
        node.dir === "row"
          ? (ev.clientX - b.left - r.x * b.width) / (r.w * b.width)
          : (ev.clientY - b.top - r.y * b.height) / (r.h * b.height);
      node.ratio = Math.min(0.88, Math.max(0.12, v));
    };
    const up = () => {
      resizing = false;
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
  }

  // ---- arrastrar un panel por su nombre: a otro panel de esta pestaña o de otra (lo
  // reacomoda o lo muda), o a la barra de pestañas. En la barra: sobre otra pestaña la suma a
  // esa (y si se queda un momento, la muestra para soltarlo en el panel que se quiera); en
  // otro lado vuelve a ser una pestaña sola. Soltado en ningun lado, vuelve lo que se veia.

  function pillDown(e: PointerEvent, m: TermTab) {
    dragToTabs(e, {
      label: m.title,
      self: tab.key,
      skip: (h) => h.tab === tab.key && h.leaf === m.key,
      barText: `Release to make "${m.title}" a tab`,
      onDrop: (to) => {
        if (to === "bar") popOut(tab, m.key);
        else if (to.tab !== tab.key) moveLeafTo(tab, m.key, to.tab, to.leaf, to.side);
        else if (to.leaf != null) {
          moveLeaf(tab, m.key, to.leaf, to.side);
          app.activeTerm = tab.key;
        }
      },
    });
  }
</script>

<div class="tiles" class:tiled class:resizing bind:this={box}>
  {#each list as m (m.slot ?? m.key)}
    <div class="slot" style={pct(geo.panes.get(m.key))} in:pop={{ start: 0.96 }}>
      <div
        class="frame"
        style={tabStyle(m)}
        class:focus={tiled && tab.focus === m.key}
        data-tile-tab={tab.key}
        data-tile-leaf={m.key}
        onpointerdowncapture={() => tiled && (tab.focus = m.key)}
      >
        {#if tiled}
          <div class="legend">
            <button class="pill" title="Drag to move it, here or into another tab (hold it over the tab to open it). Drop it on an empty part of the tab bar to make it a tab." onpointerdown={(e) => pillDown(e, m)}>
              <span class="dot" class:dead={!m.alive && !m.connecting}></span>
              <span class="ellipsis"><Scramble text={m.title} /></span>
            </button>
            <button class="x" title="Close this terminal" aria-label="Close {m.title}" onclick={() => closeLeaf(tab, m.key)}><X size={11} /></button>
          </div>
          <div class="where ellipsis">{where(m)}</div>
        {/if}
        {#if m.approvals}
          <ApprovalsPane bare={tiled} />
        {:else}
          <TermPane tab={m} active={active && (!tiled || tab.focus === m.key)} shown={active} bare={tiled} />
          <ConnState {m} onretry={() => retry(m.key)} />
        {/if}
        {#if drop.hint && drop.hint.tab === tab.key && drop.hint.leaf === m.key}
          <div class="drop {drop.hint.side}" transition:fadeQuick></div>
        {/if}
        {#if drop.swap && drop.swap.tab === tab.key && drop.swap.leaf === m.key}
          <!-- Con el teclado: se intercambia con esta, o (si es la que se mueve) se queda. -->
          <div class="drop full" class:here={m.key === tab.focus} transition:fadeQuick></div>
        {/if}
      </div>
    </div>
  {/each}

  {#if tiled}
    {#each geo.splits as s, i (i)}
      {@const r = s.r}
      {#if s.node.dir === "row"}
        <div
          class="gutter v"
          role="separator"
          aria-orientation="vertical"
          style="left:{(r.x + r.w * s.node.ratio) * 100}%;top:{r.y * 100}%;height:{r.h * 100}%"
          onpointerdown={(e) => gutterDown(e, s.node, r)}
        ></div>
      {:else}
        <div
          class="gutter h"
          role="separator"
          aria-orientation="horizontal"
          style="top:{(r.y + r.h * s.node.ratio) * 100}%;left:{r.x * 100}%;width:{r.w * 100}%"
          onpointerdown={(e) => gutterDown(e, s.node, r)}
        ></div>
      {/if}
    {/each}
  {/if}
  {#if drop.edge && drop.edge.tab === tab.key}
    <!-- Con el teclado: ocupa ese lado entero del mosaico. -->
    <div class="drop edge {drop.edge.side}" style="--share: {100 / list.length}%" transition:fadeQuick></div>
  {/if}
</div>


<style>
  .tiles {
    position: relative;
    height: 100%;
  }
  /* En mosaico: 12 px contra el borde y 10 px entre paneles, como los huecos de Hyprland. */
  .tiles.tiled {
    margin: 7px;
    height: calc(100% - 14px);
  }
  .slot {
    position: absolute;
  }
  /* Al cambiar la distribucion los paneles se deslizan a su lugar. */
  .tiled .slot {
    transition:
      left 0.2s cubic-bezier(0.2, 0.8, 0.2, 1),
      top 0.2s cubic-bezier(0.2, 0.8, 0.2, 1),
      width 0.2s cubic-bezier(0.2, 0.8, 0.2, 1),
      height 0.2s cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .tiled.resizing .slot {
    transition: none;
  }
  .frame {
    position: relative;
    height: 100%;
  }
  .tiled .frame {
    margin: 5px;
    height: calc(100% - 10px);
    padding: 26px 4px 4px 10px;
    border: 1px solid var(--line-strong);
    border-radius: max(var(--radius), 10px);
    background: var(--term-bg);
    transition:
      border-color 0.18s,
      box-shadow 0.25s;
  }
  .tiled .frame.focus {
    border-color: color-mix(in srgb, var(--accent) 70%, var(--line-strong));
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 9%, transparent);
  }
  /* El nombre va sobre la linea del borde, en una pastilla; el host abajo, tenue. */
  .legend {
    position: absolute;
    top: -11px;
    left: 12px;
    right: 12px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 4px;
    pointer-events: none;
  }
  .pill {
    pointer-events: auto;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    height: 21px;
    padding: 0 10px 0 8px;
    border: 1px solid var(--line-strong);
    border-radius: 999px;
    background: var(--term-bg);
    color: var(--text-2);
    font: 600 11.5px var(--mono);
    cursor: grab;
    transition:
      border-color 0.18s,
      color 0.18s,
      transform 0.12s;
  }
  .pill:hover {
    transform: translateY(-1px);
  }
  .focus .pill {
    border-color: color-mix(in srgb, var(--accent) 70%, var(--line-strong));
    color: var(--text);
  }
  .pill:active {
    cursor: grabbing;
  }
  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--ok);
  }
  .dot.dead {
    background: var(--danger);
  }
  .x {
    pointer-events: auto;
    display: grid;
    place-items: center;
    width: 19px;
    height: 19px;
    margin-left: auto;
    border-radius: 50%;
    background: var(--term-bg);
    color: var(--text-3);
    opacity: 0;
    transition: opacity 0.12s;
  }
  .frame:hover .x {
    opacity: 1;
  }
  .x:hover {
    color: var(--text);
  }
  .where {
    position: absolute;
    top: 10px;
    left: 22px;
    right: 14px;
    font: 10.5px var(--mono);
    color: var(--text-3);
    opacity: 0.75;
    pointer-events: none;
  }
  .gutter {
    position: absolute;
    z-index: 2;
  }
  .gutter.v {
    width: 10px;
    margin-left: -5px;
    cursor: col-resize;
  }
  .gutter.h {
    height: 10px;
    margin-top: -5px;
    cursor: row-resize;
  }
  /* Una rayita en el medio de la separacion, que aparece al pasar el mouse. */
  .gutter::after {
    content: "";
    position: absolute;
    border-radius: 2px;
    background: var(--accent);
    opacity: 0;
    transition:
      opacity 0.15s,
      transform 0.15s;
  }
  .gutter.v::after {
    left: 4px;
    width: 2px;
    top: calc(50% - 22px);
    height: 44px;
    transform: scaleY(0.6);
  }
  .gutter.h::after {
    top: 4px;
    height: 2px;
    left: calc(50% - 22px);
    width: 44px;
    transform: scaleX(0.6);
  }
  .gutter:hover::after,
  .resizing .gutter:active::after {
    opacity: 0.7;
    transform: none;
  }
  /* Donde caeria: la mitad del panel de ese lado. */
  .drop {
    position: absolute;
    z-index: 3;
    border-radius: max(var(--radius-sm), 6px);
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    outline: 2px solid color-mix(in srgb, var(--accent) 65%, transparent);
    outline-offset: -2px;
    pointer-events: none;
    transition: inset 0.14s cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .drop.left {
    inset: 0 50% 0 0;
  }
  .drop.right {
    inset: 0 0 0 50%;
  }
  .drop.top {
    inset: 0 0 50% 0;
  }
  .drop.bottom {
    inset: 50% 0 0 0;
  }
  /* Con el teclado: intercambiar con esta terminal entera. Sobre la que se mueve: se queda. */
  .drop.full {
    inset: 0;
  }
  .drop.full.here {
    background: transparent;
    outline-style: dashed;
  }
  /* Con el teclado: un lado entero del mosaico, con la parte que le tocaria. */
  .drop.edge {
    z-index: 6;
    margin: 5px;
  }
  .drop.edge.left {
    inset: 0 calc(100% - var(--share)) 0 0;
  }
  .drop.edge.right {
    inset: 0 0 0 calc(100% - var(--share));
  }
  .drop.edge.top {
    inset: 0 0 calc(100% - var(--share)) 0;
  }
  .drop.edge.bottom {
    inset: calc(100% - var(--share)) 0 0 0;
  }
  @media (prefers-reduced-motion: reduce) {
    .tiled .slot,
    .drop {
      transition: none;
      animation: none;
    }
  }
</style>
