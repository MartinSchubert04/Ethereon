<script lang="ts">
  import { Download, RefreshCw, ShieldCheck } from "@lucide/svelte";
  import { listen } from "@tauri-apps/api/event";
  import { save } from "@tauri-apps/plugin-dialog";
  import { onDestroy } from "svelte";
  import { api, type AuditEntry } from "../lib/api";
  import { attempt, toast } from "../lib/state.svelte";

  let query = $state("");
  let rows = $state<AuditEntry[]>([]);
  let open = $state<number | null>(null);

  async function load() {
    rows = await api.auditSearch(query);
  }
  load();
  // Se actualiza sola cuando el agente corre algo.
  const un = listen("activity", () => load());
  onDestroy(() => un.then((f) => f()));

  async function verify() {
    try {
      toast(await api.auditVerify());
    } catch (e) {
      toast(String(e), true);
    }
  }

  async function exportLog() {
    const path = await save({
      defaultPath: "ethereon-audit.csv",
      filters: [
        { name: "CSV", extensions: ["csv"] },
        { name: "JSON lines", extensions: ["jsonl"] },
      ],
    });
    if (path) attempt(() => api.auditExport(path), "Exported");
  }

  const tone = (d: string) =>
    d === "allowed" || d === "approved" ? "ok" : d === "denied" || d === "rejected" || d === "error" ? "bad" : "warn";
  const time = (iso: string) => new Date(iso).toLocaleString();
</script>

<div class="page">
  <div class="page-head">
    <h1>Audit</h1>
    <span class="sub">Every agent request and every terminal you open.</span>
    <span class="grow"></span>
    <input class="q" placeholder="Search host, command, device..." bind:value={query} oninput={load} />
    <button class="icon-btn" title="Reload" onclick={load}><RefreshCw size={14} /></button>
    <button class="btn" onclick={verify}><ShieldCheck size={14} /> Verify</button>
    <button class="btn" onclick={exportLog}><Download size={14} /> Export</button>
  </div>

  {#if !rows.length}
    <div class="empty"><h3>Nothing here yet</h3></div>
  {:else}
    <div class="card">
      <table class="list">
        <thead><tr><th>Time</th><th>Device</th><th>Host</th><th>Command</th><th>Result</th><th>Exit</th><th>ms</th></tr></thead>
        <tbody>
          {#each rows as r, i}
            <tr class="click" onclick={() => (open = open === i ? null : i)}>
              <td class="faint nowrap">{time(r.ts)}</td>
              <td class="muted">{r.device}</td>
              <td>{r.host}</td>
              <td class="mono cmd ellipsis">{#if r.sudo}<span class="chip bad">sudo</span> {/if}{r.command}</td>
              <td><span class="chip {tone(r.decision)}">{r.decision}</span></td>
              <td class="mono">{r.exit_code ?? ""}</td>
              <td class="mono faint">{r.duration_ms ?? ""}</td>
            </tr>
            {#if open === i}
              <tr class="detail">
                <td colspan="7">
                  <div class="kv">
                    {#if r.reason}<span>Reason</span><span>{r.reason}</span>{/if}
                    {#if r.detail}<span>Detail</span><span>{r.detail}</span>{/if}
                    {#if r.approved_by}<span>Approved by</span><span>{r.approved_by}</span>{/if}
                    {#if r.client}<span>From</span><span class="mono">{r.client}</span>{/if}
                    {#if r.output_sha256}<span>Output SHA-256</span><span class="mono">{r.output_sha256}</span>{/if}
                    {#if r.masked}<span>Masked</span><span>{r.masked} secrets replaced by ***</span>{/if}
                  </div>
                  {#if r.output}<pre>{r.output}</pre>{/if}
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<style>
  .q {
    width: 260px;
  }
  .cmd {
    max-width: 380px;
  }
  .nowrap {
    white-space: nowrap;
  }
  .detail td {
    background: var(--panel-2);
  }
  .kv {
    display: grid;
    grid-template-columns: 130px 1fr;
    gap: 6px 12px;
    font-size: 12.5px;
    user-select: text;
  }
  .kv span:nth-child(odd) {
    color: var(--text-3);
  }
  pre {
    margin: 10px 0 0;
    padding: 10px;
    max-height: 260px;
    overflow: auto;
    background: var(--term-bg);
    color: var(--term-fg);
    border-radius: var(--radius-sm);
    user-select: text;
  }
</style>
