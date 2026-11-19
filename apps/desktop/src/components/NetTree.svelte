<script lang="ts">
  // El arbol de red: esta PC, un salto por nivel y la latencia de cada tramo. Los caminos
  // que comparten routers se juntan (ver lib/nettree.ts). Horizontal en el mapa; vertical
  // en el test de un host, que es angosto.
  import { CircleHelp, KeyRound, Monitor, Router, Server, ServerOff } from "@lucide/svelte";
  import type { Trace } from "../lib/api";
  import { buildTree, layout, NODE_H, NODE_W, rttLevel, type NetNode, type NodeKind } from "../lib/nettree";

  let { traces, onpick, vertical = false }: { traces: Trace[]; onpick?: (host: string) => void; vertical?: boolean } = $props();

  const graph = $derived(layout(buildTree(traces), vertical));
  // Margen para las sombras y el numero de salto, que sale un poco del nodo.
  const PAD = 14;

  const ICON = { root: Monitor, router: Router, host: Server, lost: CircleHelp, jump: KeyRound, unreached: ServerOff } as const;

  /** Las puntas de un tramo: del borde del padre al borde del hijo. */
  function ends(a: NetNode, b: NetNode) {
    return vertical
      ? { x1: a.x + NODE_W / 2 + PAD, y1: a.y + NODE_H + PAD, x2: b.x + NODE_W / 2 + PAD, y2: b.y + PAD }
      : { x1: a.x + NODE_W + PAD, y1: a.y + NODE_H / 2 + PAD, x2: b.x + PAD, y2: b.y + NODE_H / 2 + PAD };
  }

  function path(a: NetNode, b: NetNode): string {
    const { x1, y1, x2, y2 } = ends(a, b);
    if (vertical) {
      const my = (y1 + y2) / 2;
      return `M ${x1} ${y1} C ${x1} ${my}, ${x2} ${my}, ${x2} ${y2}`;
    }
    const mx = (x1 + x2) / 2;
    return `M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`;
  }

  /** Donde va la pastilla con la latencia: al medio del tramo. */
  function mid(a: NetNode, b: NetNode) {
    const { x1, y1, x2, y2 } = ends(a, b);
    return { x: (x1 + x2) / 2, y: (y1 + y2) / 2 };
  }

  const fmt = (ms: number) => (ms < 10 ? `${ms.toFixed(1)} ms` : `${Math.round(ms)} ms`);
  const pillW = (ms: number) => fmt(ms).length * 6.6 + 14;

  function tip(n: NetNode): string {
    const lines = [n.label];
    if (n.sub) lines.push(n.sub);
    if (n.ttl) lines.push(`Hop ${n.ttl}`);
    if (n.rtts.length) lines.push(`Probes: ${n.rtts.map((r) => (r === null ? "*" : `${r} ms`)).join(", ")}`);
    if (n.kind !== "root" && n.hosts.size) lines.push(`On the way to: ${[...n.hosts].join(", ")}`);
    return lines.join("\n");
  }

  /** Corta el texto para que entre en el nodo (el SVG no tiene ellipsis). */
  const cut = (s: string, n: number) => (s.length > n ? s.slice(0, n - 1) + "…" : s);
  const tone = (n: NetNode): string => (n.kind === "root" ? "accent" : n.kind === "unreached" ? "bad" : n.kind === "lost" ? "none" : rttLevel(n.rtt));
  const iconOf = (k: NodeKind) => ICON[k];
</script>

<div class="tree" class:vertical>
  <svg width={graph.width + PAD * 2} height={graph.height + PAD * 2} role="img" aria-label="Network path">
    <!-- El nodo se define adentro del <svg>: asi sus rect y text son SVG y no HTML. -->
    {#snippet body(n: NetNode)}
      {@const Icon = iconOf(n.kind)}
      <title>{tip(n)}</title>
      <rect class="card" width={NODE_W} height={NODE_H} rx="10" />
      <rect class="bar {tone(n)}" x="0" y="8" width="3" height={NODE_H - 16} rx="1.5" />
      <circle class="badge {tone(n)}" cx="21" cy={NODE_H / 2} r="12" />
      <Icon x={14} y={NODE_H / 2 - 7} width={14} height={14} class="ico {tone(n)}" />
      <text class="label" x="40" y={n.sub ? 17 : NODE_H / 2 + 4}>{cut(n.label, 18)}</text>
      {#if n.sub}<text class="sub" x="40" y="31">{cut(n.sub, 21)}</text>{/if}
      {#if n.ttl && n.kind !== "jump" && n.kind !== "unreached"}
        <g class="hop" transform="translate({NODE_W - 4} -4)">
          <circle r="9" />
          <text y="3.5" text-anchor="middle">{n.ttl}</text>
        </g>
      {/if}
    {/snippet}

    {#each graph.edges as e (e.from.key + ">" + e.to.key)}
      <path class="edge base" d={path(e.from, e.to)} />
      <path class="edge {rttLevel(e.to.rtt)}" class:dashed={e.dashed} class:flow={!e.dashed} d={path(e.from, e.to)} />
    {/each}

    {#each graph.nodes as n (n.key)}
      {#if n.kind === "host" && onpick}
        {@const pick = () => onpick(n.dest[0] ?? n.label)}
        <g
          class="node host pick"
          transform="translate({n.x + PAD} {n.y + PAD})"
          role="button"
          tabindex="0"
          onclick={pick}
          onkeydown={(ev) => ev.key === "Enter" && pick()}
        >
          {@render body(n)}
        </g>
      {:else}
        <g class="node {n.kind}" transform="translate({n.x + PAD} {n.y + PAD})">
          {@render body(n)}
        </g>
      {/if}
    {/each}

    <!-- Las latencias, encima de todo para que no las tape un nodo. -->
    {#each graph.edges as e (e.from.key + ">" + e.to.key + ":rtt")}
      {#if e.to.rtt !== null}
        {@const m = mid(e.from, e.to)}
        {@const w = pillW(e.to.rtt)}
        <g class="rtt {rttLevel(e.to.rtt)}" transform="translate({vertical ? m.x + 10 : m.x - w / 2} {m.y - 9})">
          <rect width={w} height="18" rx="9" />
          <text x={w / 2} y="12.5" text-anchor="middle">{fmt(e.to.rtt)}</text>
        </g>
      {/if}
    {/each}
  </svg>
</div>

<style>
  .tree {
    overflow: auto;
    max-width: 100%;
    padding-bottom: 4px;
  }
  .tree.vertical svg {
    margin: 0 auto;
  }
  svg {
    display: block;
    overflow: visible;
  }

  /* ---- tramos */
  .edge {
    fill: none;
    stroke-width: 2.5;
    stroke-linecap: round;
    stroke: var(--line-strong);
  }
  .edge.base {
    stroke: var(--line);
    stroke-width: 6;
    opacity: 0.5;
  }
  .edge.ok {
    stroke: var(--ok);
  }
  .edge.warn {
    stroke: var(--warn);
  }
  .edge.bad {
    stroke: var(--danger);
  }
  .edge.dashed {
    stroke: var(--text-3);
    stroke-dasharray: 2 6;
  }
  /* "Trafico" que corre por los tramos que contestaron. */
  .edge.flow {
    stroke-dasharray: 10 6;
    animation: flow 1.1s linear infinite;
  }
  @keyframes flow {
    to {
      stroke-dashoffset: -16;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .edge.flow {
      animation: none;
      stroke-dasharray: none;
    }
  }

  /* ---- nodos */
  .node {
    transition: transform 0.15s;
  }
  .card {
    fill: var(--float, var(--panel-2));
    stroke: var(--line-strong);
    stroke-width: 1;
    filter: drop-shadow(0 4px 10px rgba(0, 0, 0, 0.25));
  }
  .node.root .card,
  .node.host .card {
    stroke: color-mix(in srgb, var(--accent) 60%, var(--line-strong));
  }
  .node.lost .card,
  .node.jump .card {
    stroke-dasharray: 4 3;
  }
  .node.unreached .card {
    stroke: var(--danger);
    stroke-dasharray: 4 3;
  }
  .node.pick {
    cursor: pointer;
    outline: none;
  }
  .node.pick:hover .card,
  .node.pick:focus-visible .card {
    stroke: var(--accent);
    stroke-width: 2;
  }

  .bar {
    fill: var(--text-3);
  }
  .badge {
    fill: color-mix(in srgb, var(--text-3) 18%, transparent);
  }
  .node :global(.ico) {
    color: var(--text-2);
    stroke-width: 2;
  }
  .bar.ok {
    fill: var(--ok);
  }
  .badge.ok {
    fill: color-mix(in srgb, var(--ok) 18%, transparent);
  }
  .node :global(.ico.ok) {
    color: var(--ok);
  }
  .bar.warn {
    fill: var(--warn);
  }
  .badge.warn {
    fill: color-mix(in srgb, var(--warn) 18%, transparent);
  }
  .node :global(.ico.warn) {
    color: var(--warn);
  }
  .bar.bad {
    fill: var(--danger);
  }
  .badge.bad {
    fill: color-mix(in srgb, var(--danger) 18%, transparent);
  }
  .node :global(.ico.bad) {
    color: var(--danger);
  }
  .bar.accent {
    fill: var(--accent);
  }
  .badge.accent {
    fill: color-mix(in srgb, var(--accent) 20%, transparent);
  }
  .node :global(.ico.accent) {
    color: var(--accent);
  }

  .label {
    font-size: 12.5px;
    font-weight: 650;
    fill: var(--text);
  }
  .node.lost .label {
    fill: var(--text-3);
    font-weight: 500;
  }
  .sub {
    font-family: var(--mono);
    font-size: 10.5px;
    fill: var(--text-3);
  }
  .hop circle {
    fill: var(--panel);
    stroke: var(--line-strong);
  }
  .hop text {
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 650;
    fill: var(--text-2);
  }

  /* ---- pastillas de latencia */
  .rtt rect {
    fill: var(--float, var(--panel));
    stroke: var(--line-strong);
  }
  .rtt text {
    font-family: var(--mono);
    font-size: 10.5px;
    font-weight: 600;
    fill: var(--text-2);
  }
  .rtt.ok rect {
    stroke: color-mix(in srgb, var(--ok) 60%, transparent);
  }
  .rtt.ok text {
    fill: var(--ok);
  }
  .rtt.warn rect {
    stroke: color-mix(in srgb, var(--warn) 60%, transparent);
  }
  .rtt.warn text {
    fill: var(--warn);
  }
  .rtt.bad rect {
    stroke: color-mix(in srgb, var(--danger) 60%, transparent);
  }
  .rtt.bad text {
    fill: var(--danger);
  }
</style>
