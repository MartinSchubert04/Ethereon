<script lang="ts">
  // Encima de una terminal caida o conectando, como Termius: el camino de la conexion en
  // nodos (esta PC, los saltos, el host) unidos por una linea. Conectando, la linea avanza;
  // caida, el tramo final queda cortado en rojo y hay un boton para reintentar.
  import { Monitor, RotateCw, Server, SquareTerminal } from "@lucide/svelte";
  import { app, type TermTab } from "../lib/state.svelte";
  import { pretty, prefs } from "../lib/prefs.svelte";
  import { fadeQuick } from "../lib/motion";

  let { m, onretry }: { m: TermTab; onretry: () => void } = $props();

  const dead = $derived(!m.alive && !m.connecting);
  // Conectando solo se muestra por SSH: una shell local abre al instante.
  const shown = $derived(!m.sftp && (dead || (!!m.connecting && !m.local)));

  const nodes = $derived.by(() => {
    if (m.local) return [{ name: "This PC", kind: "pc" }, { name: m.host, kind: "shell" }];
    const h = app.snap?.hosts.find((x) => x.name === m.host);
    return [
      { name: "This PC", kind: "pc" },
      ...(h?.jump ?? []).map((j) => ({ name: j, kind: "jump" })),
      { name: m.host, kind: "host" },
    ];
  });

  // Donde se corto: el primer host de la cadena que nombra el error ("could not connect to
  // bastion"); si no nombra ninguno, el que informo la falla; si no, el ultimo.
  const cut = $derived.by(() => {
    const said = nodes.findIndex((n, k) => k > 0 && k < nodes.length - 1 && !!m.error && new RegExp(`\\b${n.name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`).test(m.error));
    if (said > 0) return said;
    const at = nodes.findIndex((n, k) => k > 0 && n.name === m.failedAt);
    return at > 0 ? at : nodes.length - 1;
  });
</script>

{#if shown}
  <div class="conn" class:dead transition:fadeQuick>
    <div class="path">
      {#each nodes as n, i (i)}
        {#if i > 0}
          <span class="link" class:broken={dead && i === cut} class:off={dead && i > cut} class:moving={!dead}></span>
        {/if}
        <span class="node" class:end={dead ? i === cut : i === nodes.length - 1} class:off={dead && i > cut}>
          <span class="ic">
            {#if n.kind === "pc"}<Monitor size={15} />{:else if n.kind === "shell"}<SquareTerminal size={15} />{:else}<Server size={15} />{/if}
          </span>
          <span class="name ellipsis">{n.name}</span>
        </span>
      {/each}
    </div>
    <div class="msg">
      {#if dead}
        {m.local ? "The shell ended" : "Disconnected"}
      {:else}
        Connecting…
      {/if}
    </div>
    {#if dead && m.error}
      <div class="why">{m.error}</div>
    {/if}
    {#if dead}
      <button class="retry" onclick={onretry}>
        <RotateCw size={13} />{m.local ? "Restart" : "Retry"}
        <span class="key">{pretty(prefs.binds.refresh)}</span>
      </button>
    {/if}
  </div>
{/if}

<style>
  .conn {
    position: absolute;
    inset: 0;
    z-index: 4;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 14px;
    padding: 16px;
    border-radius: inherit;
    background: color-mix(in srgb, var(--term-bg) 82%, transparent);
    backdrop-filter: blur(2px);
  }
  .path {
    display: flex;
    align-items: center;
    max-width: 100%;
  }
  .node {
    flex: none;
    display: grid;
    justify-items: center;
    gap: 6px;
    max-width: 150px;
  }
  .ic {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border: 1px solid var(--line-strong);
    border-radius: 50%;
    background: var(--panel);
    color: var(--text-2);
    transition:
      border-color 0.2s,
      color 0.2s;
  }
  .end .ic {
    border-color: color-mix(in srgb, var(--accent) 60%, var(--line-strong));
    color: var(--text);
    animation: breathe 1.6s ease-in-out infinite;
  }
  .dead .end .ic {
    border-color: var(--danger);
    color: var(--danger);
    animation: none;
  }
  .name {
    max-width: 100%;
    font: 11px var(--mono);
    color: var(--text-3);
  }
  /* La linea entre nodos: punteada y avanzando mientras conecta. */
  .link {
    flex: 0 1 64px;
    min-width: 18px;
    height: 2px;
    margin: 0 6px 19px;
    border-radius: 1px;
    background: var(--line-strong);
  }
  .link.moving {
    background: repeating-linear-gradient(90deg, var(--accent) 0 6px, transparent 6px 11px);
    background-size: 22px 2px;
    opacity: 0.8;
    animation: march 0.7s linear infinite;
  }
  /* Despues del corte: no se llego. */
  .node.off,
  .link.off {
    opacity: 0.35;
  }
  .link.broken {
    background: repeating-linear-gradient(90deg, var(--danger) 0 5px, transparent 5px 10px);
  }
  .msg {
    font-size: 12.5px;
    color: var(--text-2);
  }
  .dead .msg {
    color: var(--danger);
  }
  .why {
    max-width: min(460px, 90%);
    margin-top: -8px;
    text-align: center;
    font: 11.5px var(--mono);
    color: var(--text-3);
    overflow-wrap: anywhere;
  }
  .retry {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--line-strong);
    border-radius: 999px;
    background: var(--panel-2);
    color: var(--text);
    font-size: 12.5px;
    font-weight: 600;
    transition:
      transform 0.12s,
      border-color 0.15s;
  }
  .retry:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
  }
  .retry:active {
    transform: translateY(0) scale(0.97);
  }
  .key {
    font: 10.5px var(--mono);
    color: var(--text-3);
  }
  @keyframes march {
    to {
      background-position: 22px 0;
    }
  }
  @keyframes breathe {
    50% {
      box-shadow: 0 0 0 5px color-mix(in srgb, var(--accent) 14%, transparent);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .link.moving,
    .end .ic {
      animation: none;
    }
  }
</style>
