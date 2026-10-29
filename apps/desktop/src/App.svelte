<script lang="ts">
  import {
    Bot,
    Inbox,
    KeyRound,
    Lock,
    Power,
    ScrollText,
    Server,
    Settings as SettingsIcon,
    ShieldCheck,
    TriangleAlert,
    X,
  } from "@lucide/svelte";
  import { api } from "./lib/api";
  import { app, attempt, start, type View } from "./lib/state.svelte";
  import { installShortcuts } from "./lib/shortcuts";
  import { confirmKey, prompt } from "./lib/hostkey.svelte";
  import HostKeyPrompt from "./components/HostKeyPrompt.svelte";
  import Unlock from "./views/Unlock.svelte";
  import Hosts from "./views/Hosts.svelte";
  import Keychain from "./views/Keychain.svelte";
  import Rules from "./views/Rules.svelte";
  import Approvals from "./views/Approvals.svelte";
  import Agents from "./views/Agents.svelte";
  import Audit from "./views/Audit.svelte";
  import Settings from "./views/Settings.svelte";
  import SftpPane from "./components/SftpPane.svelte";
  import TilePane from "./components/TilePane.svelte";
  import WorkspacePane from "./components/WorkspacePane.svelte";
  import { initWindows } from "./lib/windows";
  import Titlebar from "./components/Titlebar.svelte";
  import QuickOpen from "./components/QuickOpen.svelte";
  import Confirm from "./components/Confirm.svelte";
  import { confirmState } from "./lib/confirm.svelte";
  import { flip } from "svelte/animate";
  import { fade } from "svelte/transition";
  import { flipMs, rise, toastIn } from "./lib/motion";

  start();
  installShortcuts();
  // Pestañas que vuelven de otras ventanas.
  initWindows();

  const nav: { id: View; label: string; icon: typeof Server }[] = [
    { id: "hosts", label: "Hosts", icon: Server },
    { id: "keychain", label: "Keychain", icon: KeyRound },
    { id: "rules", label: "Rules", icon: ShieldCheck },
    { id: "approvals", label: "Approvals", icon: Inbox },
    { id: "agents", label: "Agents", icon: Bot },
    { id: "audit", label: "Audit", icon: ScrollText },
    { id: "settings", label: "Settings", icon: SettingsIcon },
  ];

  const agentOn = $derived(app.snap?.settings.agent_enabled ?? false);
  const overlay = $derived(app.sidebarOverlay && app.view === "terminal");

  // Elegir una seccion desde el menu flotante lleva al vault y lo cierra.
  function go(v: View) {
    app.sidebarOverlay = false;
    app.view = v;
  }

  // Cualquier cambio de vista cierra el menu flotante.
  $effect(() => {
    app.view;
    app.sidebarOverlay = false;
  });

  function outside(e: PointerEvent) {
    const t = e.target as HTMLElement;
    if (overlay && !t.closest(".sidebar") && !t.closest(".ham")) app.sidebarOverlay = false;
  }
</script>

<div class="frame">
{#if !app.status}
  <Titlebar locked />
{:else if !app.status.unlocked}
  <Titlebar locked />
  <div class="fill"><Unlock /></div>
{:else}
  <Titlebar />
  <!-- Como en Termius: la barra lateral es de la pestaña Vault; las terminales usan todo el ancho. -->
  <div class="shell" class:collapsed={app.sidebarCollapsed && !overlay} class:full={app.view === "terminal"} class:overlay>
    <aside class="sidebar">
      <nav>
        {#each nav as n}
          <button class="nav" class:on={app.view === n.id} onclick={() => go(n.id)} title={n.label}>
            <n.icon size={16} />
            <span class="grow label">{n.label}</span>
            {#if n.id === "approvals" && app.approvals.length}
              <span class="badge">{app.approvals.length}</span>
            {/if}
          </button>
        {/each}
      </nav>

      <div class="grow"></div>

      <button
        class="agent card"
        class:off={!agentOn}
        onclick={() => attempt(() => api.agentSet(!agentOn))}
        title="Turns off all agent access at once"
      >
        <Power size={15} />
        <span class="grow label">Agent access</span>
        <span class="state label">{agentOn ? "On" : "Off"}</span>
      </button>
      <button class="nav" onclick={() => api.lock()} title="Lock vault (Ctrl Shift L)">
        <Lock size={16} /><span class="label">Lock vault</span>
      </button>
    </aside>

    <main>
      {#each app.alerts as a (a.id)}
        {#if a.error.kind === "host_key_unknown" || a.error.kind === "host_key_changed"}
          {@const err = a.error}
          <div class="alert" class:bad={err.kind === "host_key_changed"}>
            <TriangleAlert size={15} />
            <span class="grow">
              {#if err.kind === "host_key_changed"}
                Host key changed for <b>{err.host}</b> ({err.addr}). Connections are blocked.
              {:else}
                An agent tried <b>{err.host}</b> but its host key ({err.addr}) is not confirmed yet.
              {/if}
            </span>
            <button class="btn sm" onclick={() => confirmKey(err)}>Review</button>
            <button class="icon-btn" aria-label="Dismiss" onclick={() => api.alertDismiss(a.id)}><X size={14} /></button>
          </div>
        {/if}
      {/each}

      <div class="view">
        {#key app.view}
        <div class="view-page" in:rise={{ y: 6 }}>
        {#if app.view === "hosts"}<Hosts />
        {:else if app.view === "keychain"}<Keychain />
        {:else if app.view === "rules"}<Rules />
        {:else if app.view === "approvals"}<Approvals />
        {:else if app.view === "agents"}<Agents />
        {:else if app.view === "audit"}<Audit />
        {:else if app.view === "settings"}<Settings />
        {/if}
        </div>
        {/key}
        <!-- Las terminales quedan montadas para no perder la sesion al cambiar de vista. -->
        {#each app.terms as t (t.key)}
          <div class="term-wrap" class:shown={app.view === "terminal" && app.activeTerm === t.key}>
            {#if t.members && !t.layout}
              <WorkspacePane tab={t} active={app.view === "terminal" && app.activeTerm === t.key} />
            {:else if t.sftp}
              <SftpPane tab={t} active={app.view === "terminal" && app.activeTerm === t.key} />
            {:else}
              <TilePane tab={t} active={app.view === "terminal" && app.activeTerm === t.key} />
            {/if}
          </div>
        {/each}
      </div>
    </main>
  </div>
{/if}
</div>

{#if app.quickOpen && app.status?.unlocked}
  <QuickOpen />
{/if}

<svelte:window onpointerdowncapture={outside} onkeydown={(e) => overlay && e.key === "Escape" && (app.sidebarOverlay = false)} />

{#if confirmState.open}
  <Confirm />
{/if}

{#if prompt.error}
  <HostKeyPrompt error={prompt.error} ontrust={() => prompt.resolve?.(true)} oncancel={() => prompt.resolve?.(false)} />
{/if}

<div class="toasts">
  {#each app.toasts as t (t.id)}
    <div class="toast card" class:bad={t.bad} in:toastIn out:fade={{ duration: 150 }} animate:flip={{ duration: flipMs() }}>{t.text}</div>
  {/each}
</div>

<style>
  .frame {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .fill {
    flex: 1;
    min-height: 0;
  }
  .shell {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 216px 1fr;
  }
  .shell.collapsed {
    grid-template-columns: 58px 1fr;
  }
  .shell.collapsed .label,
  .shell.collapsed .badge {
    display: none;
  }
  .shell.collapsed .nav,
  .shell.collapsed .agent {
    justify-content: center;
    padding: 0;
  }
  .shell.collapsed .agent {
    height: 34px;
  }
  .shell.full {
    grid-template-columns: 0 1fr;
  }
  .shell.full .sidebar {
    visibility: hidden;
    border: 0;
    padding: 0;
  }
  /* Menu flotante sobre la terminal. */
  .shell {
    position: relative;
  }
  .shell.full.overlay .sidebar {
    visibility: visible;
    position: absolute;
    z-index: 30;
    top: 0;
    bottom: 0;
    left: 0;
    width: 216px;
    padding: 12px 10px;
    border-right: var(--border) solid var(--line-strong);
    background: var(--sidebar);
    box-shadow: 12px 0 40px -12px rgba(0, 0, 0, 0.6);
    animation: menu-in 0.22s cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  :global([data-theme="warp"]) .shell.full.overlay .sidebar,
  :global([data-theme="haze"]) .shell.full.overlay .sidebar {
    background: color-mix(in srgb, var(--bg) 90%, transparent);
  }
  @keyframes menu-in {
    from {
      transform: translateX(-24px);
      opacity: 0;
    }
  }
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 10px 12px;
    background: var(--sidebar);
    backdrop-filter: var(--blur);
    border-right: var(--border) solid var(--line);
    min-height: 0;
    overflow: auto;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    color: var(--text-2);
    font-weight: 520;
    cursor: pointer;
    text-align: left;
  }
  .nav:hover {
    background: var(--hover);
    color: var(--text);
  }
  .nav.on {
    background: var(--panel-2);
    color: var(--text);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  :global([data-theme="poolsuite"]) .nav.on {
    background: var(--accent);
    color: var(--accent-ink);
    box-shadow: 2px 2px 0 var(--line);
    border: 2px solid var(--line);
  }
  .badge {
    min-width: 19px;
    height: 19px;
    padding: 0 5px;
    border-radius: 999px;
    background: var(--warn);
    color: #1a1a1a;
    font-size: 11px;
    font-weight: 700;
    display: grid;
    place-items: center;
  }
  .agent {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 9px 10px;
    margin-bottom: 4px;
    color: var(--ok);
    font-weight: 600;
  }
  .agent .state {
    font-size: 12px;
  }
  .agent.off {
    color: var(--danger);
  }
  main {
    /* Fija: con el menu flotante la barra lateral sale de la grilla y main se correria a
       la primera columna (que en una terminal mide 0). */
    grid-column: 2;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .view {
    flex: 1;
    min-height: 0;
    position: relative;
  }
  .view-page {
    position: absolute;
    inset: 0;
  }
  .term-wrap {
    position: absolute;
    inset: 0;
    visibility: hidden;
  }
  .term-wrap.shown {
    visibility: visible;
  }
  .alert {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    background: color-mix(in srgb, var(--warn) 14%, var(--panel));
    border-bottom: var(--border) solid var(--line);
    color: var(--text);
    font-size: 12.5px;
  }
  .alert.bad {
    background: color-mix(in srgb, var(--danger) 18%, var(--panel));
  }
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 80;
    max-width: 420px;
  }
  .toast {
    padding: 10px 14px;
    background: var(--sidebar);
    box-shadow: var(--pop-shadow);
    font-size: 12.5px;
    user-select: text;
  }
  .toast.bad {
    border-color: var(--danger);
    color: var(--danger);
  }
</style>
