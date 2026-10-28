<script lang="ts">
  import { Check, Copy, Plus } from "@lucide/svelte";
  import { api } from "../lib/api";
  import { app, attempt } from "../lib/state.svelte";

  let name = $state("Claude Code on this PC");
  let expires = $state<string>("90");
  let limitHosts = $state(false);
  let hosts = $state<string[]>([]);
  let created = $state<{ command: string; token: string } | null>(null);
  let copied = $state(false);

  const agentOn = $derived(app.snap?.settings.agent_enabled ?? false);

  async function create() {
    try {
      const r = await api.deviceCreate(name, expires === "never" ? null : Number(expires), limitHosts ? hosts : []);
      created = r;
      copied = false;
    } catch (e) {
      attempt(() => Promise.reject(e));
    }
  }

  async function copy() {
    if (!created) return;
    await navigator.clipboard.writeText(created.command);
    copied = true;
  }

  const fmt = (iso: string | null) => (iso ? new Date(iso).toLocaleDateString() : "never");
  const expired = (iso: string | null) => !!iso && Date.parse(iso) < Date.now();
</script>

<div class="page">
  <div class="page-head">
    <h1>Agents</h1>
    <span class="sub">Who can ask Ethereon to run commands.</span>
  </div>

  <div class="card kill" class:off={!agentOn}>
    <div class="grow">
      <h3>Agent access is {agentOn ? "on" : "off"}</h3>
      <div class="muted">
        {agentOn
          ? "Agents with a valid token can list hosts and run commands under the rules."
          : "Every agent request is refused, and anything waiting for approval was rejected."}
      </div>
    </div>
    <button class="btn {agentOn ? 'danger solid' : 'primary'}" onclick={() => attempt(() => api.agentSet(!agentOn))}>
      {agentOn ? "Turn off all access" : "Turn on"}
    </button>
  </div>

  <div class="section-title">Connect Claude Code</div>
  <div class="card box">
    {#if created}
      <div class="muted">Run this once in a terminal. The token is shown only now; Ethereon keeps only a hash of it.</div>
      <div class="cmd mono">{created.command}</div>
      <div class="row">
        <button class="btn primary" onclick={copy}>{#if copied}<Check size={14} /> Copied{:else}<Copy size={14} /> Copy command{/if}</button>
        <button class="btn" onclick={() => (created = null)}>Done</button>
      </div>
    {:else}
      <div class="grid2">
        <label class="field">Name<input bind:value={name} /></label>
        <label class="field">Expires
          <select bind:value={expires}>
            <option value="7">In 7 days</option>
            <option value="30">In 30 days</option>
            <option value="90">In 90 days</option>
            <option value="365">In a year</option>
            <option value="never">Never</option>
          </select>
        </label>
      </div>
      <label class="row"><input type="checkbox" bind:checked={limitHosts} /> Only some hosts</label>
      {#if limitHosts}
        <div class="hosts">
          {#each app.snap?.hosts ?? [] as h}
            <label class="row"><input type="checkbox" value={h.name} bind:group={hosts} /> {h.name}</label>
          {/each}
        </div>
      {/if}
      <div><button class="btn primary" onclick={create}><Plus size={15} /> Create token</button></div>
    {/if}
    <div class="faint small">
      Endpoint: <code>{app.status?.mcp_url}</code>
      {#if app.status?.mcp_error}<span class="bad"> · {app.status.mcp_error}</span>{/if}
    </div>
  </div>

  <div class="section-title">Devices</div>
  {#if app.snap?.devices.length}
    <div class="card">
      <table class="list">
        <thead><tr><th>Name</th><th>Created</th><th>Expires</th><th>Hosts</th><th>Status</th><th></th></tr></thead>
        <tbody>
          {#each app.snap.devices as d (d.id)}
            <tr>
              <td>{d.name}</td>
              <td class="muted">{fmt(d.created)}</td>
              <td class="muted">{fmt(d.expires)}</td>
              <td class="muted">{d.hosts.length ? d.hosts.join(", ") : "all"}</td>
              <td>
                {#if d.revoked}<span class="chip bad">revoked</span>
                {:else if expired(d.expires)}<span class="chip warn">expired</span>
                {:else}<span class="chip ok">active</span>{/if}
              </td>
              <td class="right">
                {#if !d.revoked}
                  <button class="btn sm danger" onclick={() => attempt(() => api.deviceRevoke(d.id), "Revoked")}>Revoke</button>
                {:else}
                  <button class="btn sm ghost" onclick={() => attempt(() => api.deviceDelete(d.id))}>Remove</button>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <div class="faint">No devices yet.</div>
  {/if}
</div>

<style>
  .kill {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px 18px;
    border-left: 4px solid var(--ok);
  }
  .kill.off {
    border-left-color: var(--danger);
  }
  .kill h3 {
    margin: 0 0 4px;
    font-size: 15px;
  }
  .box {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .cmd {
    padding: 12px;
    background: var(--term-bg);
    color: var(--term-fg);
    border-radius: var(--radius-sm);
    border: var(--border) solid var(--line);
    user-select: all;
    word-break: break-all;
  }
  .hosts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 6px;
  }
  .small {
    font-size: 12px;
  }
  .bad {
    color: var(--danger);
  }
  .right {
    text-align: right;
  }
</style>
