<script lang="ts">
  // Panel lateral para formularios (crear/editar).
  import { X } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import { fadeQuick, slideIn, slideOut } from "../lib/motion";

  let { title, onclose, children, footer }: { title: string; onclose: () => void; children: Snippet; footer?: Snippet } =
    $props();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="scrim" onclick={onclose} role="presentation" transition:fadeQuick|global></div>
<aside class="drawer card" in:slideIn|global out:slideOut|global>
  <header class="titlebar">
    <h2>{title}</h2>
    <button class="icon-btn" onclick={onclose} aria-label="Close"><X size={16} /></button>
  </header>
  <div class="body">{@render children()}</div>
  {#if footer}<footer>{@render footer()}</footer>{/if}
</aside>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    backdrop-filter: blur(2px);
    z-index: 40;
  }
  .drawer {
    position: fixed;
    top: 10px;
    right: 10px;
    bottom: 10px;
    width: min(520px, calc(100vw - 20px));
    z-index: 41;
    display: flex;
    flex-direction: column;
    background: var(--sidebar);
    box-shadow: var(--pop-shadow);
  }
  :global([data-theme="warp"]) .drawer,
  :global([data-theme="haze"]) .drawer {
    background: color-mix(in srgb, var(--bg) 82%, transparent);
  }
  header {
    display: flex;
    align-items: center;
    padding: 14px 16px;
    border-bottom: var(--border) solid var(--line);
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: 15px;
    font-weight: 650;
  }
  :global([data-theme="poolsuite"]) h2 {
    flex: none;
    background: var(--sidebar);
    padding: 0 10px;
    margin: 0 auto;
  }
  .body {
    flex: 1;
    overflow: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  footer {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    padding: 12px 16px;
    border-top: var(--border) solid var(--line);
  }
</style>
