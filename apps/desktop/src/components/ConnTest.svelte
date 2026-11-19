<script lang="ts">
  // Test de conexion detallado: DNS, ping (latencia, jitter, perdida), puerto TCP con el
  // banner del servidor, login SSH (con la confirmacion de huella de siempre) y traceroute.
  import { CircleCheck, CircleX, LoaderCircle, RefreshCw, Route } from "@lucide/svelte";
  import Drawer from "./Drawer.svelte";
  import NetTree from "./NetTree.svelte";
  import { api, type Diagnosis, type Host, type Trace } from "../lib/api";
  import { testHost } from "../lib/hostkey.svelte";
  import { rttLevel } from "../lib/nettree";

  let { host, onclose }: { host: Host; onclose: () => void } = $props();

  const serial = $derived(host.transport === "serial");

  let diag = $state<Diagnosis | null>(null);
  let diagErr = $state("");
  let login = $state<{ ok: boolean; ms: number; why: string } | null>(null);
  let running = $state(false);
  let trace = $state<Trace | null>(null);
  let tracing = $state(false);
  let traceErr = $state("");

  async function run() {
    running = true;
    diag = null;
    diagErr = "";
    login = null;
    const net = serial
      ? Promise.resolve()
      : api.hostDiagnose(host.name, 5).then(
          (d) => (diag = d),
          (e) => (diagErr = String(e)),
        );
    const t0 = performance.now();
    let why = "";
    const ok = await testHost(host.name, true, (text) => (why = text));
    login = { ok, ms: Math.round(performance.now() - t0), why };
    await net;
    running = false;
  }

  async function runTrace() {
    tracing = true;
    traceErr = "";
    try {
      trace = await api.hostTrace(host.name);
    } catch (e) {
      traceErr = String(e);
    }
    tracing = false;
  }

  $effect(() => {
    void host.name;
    run();
  });

  const ms = (x: number | null | undefined) => (x === null || x === undefined ? "—" : x < 10 ? `${x.toFixed(2)} ms` : `${x.toFixed(1)} ms`);

  /** Que dice el TTL de la respuesta sobre el equipo (los valores iniciales mas comunes). */
  function ttlHint(ttl: number | null): string {
    if (ttl === null) return "";
    const start = ttl <= 64 ? 64 : ttl <= 128 ? 128 : 255;
    const hops = start - ttl;
    const os = start === 64 ? "Linux / Unix" : start === 128 ? "Windows" : "network device";
    return `${os}, ${hops} router${hops === 1 ? "" : "s"} away`;
  }

  // Las barritas del ping: alto segun la latencia (relativa al maximo).
  const bars = $derived.by(() => {
    const s = diag?.ping.samples ?? [];
    const max = Math.max(1, ...s.filter((x): x is number => x !== null));
    return s.map((x) => ({ x, h: x === null ? 100 : Math.max(8, (x / max) * 100) }));
  });
</script>

<Drawer title="Test {host.name}" {onclose}>
  <div class="row head">
    <span class="muted">{serial ? `Serial console ${host.serial.port}` : diag ? `${diag.address}:${diag.port}` : `${host.hostname}:${host.port}`}</span>
    <span class="grow"></span>
    <button class="btn sm" onclick={run} disabled={running}><RefreshCw size={13} /> Run again</button>
  </div>
  {#if diag?.via.length}
    <div class="faint">Goes through {diag.via.join(" → ")}: ping, port and route are measured to {diag.via[0]}.</div>
  {/if}

  {#if !serial}
    <section>
      <h4>Ping</h4>
      {#if !diag && !diagErr}
        <div class="muted wait"><LoaderCircle size={14} class="spin" /> Sending 5 pings…</div>
      {:else if diagErr}
        <div class="bad">{diagErr}</div>
      {:else if diag}
        {#if diag.ping.error}
          <div class="bad">{diag.ping.error}</div>
        {:else}
          <div class="stats">
            <div><span class="k">Average</span><span class="v {rttLevel(diag.ping.avg_ms)}">{ms(diag.ping.avg_ms)}</span></div>
            <div><span class="k">Min / max</span><span class="v">{ms(diag.ping.min_ms)} / {ms(diag.ping.max_ms)}</span></div>
            <div><span class="k">Jitter</span><span class="v">{ms(diag.ping.jitter_ms)}</span></div>
            <div><span class="k">Loss</span><span class="v {diag.ping.loss === 0 ? 'ok' : diag.ping.loss < 50 ? 'warn' : 'bad'}">{diag.ping.loss}% <span class="faint">({diag.ping.received}/{diag.ping.sent})</span></span></div>
          </div>
          <div class="bars" aria-hidden="true">
            {#each bars as b}<span class="bar {b.x === null ? 'lost' : rttLevel(b.x)}" style="height: {b.h}%" title={b.x === null ? "lost" : ms(b.x)}></span>{/each}
          </div>
          <div class="kv">
            {#if diag.ip}<span class="k">IP</span><span class="mono">{diag.ip}</span>{/if}
            {#if diag.resolve_ms !== null && diag.ip !== diag.address}<span class="k">DNS</span><span>{ms(diag.resolve_ms)}</span>{/if}
            {#if diag.ping.ttl !== null}<span class="k">TTL</span><span>{diag.ping.ttl} <span class="faint">· probably {ttlHint(diag.ping.ttl)}</span></span>{/if}
          </div>
          {#if diag.ping.received === 0}
            <div class="faint">No reply to ping. Many servers and firewalls block it; the port test below still tells if the host is reachable.</div>
          {/if}
        {/if}
      {/if}
    </section>

    <section>
      <h4>Port {diag?.port ?? host.port}</h4>
      {#if diag}
        {#if diag.tcp.ok}
          <div class="line"><CircleCheck size={15} class="ok" /> Open · connects in <b>{ms(diag.tcp.connect_ms)}</b></div>
          {#if diag.tcp.banner}<div class="kv"><span class="k">Server</span><span class="mono">{diag.tcp.banner}</span></div>{/if}
        {:else}
          <div class="line"><CircleX size={15} class="bad" /> {diag.tcp.error}</div>
        {/if}
      {:else if !diagErr}
        <div class="muted wait"><LoaderCircle size={14} class="spin" /> Connecting…</div>
      {/if}
    </section>
  {/if}

  <section>
    <h4>{serial ? "Console" : "SSH login"}</h4>
    {#if login}
      {#if login.ok}
        <div class="line"><CircleCheck size={15} class="ok" /> {serial ? "Port opened" : "Logged in"} · <b>{login.ms} ms</b> <span class="faint">(connection, key exchange and authentication{host.jump.length ? ", through the jumps" : ""})</span></div>
      {:else}
        <div class="line"><CircleX size={15} class="bad" /> {login.why || "Failed"}</div>
      {/if}
    {:else}
      <div class="muted wait"><LoaderCircle size={14} class="spin" /> {serial ? "Opening the port…" : "Logging in…"}</div>
    {/if}
  </section>

  {#if !serial}
    <section>
      <div class="row">
        <h4>Route</h4>
        <span class="grow"></span>
        <button class="btn sm" onclick={runTrace} disabled={tracing}>
          {#if tracing}<LoaderCircle size={13} class="spin" /> Tracing…{:else}<Route size={13} /> {trace ? "Trace again" : "Traceroute"}{/if}
        </button>
      </div>
      {#if traceErr}
        <div class="bad">{traceErr}</div>
      {:else if trace}
        {#if trace.error}<div class="bad">{trace.error}</div>{/if}
        {#if trace.hops.length}
          <NetTree traces={[trace]} />
          <table class="list hops">
            <thead><tr><th>#</th><th>Hop</th><th>Probes</th></tr></thead>
            <tbody>
              {#each trace.hops as h}
                <tr>
                  <td class="faint">{h.ttl}</td>
                  <td>
                    {#if h.ip}
                      {#if h.host}<b>{h.host}</b> {/if}<span class="mono">{h.ip}</span>
                      {#if h.dns}<div class="faint mono">{h.dns}</div>{/if}
                    {:else}<span class="faint">no reply</span>{/if}
                  </td>
                  <td class="mono">{#each h.rtts as r, i}{i ? " · " : ""}<span class={r === null ? "faint" : rttLevel(r)}>{r === null ? "*" : ms(r)}</span>{/each}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          {#if !trace.reached}<div class="faint">The last routers didn't answer: the host or a firewall on the way drops ping and traceroute.</div>{/if}
        {/if}
      {:else if !tracing}
        <div class="faint">Every router between this PC and {diag?.via[0] ?? host.name}, with its latency. Hosts in the vault show by name.</div>
      {/if}
    </section>
  {/if}
</Drawer>

<style>
  .head {
    align-items: center;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 14px;
    border-top: var(--border) solid var(--line);
  }
  h4 {
    margin: 0;
    font-size: 11.5px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }
  .stats > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    background: var(--panel-2);
    border-radius: var(--radius-sm);
  }
  .k {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .v {
    font-family: var(--mono);
    font-size: 13px;
  }
  .kv {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
    align-items: baseline;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .wait {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .bars {
    display: flex;
    align-items: flex-end;
    gap: 4px;
    height: 34px;
  }
  .bar {
    flex: 1;
    max-width: 40px;
    border-radius: 3px 3px 0 0;
    background: var(--text-3);
  }
  .bar.lost {
    background: repeating-linear-gradient(45deg, var(--danger) 0 3px, transparent 3px 6px);
    opacity: 0.6;
  }
  .ok,
  .line :global(svg.ok) {
    color: var(--ok);
  }
  .bar.ok {
    background: var(--ok);
  }
  .warn {
    color: var(--warn);
  }
  .bar.warn {
    background: var(--warn);
  }
  .bad,
  .line :global(svg.bad) {
    color: var(--danger);
  }
  .bar.bad {
    background: var(--danger);
  }
  .hops td {
    vertical-align: top;
  }
</style>
