<script lang="ts">
  import { untrack } from "svelte";
  // El formulario trabaja sobre una copia: se monta de nuevo cada vez que se abre.
  import Drawer from "./Drawer.svelte";
  import PolicyEditor from "./PolicyEditor.svelte";
  import { api, defaultPolicy, newHost, type Env, type Host } from "../lib/api";
  import { app, attempt } from "../lib/state.svelte";

  let { host, onclose }: { host: Host | null; onclose: () => void } = $props();
  const init = untrack(() => host);

  // Copia editable; `original` es el nombre con el que estaba guardado.
  const original = init?.name ?? null;
  let h = $state<Host>(init ? structuredClone($state.snapshot(init)) : newHost());
  let tags = $state(h.tags.join(", "));
  let jump = $state(h.jump.join(", "));
  let ownRules = $state(h.policy !== null);
  let policy = $state(h.policy ?? defaultPolicy());


  const loginCreds = $derived(
    (app.snap?.credentials ?? []).filter((c) => ["password", "private_key", "certificate"].includes(c.kind)),
  );
  const sudoCreds = $derived((app.snap?.credentials ?? []).filter((c) => c.kind === "sudo_password" || c.kind === "password"));
  const others = $derived((app.snap?.hosts ?? []).filter((x) => x.name !== original).map((x) => x.name));
  const envs: Env[] = ["prod", "test", "dev"];

  const split = (s: string) =>
    s
      .split(",")
      .map((x) => x.trim())
      .filter(Boolean);

  async function save() {
    const out: Host = {
      ...h,
      port: Number(h.port) || 22,
      tags: split(tags),
      jump: split(jump),
      group: h.group || null,
      sudo_credential: h.sudo_credential || null,
      policy: ownRules ? policy : null,
    };
    if (await attempt(() => api.hostSave(out, original), "Host saved")) onclose();
  }

  async function remove() {
    if (original && confirm(`Delete ${original}?`)) {
      if (await attempt(() => api.hostDelete(original), "Host deleted")) onclose();
    }
  }
</script>

<Drawer title={original ? `Edit ${original}` : "New host"} {onclose}>
  <div class="grid2">
    <label class="field">Name <span class="hint">What the agent calls it</span><input bind:value={h.name} placeholder="web-1" /></label>
    <label class="field">Group
      <select bind:value={h.group}>
        <option value={null}>No group</option>
        {#each app.snap?.groups ?? [] as g}<option value={g.name}>{g.name}</option>{/each}
      </select>
    </label>
  </div>
  <div class="addr">
    <label class="field">Hostname or IP<input bind:value={h.hostname} placeholder="10.0.0.5" /></label>
    <label class="field">Port<input type="number" bind:value={h.port} /></label>
  </div>
  <div class="grid2">
    <label class="field">User<input bind:value={h.user} placeholder="deploy" /></label>
    <label class="field">Credential
      <select bind:value={h.credential}>
        <option value="">Pick one</option>
        {#each loginCreds as c}<option value={c.id}>{c.id}</option>{/each}
      </select>
    </label>
  </div>
  <div class="grid2">
    <label class="field">Sudo password
      <select bind:value={h.sudo_credential}>
        <option value={null}>None</option>
        {#each sudoCreds as c}<option value={c.id}>{c.id}</option>{/each}
      </select>
    </label>
    <label class="field">Environment
      <div class="seg">
        {#each envs as e}<button class:on={h.env === e} onclick={() => (h.env = e)}>{e}</button>{/each}
      </div>
    </label>
  </div>
  <label class="field">Jump through <span class="hint">Host names in order, comma separated ({others.join(", ") || "none yet"})</span>
    <input bind:value={jump} placeholder="bastion" />
  </label>
  <label class="field">Tags <span class="hint">Comma separated</span><input bind:value={tags} placeholder="nginx, docker" /></label>
  <label class="field">Description <span class="hint">The agent reads this: what the host is for, what runs there</span>
    <textarea bind:value={h.description} rows="3" placeholder="Public web server. Nginx in front of the app in Docker."></textarea>
  </label>

  <div class="section-title">Agent rules</div>
  <label class="row"><input type="checkbox" bind:checked={ownRules} /> Own rules for this host {#if !ownRules}<span class="faint">(uses the group's, or Approval)</span>{/if}</label>
  {#if ownRules}<PolicyEditor bind:policy />{/if}

  {#snippet footer()}
    {#if original}<button class="btn danger" onclick={remove}>Delete</button>{/if}
    <span class="grow"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" onclick={save}>Save</button>
  {/snippet}
</Drawer>

<style>
  .addr {
    display: grid;
    grid-template-columns: 1fr 90px;
    gap: 12px;
  }
</style>
