<script lang="ts">
  import { api, MODE_LABEL } from "../lib/api";
  import { app, attempt } from "../lib/state.svelte";

  let deny = $state((app.snap?.denylist ?? []).join("\n"));
  let host = $state(app.snap?.hosts[0]?.name ?? "");
  let command = $state("");
  let result = $state<{ verdict: string; detail: string } | null>(null);

  const dirty = $derived(deny !== (app.snap?.denylist ?? []).join("\n"));

  async function save() {
    await attempt(() => api.denylistSave(deny.split("\n")), "Rules saved");
  }

  async function test() {
    if (!host || !command.trim()) return;
    try {
      result = await api.policyTest(host, command);
    } catch (e) {
      result = { verdict: "deny", detail: String(e) };
    }
  }
</script>

<div class="page">
  <div class="page-head">
    <h1>Rules</h1>
    <span class="sub">What an agent can run. Hosts and groups pick a mode; this list applies everywhere.</span>
  </div>

  <div class="cols">
    <div class="stack">
      <div class="card box">
        <div class="row">
          <h3 class="grow">Always forbidden</h3>
          <button class="btn sm ghost" onclick={() => (deny = app.snap?.default_deny.join("\n") ?? "")}>Reset to defaults</button>
          <button class="btn sm primary" disabled={!dirty} onclick={save}>Save</button>
        </div>
        <textarea class="mono" rows="18" bind:value={deny} spellcheck="false"></textarea>
        <div class="faint small">Checked on every part of the command: pipes, ;, &&, $( ), backticks, sh -c, sudo, xargs, find -exec.</div>
      </div>
    </div>

    <div class="stack">
      <div class="card box">
        <h3>Try a command</h3>
        <div class="row">
          <select bind:value={host} class="host">
            {#each app.snap?.hosts ?? [] as h}<option value={h.name}>{h.name}</option>{/each}
          </select>
          <input class="mono" placeholder="cat /etc/passwd | grep root" bind:value={command} onkeydown={(e) => e.key === "Enter" && test()} />
          <button class="btn" onclick={test}>Check</button>
        </div>
        {#if result}
          <div class="verdict {result.verdict}">
            <b>{result.verdict === "allow" ? "Runs" : result.verdict === "ask" ? "Asks you" : "Rejected"}</b>
            <span>{result.detail}</span>
          </div>
        {/if}
      </div>

      <div class="card box">
        <h3>Modes</h3>
        <div class="modes">
          <span class="chip">{MODE_LABEL.read_only}</span><span>Only commands in the allowed list; nothing that writes files.</span>
          <span class="chip">{MODE_LABEL.approval}</span><span>Every command waits for you in Approvals.</span>
          <span class="chip">{MODE_LABEL.free}</span><span>Anything not forbidden. For test machines.</span>
          <span class="chip">{MODE_LABEL.off}</span><span>The agent does not see the host.</span>
        </div>
        <div class="faint small">Set them per group (Hosts → group pencil) or per host. Default is Approval.</div>
      </div>

      <div class="card box">
        <h3>Rule syntax</h3>
        <div class="syntax mono">
          <code>shutdown</code><span>the program (globs work: <code>mkfs*</code>)</span>
          <code>systemctl reboot|halt</code><span>subcommands, in order; <code>a|b</code> means either</span>
          <code>rm -r|--recursive -f</code><span>flags anywhere; <code>-r</code> also matches <code>-rf</code></span>
          <code>dd of=/dev/*</code><span>any argument matching the pattern</span>
          <code>find !-delete</code><span><code>!</code> means it must not be there</span>
          <code>path:.ssh</code><span>any argument or redirect touching that path</span>
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .cols {
    display: grid;
    grid-template-columns: minmax(300px, 1fr) minmax(320px, 1.1fr);
    gap: 16px;
    align-items: start;
  }
  .box {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h3 {
    margin: 0;
    font-size: 14px;
  }
  .small {
    font-size: 12px;
  }
  .host {
    width: 150px;
    flex: none;
  }
  .verdict {
    display: flex;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--panel-2);
    border-left: 3px solid var(--ok);
    font-size: 12.5px;
  }
  .verdict.ask {
    border-left-color: var(--warn);
  }
  .verdict.deny {
    border-left-color: var(--danger);
  }
  .modes,
  .syntax {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 12px;
    align-items: center;
    font-size: 12.5px;
  }
  .syntax span {
    font-family: var(--font);
    color: var(--text-2);
  }
</style>
