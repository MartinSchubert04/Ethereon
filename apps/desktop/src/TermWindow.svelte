<script lang="ts">
  // Ventana de terminales sacada de la principal: la barra con sus pestañas y las
  // terminales, sin vault. Las pestañas se pueden pasar a otra ventana o volver a la principal.
  import { app, start } from "./lib/state.svelte";
  import { installShortcuts } from "./lib/shortcuts";
  import { initWindows } from "./lib/windows";
  import { prompt } from "./lib/hostkey.svelte";
  import { confirmState } from "./lib/confirm.svelte";
  import Titlebar from "./components/Titlebar.svelte";
  import TilePane from "./components/TilePane.svelte";
  import SftpPane from "./components/SftpPane.svelte";
  import WorkspacePane from "./components/WorkspacePane.svelte";
  import HostKeyPrompt from "./components/HostKeyPrompt.svelte";
  import Confirm from "./components/Confirm.svelte";
  import { toastIn } from "./lib/motion";
  import { fade } from "svelte/transition";

  app.view = "terminal";
  start();
  installShortcuts();
  initWindows();
</script>

<div class="frame">
  <Titlebar />
  <div class="view">
    {#each app.terms as t (t.key)}
      <div class="term-wrap" class:shown={app.activeTerm === t.key}>
        {#if t.members && !t.layout}
          <WorkspacePane tab={t} active={app.activeTerm === t.key} />
        {:else if t.sftp}
          <SftpPane tab={t} active={app.activeTerm === t.key} />
        {:else}
          <TilePane tab={t} active={app.activeTerm === t.key} />
        {/if}
      </div>
    {/each}
  </div>
</div>

{#if confirmState.open}<Confirm />{/if}
{#if prompt.error}
  <HostKeyPrompt error={prompt.error} ontrust={() => prompt.resolve?.(true)} oncancel={() => prompt.resolve?.(false)} />
{/if}

<div class="toasts">
  {#each app.toasts as t (t.id)}
    <div class="toast card" class:bad={t.bad} in:toastIn out:fade={{ duration: 150 }}>{t.text}</div>
  {/each}
</div>

<style>
  .frame {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .view {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .term-wrap {
    position: absolute;
    inset: 0;
    visibility: hidden;
  }
  .term-wrap.shown {
    visibility: visible;
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
  }
  .toast.bad {
    border-color: var(--danger);
    color: var(--danger);
  }
</style>
