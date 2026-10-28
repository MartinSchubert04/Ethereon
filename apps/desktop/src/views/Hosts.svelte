<script lang="ts">
  import { FileKey, FolderInput, Pencil, Plug, Plus, Search, Server, Upload } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import HostForm from "../components/HostForm.svelte";
  import GroupForm from "../components/GroupForm.svelte";
  import { api, MODE_LABEL, type Group, type Host, type ImportSummary } from "../lib/api";
  import { app, attempt, openTerminal, toast } from "../lib/state.svelte";
  import { testHost } from "../lib/hostkey.svelte";

  let query = $state("");
  let editing = $state<Host | null | undefined>(undefined); // undefined = cerrado, null = nuevo
  let editingGroup = $state<Group | null | undefined>(undefined);
  let importOpen = $state(false);

  const hosts = $derived(
    (app.snap?.hosts ?? []).filter((h) => {
      const q = query.toLowerCase();
      return !q || [h.name, h.hostname, h.description, ...h.tags].some((x) => x.toLowerCase().includes(q));
    }),
  );
  const groups = $derived.by(() => {
    const names = [...(app.snap?.groups ?? []).map((g) => g.name)];
    const out: { name: string | null; group: Group | null; hosts: Host[] }[] = names.map((n) => ({
      name: n,
      group: app.snap!.groups.find((g) => g.name === n)!,
      hosts: hosts.filter((h) => h.group === n),
    }));
    const loose = hosts.filter((h) => !h.group || !names.includes(h.group));
    if (loose.length || !out.length) out.push({ name: null, group: null, hosts: loose });
    return out;
  });

  function mode(h: Host) {
    const p = h.policy ?? app.snap?.groups.find((g) => g.name === h.group)?.policy;
    return p?.mode ?? "approval";
  }

  function report(s: ImportSummary) {
    toast(`Imported ${s.hosts} hosts and ${s.credentials} credentials${s.skipped ? `, ${s.skipped} already there` : ""}`);
    for (const n of s.notes.slice(0, 4)) toast(n);
  }

  async function importFrom(kind: "ssh" | "ssh-file" | "key" | "bitwarden") {
    importOpen = false;
    let path: string | null = null;
    if (kind !== "ssh") {
      const filters =
        kind === "bitwarden" ? [{ name: "Bitwarden JSON", extensions: ["json"] }] : undefined;
      path = (await open({ multiple: false, filters })) as string | null;
      if (!path) return;
    }
    try {
      const f =
        kind === "key" ? api.importKeyFile(path!) : kind === "bitwarden" ? api.importBitwarden(path!) : api.importSshConfig(path);
      report(await f);
    } catch (e) {
      toast(String(e), true);
    }
  }
</script>

<div class="page">
  <div class="page-head">
    <h1>Hosts</h1>
    <span class="sub">{app.snap?.hosts.length ?? 0} saved</span>
    <span class="grow"></span>
    <div class="search">
      <Search size={14} />
      <input placeholder="Search (Ctrl F)" bind:value={query} data-host-search />
    </div>
    <div class="menu-wrap">
      <button class="btn" onclick={() => (importOpen = !importOpen)}><Upload size={14} /> Import</button>
      {#if importOpen}
        <div class="menu card">
          <button onclick={() => importFrom("ssh")}><FolderInput size={14} /> ~/.ssh/config of this PC</button>
          <button onclick={() => importFrom("ssh-file")}><FolderInput size={14} /> SSH config file (Termius export)</button>
          <button onclick={() => importFrom("key")}><FileKey size={14} /> Private key file</button>
          <button onclick={() => importFrom("bitwarden")}><FileKey size={14} /> Bitwarden JSON</button>
        </div>
      {/if}
    </div>
    <button class="btn" onclick={() => (editingGroup = null)}>New group</button>
    <button class="btn primary" onclick={() => (editing = null)}><Plus size={15} /> New host</button>
  </div>

  {#if !app.snap?.hosts.length}
    <div class="empty">
      <Server size={30} />
      <h3>No hosts yet</h3>
      <div>Add one, or import your ~/.ssh/config.</div>
      <div class="row">
        <button class="btn" onclick={() => importFrom("ssh")}>Import ~/.ssh/config</button>
        <button class="btn primary" onclick={() => (editing = null)}>New host</button>
      </div>
    </div>
  {:else}
    {#each groups as g}
      {#if g.hosts.length || g.group}
        <div class="group-head">
          <span class="section-title">{g.name ?? "No group"}</span>
          {#if g.group}
            <span class="chip">{MODE_LABEL[g.group.policy.mode]}</span>
            <button class="icon-btn" aria-label="Edit group" onclick={() => (editingGroup = g.group)}><Pencil size={13} /></button>
          {/if}
        </div>
        <div class="grid">
          {#each g.hosts as h (h.name)}
            <div class="host card" role="button" tabindex="0" ondblclick={() => openTerminal(h.name)} onkeydown={(e) => e.key === "Enter" && openTerminal(h.name)}>
              <div class="top">
                <div class="badge {h.env}"><Server size={16} /></div>
                <div class="grow info">
                  <div class="name ellipsis">{h.name}</div>
                  <div class="addr mono ellipsis">{h.user}@{h.hostname}{h.port !== 22 ? `:${h.port}` : ""}</div>
                </div>
                <div class="actions">
                  <button class="icon-btn" title="Test connection" onclick={() => testHost(h.name)}><Plug size={14} /></button>
                  <button class="icon-btn" title="Edit" onclick={() => (editing = h)}><Pencil size={14} /></button>
                </div>
              </div>
              {#if h.description}<div class="desc faint">{h.description}</div>{/if}
              <div class="chips">
                <span class="chip {h.env}">{h.env}</span>
                <span class="chip">{MODE_LABEL[mode(h)]}</span>
                {#if h.jump.length}<span class="chip">via {h.jump.join(" → ")}</span>{/if}
                {#each h.tags as t}<span class="chip">{t}</span>{/each}
              </div>
              <button class="btn sm connect" onclick={() => openTerminal(h.name)}>Connect</button>
            </div>
          {/each}
        </div>
      {/if}
    {/each}
  {/if}
</div>

{#if editing !== undefined}
  <HostForm host={editing} onclose={() => (editing = undefined)} />
{/if}
{#if editingGroup !== undefined}
  <GroupForm group={editingGroup} onclose={() => (editingGroup = undefined)} />
{/if}

<svelte:window onclick={(e) => importOpen && !(e.target as HTMLElement).closest(".menu-wrap") && (importOpen = false)} />

<style>
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 10px;
    width: 200px;
    background: var(--panel-2);
    border: var(--border) solid var(--line);
    border-radius: var(--radius-sm);
    color: var(--text-3);
  }
  .search input {
    border: 0;
    background: none;
    padding: 6px 8px 6px 0;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    top: 38px;
    right: 0;
    z-index: 20;
    width: 260px;
    padding: 5px;
    background: var(--sidebar);
    box-shadow: var(--pop-shadow);
    display: flex;
    flex-direction: column;
  }
  .menu button {
    display: flex;
    gap: 9px;
    align-items: center;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    text-align: left;
  }
  .menu button:hover {
    background: var(--hover);
  }
  .group-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .group-head .section-title {
    margin: 18px 0 10px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
    align-items: start;
    gap: 12px;
  }
  .host {
    padding: 13px;
    display: flex;
    flex-direction: column;
    gap: 9px;
    cursor: default;
    transition: border-color 0.15s, transform 0.12s;
  }
  .host:hover {
    border-color: var(--line-strong);
  }
  :global([data-theme="poolsuite"]) .host:hover {
    transform: translate(-1px, -1px);
    box-shadow: 4px 4px 0 var(--line);
  }
  .top {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .badge {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    flex: none;
    background: color-mix(in srgb, var(--env-dev) 18%, transparent);
    color: var(--env-dev);
  }
  .badge.prod {
    background: color-mix(in srgb, var(--env-prod) 18%, transparent);
    color: var(--env-prod);
  }
  .badge.test {
    background: color-mix(in srgb, var(--env-test) 18%, transparent);
    color: var(--env-test);
  }
  .info {
    min-width: 0;
  }
  .name {
    font-weight: 620;
  }
  .addr {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .actions {
    display: flex;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .host:hover .actions,
  .host:focus-within .actions {
    opacity: 1;
  }
  .desc {
    font-size: 12px;
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .connect {
    align-self: flex-start;
  }
</style>
