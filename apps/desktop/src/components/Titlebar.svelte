<script lang="ts">
  // Barra de arriba al estilo Termius: es la barra de titulo de la ventana (no hay marco
  // de Windows) y a la vez la tira de pestañas. Se arrastra desde cualquier hueco y doble
  // clic maximiza (data-tauri-drag-region).
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { ArrowDownToLine, Bell, Copy, Menu, Minus, Plus, Square, SquareTerminal, X } from "@lucide/svelte";
  import { api } from "../lib/api";
  import { onMount } from "svelte";
  import { app, closeTerminal, goToTab, toggleSidebar } from "../lib/state.svelte";
  import { pop } from "../lib/motion";

  let { locked = false }: { locked?: boolean } = $props();

  const win = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    const sync = async () => (maximized = await win.isMaximized());
    sync();
    const un = win.onResized(sync);
    return () => un.then((f) => f());
  });

  const onTerm = $derived(app.view === "terminal");
</script>

<header class="bar" data-tauri-drag-region>
  {#if locked}
    <div class="brand" data-tauri-drag-region>
      <img src="/icon.svg" alt="" width="18" height="18" data-tauri-drag-region />
      <span data-tauri-drag-region>Ethereon</span>
    </div>
  {:else}
    <button class="ham" title="Sidebar (Ctrl B)" onclick={toggleSidebar}><Menu size={17} /></button>

    <div class="tabs" data-tauri-drag-region>
      <button class="tab vault" class:on={!onTerm} title="Vault (Ctrl 1)" onclick={() => goToTab(0)}>
        <img src="/icon.svg" alt="" width="16" height="16" />
        <span>Vault</span>
      </button>
      {#each app.terms as t, i (t.key)}
        <div
          in:pop={{ start: 0.85 }}
          class="tab"
          class:on={onTerm && app.activeTerm === t.key}
          role="button"
          tabindex="0"
          title="{t.title} (Ctrl {i + 2})"
          onclick={() => goToTab(i + 1)}
          onkeydown={(e) => e.key === "Enter" && goToTab(i + 1)}
          onauxclick={(e) => e.button === 1 && closeTerminal(t.key)}
        >
          <span class="dot" class:dead={!t.alive}><SquareTerminal size={14} /></span>
          <span class="label ellipsis">{t.title}</span>
          <button class="x" aria-label="Close tab" title="Close (Ctrl W)" onclick={(e) => (e.stopPropagation(), closeTerminal(t.key))}>
            <X size={12} />
          </button>
        </div>
      {/each}
      <button class="tab new" title="New tab (Ctrl T)" onclick={() => (app.quickOpen = true)}>
        <Plus size={15} /><span>New tab</span>
      </button>
    </div>
  {/if}

  <div class="drag" data-tauri-drag-region></div>

  {#if app.update?.status === "ready"}
    <button
      class="update"
      title="Ethereon {app.update.version}: restarts and locks the vault. Installs by itself when nobody is using it."
      onclick={() => api.updateNow()}
    >
      <ArrowDownToLine size={13} /> Update
    </button>
  {/if}
  {#if !locked && app.approvals.length}
    <button class="pending" onclick={() => (goToTab(0), (app.view = "approvals"))} title="Commands waiting for approval">
      <Bell size={13} /> {app.approvals.length}
    </button>
  {/if}
  <div class="controls">
    <button class="win" title="Minimize" onclick={() => win.minimize()}><Minus size={15} /></button>
    <button class="win" title={maximized ? "Restore" : "Maximize"} onclick={() => win.toggleMaximize()}>
      {#if maximized}<Copy size={12} />{:else}<Square size={12} />{/if}
    </button>
    <button class="win close" title="Close to tray" onclick={() => win.close()}><X size={16} /></button>
  </div>
</header>

<style>
  .bar {
    height: 46px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 6px;
    background: var(--titlebar-bg, color-mix(in srgb, var(--bg) 82%, #000));
    border-bottom: var(--border) solid var(--line);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 0 10px;
    font-weight: 700;
    font-size: 13.5px;
  }
  .ham {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: var(--radius-sm);
    color: var(--text-2);
  }
  .ham:hover {
    background: var(--hover);
    color: var(--text);
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 10px 0 12px;
    border-radius: var(--radius-sm);
    background: var(--panel);
    border: var(--border) solid transparent;
    color: var(--text-2);
    font-weight: 560;
    font-size: 13px;
    white-space: nowrap;
    cursor: pointer;
    min-width: 0;
    max-width: 200px;
    transition: background 0.12s, color 0.12s;
  }
  .tab:hover {
    color: var(--text);
    background: var(--panel-2);
  }
  .tab.on {
    background: var(--panel-2);
    color: var(--text);
    border-color: var(--line-strong);
  }
  .tab.vault {
    padding-right: 14px;
    flex: none;
  }
  .tab.new {
    background: none;
    color: var(--text-3);
    padding: 0 10px;
    flex: none;
  }
  .tab.new:hover {
    color: var(--text);
    background: var(--hover);
  }
  .label {
    min-width: 0;
  }
  .dot {
    display: grid;
    color: var(--accent);
    flex: none;
  }
  /* Sobre una pestaña activa pintada con el acento, el icono va en el color de tinta. */
  :global([data-theme="athanor"]) .tab.on .dot,
  :global([data-theme="poolsuite"]) .tab.on .dot {
    color: var(--accent-ink);
  }
  .dot.dead {
    color: var(--text-3);
  }
  .x {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 4px;
    flex: none;
    opacity: 0.55;
  }
  .x:hover {
    opacity: 1;
    background: var(--hover);
  }
  .drag {
    flex: 1;
    align-self: stretch;
  }
  .pending {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 700;
    color: #1a1a1a;
    background: var(--warn);
  }
  .update {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 11px;
    border-radius: var(--radius-sm);
    font-size: 12px;
    font-weight: 650;
    color: var(--text);
    background: var(--panel-2);
    border: var(--border) solid var(--line-strong);
  }
  .update:hover {
    border-color: var(--accent);
  }
  .controls {
    display: flex;
    align-self: stretch;
  }
  .win {
    width: 46px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    transition: background 0.12s, color 0.12s;
  }
  .win:hover {
    background: var(--hover);
    color: var(--text);
  }
  .win.close:hover {
    background: #c42b1c;
    color: #fff;
  }

  /* Poolsuite: barra de ventana del Mac clasico (rayas) y pestañas con borde negro. */
  :global([data-theme="poolsuite"]) .bar {
    background: var(--sidebar);
  }
  :global([data-theme="poolsuite"]) .drag {
    background: var(--titlebar);
    background-size: 100% 13px;
    background-repeat: no-repeat;
    background-position: center;
    margin: 0 8px;
  }
  :global([data-theme="poolsuite"]) .tab:not(.new) {
    border-color: var(--line);
  }
  :global([data-theme="poolsuite"]) .tab.on {
    background: var(--accent);
    color: var(--accent-ink);
    box-shadow: 2px 2px 0 var(--line);
  }
</style>
