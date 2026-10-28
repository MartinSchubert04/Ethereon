<script lang="ts">
  // "New tab": buscar un host y abrir su terminal. Flechas para elegir, Enter para abrir.
  import { Search, Server } from "@lucide/svelte";
  import { app, openTerminal } from "../lib/state.svelte";
  import { fadeQuick, pop } from "../lib/motion";

  let query = $state("");
  let sel = $state(0);
  let input: HTMLInputElement;

  const list = $derived(
    (app.snap?.hosts ?? []).filter((h) => {
      const q = query.toLowerCase();
      return !q || [h.name, h.hostname, h.group ?? "", h.description, ...h.tags].some((x) => x.toLowerCase().includes(q));
    }),
  );

  $effect(() => {
    input?.focus();
  });

  function close() {
    app.quickOpen = false;
  }

  function pick(name: string) {
    close();
    openTerminal(name);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
    else if (e.key === "ArrowDown") sel = Math.min(sel + 1, list.length - 1);
    else if (e.key === "ArrowUp") sel = Math.max(sel - 1, 0);
    else if (e.key === "Enter" && list[sel]) pick(list[sel].name);
    else return;
    e.preventDefault();
  }
</script>

<div class="scrim" role="presentation" onclick={close} transition:fadeQuick|global></div>
<div class="center"><div class="box card" role="dialog" transition:pop|global>
  <div class="search">
    <Search size={15} />
    <input
      bind:this={input}
      bind:value={query}
      oninput={() => (sel = 0)}
      onkeydown={onKey}
      placeholder="Connect to a host..."
    />
    <kbd>Esc</kbd>
  </div>
  <div class="list">
    {#each list as h, i (h.name)}
      <button class="item" class:on={i === sel} onmouseenter={() => (sel = i)} onclick={() => pick(h.name)}>
        <span class="icon {h.env}"><Server size={14} /></span>
        <span class="name">{h.name}</span>
        <span class="addr mono ellipsis">{h.user}@{h.hostname}</span>
        <span class="grow"></span>
        {#if h.group}<span class="chip">{h.group}</span>{/if}
        <span class="chip {h.env}">{h.env}</span>
      </button>
    {:else}
      <div class="none faint">No hosts match.</div>
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
  .none {
    padding: 14px;
  }
</style>
