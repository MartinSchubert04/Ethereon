<script lang="ts">
  // Encima de una terminal caida o conectando, como Termius: el camino de la conexion en
  // nodos (esta PC, los saltos, el host) unidos por una linea. Conectando, la linea avanza;
  // caida, el tramo final queda cortado en rojo y hay un boton para reintentar.
  // La flechita despliega el registro de la conexion: que paso, y lo que dijo el servidor
  // (Tailscale SSH manda ahi el link para autorizar el login y espera hasta que se abra).
  import { ChevronDown, Monitor, RotateCw, Server, SquareTerminal } from "@lucide/svelte";
  import { slide } from "svelte/transition";
  import { api } from "../lib/api";
  import { app, toast, type TermTab } from "../lib/state.svelte";
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

  // El registro solo es de SSH (una shell local no lo tiene).
  const lines = $derived(m.local ? [] : (app.connLogs[m.host] ?? []));
  const hasLog = $derived(!m.local);
  let open = $state(false);
  let box: HTMLDivElement | undefined = $state();

  // Si el servidor dice algo mientras conecta (un link para autorizar), se abre solo.
  let told = 0;
  $effect(() => {
    const n = lines.filter((l) => l.line.startsWith("Server ")).length;
    if (n > told && !dead) open = true;
    told = n;
  });

  // Siempre mostrando lo ultimo.
  $effect(() => {
    void lines.length;
    if (box) box.scrollTop = box.scrollHeight;
  });

  const time = (ts: string) => new Date(ts).toLocaleTimeString([], { hour12: false });

  /** Corta una linea en texto y links https, para poder abrirlos. */
  function parts(line: string) {
    return line.split(/(https:\/\/[^\s"'<>]+[^\s"'<>.,;:)\]])/).map((text, i) => ({ text, url: i % 2 === 1 }));
  }

  function openLink(url: string) {
    api.openLink(url).catch((e) => toast(String(e), true));
  }
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
    {#if dead || hasLog}
      <div class="actions">
        {#if dead}
          <button class="retry" onclick={onretry}>
            <RotateCw size={13} />{m.local ? "Restart" : "Retry"}
            <span class="key">{pretty(prefs.binds.refresh)}</span>
          </button>
        {/if}
        {#if hasLog}
          <button
            class="logs"
            class:open
            class:bare={dead}
            aria-expanded={open}
            title={open ? "Hide logs" : "Show logs"}
            aria-label={open ? "Hide logs" : "Show logs"}
            onclick={() => (open = !open)}
          >
            {#if !dead}{open ? "Hide logs" : "Show logs"}{/if}
            <span class="chev"><ChevronDown size={14} /></span>
          </button>
        {/if}
      </div>
    {/if}
    {#if hasLog && open}
      <div class="log" bind:this={box} transition:slide={{ duration: 160 }}>
        {#each lines as l, i (i)}
          <div class="row">
            <span class="t">{time(l.ts)}</span>
            <span class="l"
              >{#each parts(l.line) as p, j (j)}{#if p.url}<a
                    href={p.text}
                    onclick={(e) => {
                      e.preventDefault();
                      openLink(p.text);
                    }}>{p.text}</a
                  >{:else}{p.text}{/if}{/each}</span
            >
          </div>
        {:else}
          <div class="row empty">Nothing logged yet</div>
        {/each}
      </div>
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
    /* Una sola columna del ancho del panel: el registro no se sale en un mosaico chico. */
    grid-template-columns: minmax(0, 100%);
    justify-items: center;
    gap: 14px;
    padding: 16px;
    border-radius: inherit;
    background: color-mix(in srgb, var(--term-bg) 82%, transparent);
    backdrop-filter: blur(2px);
  }
  /* La distancia va de circulo a circulo: el nodo mide lo que el circulo y el nombre cuelga
     debajo, centrado, sin empujar la linea. El relleno deja lugar a los nombres de los
     extremos ((110 - 34) / 2 a los costados) y a la fila de nombres abajo. */
  .path {
    display: flex;
    align-items: center;
    max-width: 100%;
    padding: 0 38px 20px;
  }
  .node {
    position: relative;
    flex: none;
    display: grid;
    width: 34px;
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
  /* Hasta 110px: con la linea entera (64 + 2 * 10) no pisa al nombre del nodo de al lado. */
  .name {
    position: absolute;
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
    width: max-content;
    max-width: 110px;
    text-align: center;
    font: 11px var(--mono);
    color: var(--text-3);
  }
  /* La linea entre nodos: punteada y avanzando mientras conecta. El margen deja afuera el
     pulso del circulo (5px) y un poco de aire. */
  /* Con width y no flex-basis: el ancho intrinseco del flex (la fila esta centrada, mide lo
     que su contenido) ignora el basis de un span vacio y la dejaba en el minimo. */
  .link {
    flex: 0 1 auto;
    width: 64px;
    min-width: 18px;
    height: 2px;
    margin: 0 10px;
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
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  /* Mas apagado que Retry: es para mirar, no la accion principal. */
  .logs {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px 0 12px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: transparent;
    color: var(--text-3);
    font-size: 12px;
    font-weight: 550;
    transition:
      color 0.15s,
      border-color 0.15s;
  }
  .logs.bare {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .logs:hover,
  .logs.open {
    color: var(--text-2);
    border-color: var(--line-strong);
  }
  .chev {
    display: grid;
    transition: transform 0.2s;
  }
  .logs.open .chev {
    transform: rotate(180deg);
  }
  .log {
    width: 560px;
    max-width: 100%;
    max-height: min(220px, 40vh);
    overflow: auto;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--panel);
    font: 11px/1.5 var(--mono);
    color: var(--text-2);
    text-align: left;
  }
  .row {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 10px;
  }
  .row.empty {
    display: block;
    color: var(--text-3);
  }
  .t {
    color: var(--text-3);
  }
  .l {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .l a {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
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
