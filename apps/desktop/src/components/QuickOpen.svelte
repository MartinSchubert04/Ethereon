<script lang="ts" module>
  import type { LocalShell as LS } from "../lib/api";
  let cache: LS[] | null = null;
</script>

<script lang="ts">
  // "New tab": buscar un host (o una shell de esta PC) y abrir su terminal. Flechas para elegir, Enter para abrir.
  import { Search } from "@lucide/svelte";
  import { api, userOf, type LocalShell } from "../lib/api";
  import Logo from "./Logo.svelte";
  import { hostIcon } from "../lib/icons";
  import { prefs } from "../lib/prefs.svelte";
  import { app, openLocal, openSftp, openTerminal } from "../lib/state.svelte";
  import { fadeQuick, pop } from "../lib/motion";

  let query = $state("");
  let sel = $state(0);
  let input: HTMLInputElement;

  // Las shells se piden una vez por sesion de la app.
  let shells = $state<LocalShell[]>(cache ?? []);
  if (!cache) api.localShells().then((l) => (shells = cache = l)).catch(() => {});

  type Item = { kind: "local"; shell: LocalShell } | { kind: "host"; host: NonNullable<typeof app.snap>["hosts"][number] };
  const list = $derived.by((): Item[] => {
    const q = query.toLowerCase();
    const has = (...xs: string[]) => !q || xs.some((x) => x.toLowerCase().includes(q));
    return [
      ...shells.filter((sh) => has(sh.label, sh.id, "local")).map((shell) => ({ kind: "local" as const, shell })),
      ...(app.snap?.hosts ?? [])
        .filter((h) => has(h.name, h.hostname, h.group ?? "", h.description, ...h.tags))
        .map((host) => ({ kind: "host" as const, host })),
    ];
  });

  $effect(() => {
    input?.focus();
  });

  function close() {
    app.quickOpen = false;
  }

  /** Enter abre la terminal; Shift Enter, los archivos (SFTP) del host. */
  function pick(it: Item, files = false) {
    close();
    if (it.kind === "local") openLocal(it.shell.id);
    else if (files) openSftp(it.host.name);
    else openTerminal(it.host.name);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
    else if (e.key === "ArrowDown") sel = Math.min(sel + 1, list.length - 1);
    else if (e.key === "ArrowUp") sel = Math.max(sel - 1, 0);
    else if (e.key === "Enter" && list[sel]) pick(list[sel], e.shiftKey);
    else return;
    e.preventDefault();
  }
</script>

<div class="scrim" role="presentation" onclick={close} transition:fadeQuick|global></div>
<div class="center"><div class="box card" role="dialog" transition:pop|global>
  <div class="search inbox bare">
    <Search size={15} />
    <input
      bind:this={input}
      bind:value={query}
      oninput={() => (sel = 0)}
      onkeydown={onKey}
      placeholder="Connect to a host or open a local shell..."
    />
    <kbd>Esc</kbd>
  </div>
  <div class="tip">Enter opens a terminal · Shift Enter opens the files (SFTP)</div>
  <div class="list">
    {#each list as it, i (it.kind + (it.kind === "local" ? it.shell.id : it.host.name))}
      {#if it.kind === "local"}
        <button class="item" class:on={i === sel} onmouseenter={() => (sel = i)} onclick={() => pick(it)}>
          <span class="icon local"><Logo id={prefs.term.shellIcons[it.shell.id] ?? "terminal"} size={14} /></span>
          <span class="name">{it.shell.label}</span>
          <span class="grow"></span>
          <span class="chip">this PC</span>
        </button>
      {:else}
      {@const h = it.host}
      <button class="item" class:on={i === sel} onmouseenter={() => (sel = i)} onclick={(e) => pick(it, e.shiftKey)}>
        <span class="icon {h.env}"><Logo id={hostIcon(h)} size={14} /></span>
        <span class="name">{h.name}</span>
        <span class="addr mono ellipsis">{userOf(h, app.snap?.credentials)}@{h.hostname}</span>
        <span class="grow"></span>
        {#if h.group}<span class="chip">{h.group}</span>{/if}
        {#if h.env !== "none"}<span class="chip {h.env}">{h.env}</span>{/if}
      </button>
      {/if}
    {:else}
      <div class="nothing faint">Nothing matches.</div>
    {/each}
  </div>
</div></div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    z-index: 70;
  }
  .center {
    position: fixed;
    inset: 76px 0 auto;
    z-index: 71;
    display: flex;
    justify-content: center;
    pointer-events: none;
  }
  .box {
    pointer-events: auto;
    transform-origin: top center;
    width: min(560px, calc(100vw - 32px));
    background: var(--sidebar);
    box-shadow: var(--pop-shadow);
    overflow: hidden;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    color: var(--text-3);
    border-bottom: var(--border) solid var(--line);
  }
  .search input {
    border: 0;
    background: none;
    padding: 14px 0;
    font-size: 14px;
  }

  kbd {
    font: inherit;
    font-size: 11px;
    border: 1px solid var(--line);
    border-radius: 5px;
    padding: 1px 6px;
  }
  .tip {
    padding: 6px 14px;
    font-size: 11px;
    color: var(--text-3);
    border-bottom: var(--border) solid var(--line);
  }
  .list {
    max-height: 360px;
    overflow: auto;
    padding: 6px;
  }
  .item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    text-align: left;
    transition: background 0.08s;
  }
  .item.on {
    background: var(--panel-2);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 6px;
    color: var(--env-dev);
    background: color-mix(in srgb, var(--env-dev) 16%, transparent);
  }
  .icon.local {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  /* Sin ambiente: neutro, con el color del texto. */
  .icon.none {
    background: var(--panel-2);
    color: var(--text-2);
  }
  .icon.prod {
    color: var(--env-prod);
    background: color-mix(in srgb, var(--env-prod) 16%, transparent);
  }
  .icon.test {
    color: var(--env-test);
    background: color-mix(in srgb, var(--env-test) 16%, transparent);
  }
  .name {
    font-weight: 600;
  }
  .addr {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .nothing {
    padding: 14px;
  }
</style>
