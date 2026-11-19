<script lang="ts">
  // Mapa de red: traceroute a todos los hosts y un arbol con las ramas comunes juntas.
  import { LoaderCircle, Network, Route } from "@lucide/svelte";
  import ConnTest from "../components/ConnTest.svelte";
  import NetTree from "../components/NetTree.svelte";
  import Select from "../components/Select.svelte";
  import TftpPanel from "../components/TftpPanel.svelte";
  import { api, type Host, type Trace } from "../lib/api";
  import { app, toast } from "../lib/state.svelte";

  let traces = $state<Trace[] | null>(null);
  let running = $state(false);
  let took = $state(0);
  let group = $state("");
  let testing = $state<Host | null>(null);

  const netHosts = $derived((app.snap?.hosts ?? []).filter((h) => h.transport !== "serial"));
  const groups = $derived([...new Set(netHosts.map((h) => h.group ?? ""))].sort());
  const shown = $derived(
    (traces ?? []).filter((t) => {
      if (!group) return true;
      const h = netHosts.find((x) => x.name === t.host);
      return (h?.group ?? "") === (group === "—" ? "" : group);
    }),
  );
  const failed = $derived(shown.filter((t) => t.error || !t.reached));

  async function run() {
    running = true;
    const t0 = performance.now();
    try {
      traces = await api.traceAll();
    } catch (e) {
      toast(String(e), true);
    }
    took = Math.round((performance.now() - t0) / 100) / 10;
    running = false;
  }

  function pick(name: string) {
    testing = netHosts.find((h) => h.name === name) ?? null;
  }
</script>

<div class="page">
  <div class="page-head">
    <h1>Network</h1>
    <span class="sub">The route from this PC to every host, merged where the paths share routers. And a TFTP server for network gear.</span>
    <span class="grow"></span>
    {#if groups.length > 1 && traces}
      <div class="grp">
        <Select
          bind:value={group}
          options={[{ value: "", label: "All groups" }, ...groups.map((g) => ({ value: g || "—", label: g || "No group" }))]}
        />
      </div>
    {/if}
    <button class="btn primary" onclick={run} disabled={running || !netHosts.length}>
      {#if running}<LoaderCircle size={14} class="spin" /> Tracing {netHosts.length} hosts…{:else}<Route size={14} /> {traces ? "Trace again" : "Map the network"}{/if}
    </button>
  </div>

  {#if !netHosts.length}
    <div class="empty">
      <Network size={30} />
      <h3>No network hosts</h3>
      <div>Serial consoles have no route to trace.</div>
    </div>
  {:else if !traces}
    <div class="empty">
      <Network size={30} />
      <h3>{running ? "Tracing…" : "Map your network"}</h3>
      <div>
        Runs a traceroute to each of your {netHosts.length} hosts (6 at a time) and draws the tree with the latency of each hop.
        Routers that are hosts in the vault show by name. Click a host to test it.
      </div>
    </div>
  {:else}
    <div class="card map">
      <NetTree traces={shown} onpick={pick} />
    </div>
    <div class="foot faint">
      {shown.length} host{shown.length === 1 ? "" : "s"} in {took} s.
      <span class="legend"><i class="ok"></i> under 30 ms <i class="warn"></i> under 120 ms <i class="bad"></i> slower · dashed: no reply or through SSH</span>
    </div>
    {#if failed.length}
      <div class="card fails">
        <h4>Didn't reach</h4>
        {#each failed as t}
          <button class="fail" onclick={() => pick(t.host)}>
            <b>{t.host}</b> <span class="mono faint">{t.ip ?? t.address}</span>
            <span class="faint">{t.error ?? "the last hops didn't answer (ping is often blocked)"}</span>
          </button>
        {/each}
      </div>
    {/if}
  {/if}

  <div class="tftp-wrap"><TftpPanel /></div>
</div>

{#if testing}
  <ConnTest host={testing} onclose={() => (testing = null)} />
{/if}

<style>
  .grp {
    width: 180px;
  }
  .tftp-wrap {
    margin-top: 18px;
  }
  .map {
    padding: 14px;
    overflow: auto;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-top: 8px;
    font-size: 12px;
  }
  .legend {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .legend i {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    margin-left: 6px;
  }
  .legend i.ok {
    background: var(--ok);
  }
  .legend i.warn {
    background: var(--warn);
  }
  .legend i.bad {
    background: var(--danger);
  }
  .fails {
    margin-top: 14px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  h4 {
    margin: 0 0 4px;
    font-size: 11.5px;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .fail {
    display: flex;
    gap: 10px;
    align-items: baseline;
    text-align: left;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    background: none;
    border: 0;
    color: var(--text);
    cursor: pointer;
  }
  .fail:hover {
    background: var(--panel-2);
  }
</style>
