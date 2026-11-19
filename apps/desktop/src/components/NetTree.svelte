<script lang="ts">
  // El arbol de red: esta PC a la izquierda, un salto por columna, y las latencias en cada
  // tramo. Los caminos que comparten routers se juntan (ver lib/nettree.ts).
  import type { Trace } from "../lib/api";
  import { buildTree, layout, NODE_H, NODE_W, rttLevel, type NetNode } from "../lib/nettree";

  let { traces, onpick }: { traces: Trace[]; onpick?: (host: string) => void } = $props();

  const graph = $derived(layout(buildTree(traces)));
  const PAD = 12;

  function path(a: NetNode, b: NetNode): string {
    const x1 = a.x + NODE_W + PAD;
    const y1 = a.y + NODE_H / 2 + PAD;
    const x2 = b.x + PAD;
    const y2 = b.y + NODE_H / 2 + PAD;
    const mx = (x1 + x2) / 2;
    return `M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`;
  }

  const fmt = (ms: number | null) => (ms === null ? "—" : ms < 10 ? `${ms.toFixed(1)} ms` : `${Math.round(ms)} ms`);

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
</script>

{#snippet body(n: NetNode)}
  <title>{tip(n)}</title>
  <rect width={NODE_W} height={NODE_H} rx="7" />
  <circle class="dot {n.kind === 'root' ? 'ok' : rttLevel(n.rtt)}" cx="12" cy={NODE_H / 2} r="4" />
  <text class="label" x="24" y={n.sub ? 17 : NODE_H / 2 + 4}>{cut(n.label, 20)}</text>
  {#if n.sub}<text class="sub" x="24" y="31">{cut(n.sub, 24)}</text>{/if}
{/snippet}

<div class="tree">
  <svg width={graph.width + PAD * 2} height={graph.height + PAD * 2} role="img" aria-label="Network path">
    {#each graph.edges as e (e.from.key + ">" + e.to.key)}
      <path class="edge {rttLevel(e.to.rtt)}" class:dashed={e.dashed} d={path(e.from, e.to)} />
      {#if e.to.rtt !== null}
        <text class="rtt {rttLevel(e.to.rtt)}" x={(e.from.x + NODE_W + e.to.x) / 2 + PAD} y={(e.from.y + e.to.y) / 2 + NODE_H / 2 + PAD - 5} text-anchor="middle">{fmt(e.to.rtt)}</text>
      {/if}
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
          onkeydown={(e) => e.key === "Enter" && pick()}
        >
          {@render body(n)}
        </g>
      {:else}
        <g class="node {n.kind}" transform="translate({n.x + PAD} {n.y + PAD})">
          {@render body(n)}
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
  svg {
    display: block;
  }
  .edge {
    fill: none;
    stroke: var(--line-strong);
    stroke-width: 2;
  }
  .edge.ok {
    stroke: color-mix(in srgb, var(--ok) 70%, transparent);
  }
  .edge.warn {
    stroke: color-mix(in srgb, var(--warn) 75%, transparent);
  }
  .edge.bad {
    stroke: color-mix(in srgb, var(--danger) 75%, transparent);
  }
  .edge.dashed {
    stroke-dasharray: 5 5;
  }
  .rtt {
    font-family: var(--mono);
    font-size: 10.5px;
    fill: var(--text-2);
    paint-order: stroke;
    stroke: var(--panel);
    stroke-width: 4px;
  }
  .rtt.warn {
    fill: var(--warn);
  }
  .rtt.bad {
    fill: var(--danger);
  }
  .node rect {
    fill: var(--float, var(--panel-2));
    stroke: var(--line-strong);
    stroke-width: 1;
  }
  .node.root rect,
  .node.host rect {
    stroke: var(--accent);
  }
  .node.pick {
    cursor: pointer;
    outline: none;
  }
  .node.pick:hover rect,
  .node.pick:focus-visible rect {
    stroke-width: 2;
  }
  .node.lost rect,
  .node.jump rect,
  .node.unreached rect {
    stroke-dasharray: 4 3;
  }
  .node.unreached rect {
    stroke: var(--danger);
  }
  .dot {
    fill: var(--text-3);
  }
  .dot.ok {
    fill: var(--ok);
  }
  .dot.warn {
    fill: var(--warn);
  }
  .dot.bad {
    fill: var(--danger);
  }
  .node.unreached .dot {
    fill: var(--danger);
  }
  .label {
    font-size: 12.5px;
    font-weight: 600;
    fill: var(--text);
  }
  .node.lost .label {
    fill: var(--text-3);
  }
  .sub {
    font-family: var(--mono);
    font-size: 10.5px;
    fill: var(--text-3);
  }
</style>
