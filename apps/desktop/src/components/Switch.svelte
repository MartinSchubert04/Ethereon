<script lang="ts">
  // Interruptor (en vez de una casilla). Clic en todo el renglon, Espacio o Enter.
  import type { Snippet } from "svelte";

  let {
    checked = $bindable(false),
    onchange,
    disabled = false,
    children,
  }: { checked?: boolean; onchange?: (v: boolean) => void; disabled?: boolean; children?: Snippet } = $props();

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button type="button" class="switch-row" role="switch" aria-checked={checked} {disabled} onclick={toggle}>
  <span class="track" class:on={checked}><span class="thumb"></span></span>
  {#if children}<span class="label">{@render children()}</span>{/if}
</button>

<style>
  .switch-row {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    padding: 2px 0;
  }
  .switch-row:disabled {
    opacity: 0.5;
  }
  .track {
    position: relative;
    flex: none;
    width: 34px;
    height: 20px;
    border-radius: 999px;
    background: var(--panel-2);
    border: var(--border) solid var(--line-strong);
    transition: background 0.2s, border-color 0.2s;
  }
  .track.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text-2);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.35);
    transition:
      transform 0.22s cubic-bezier(0.34, 1.56, 0.64, 1),
      background 0.2s,
      width 0.15s;
  }
  .on .thumb {
    transform: translateX(14px);
    background: var(--accent-ink);
  }
  /* Al apretar, el pulgar se estira (como en iOS). */
  .switch-row:active .thumb {
    width: 18px;
  }
  .switch-row:active .on .thumb {
    transform: translateX(10px);
  }
  .switch-row:focus-visible .track {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 25%, transparent);
  }
  :global([data-theme="athanor"]) .track,
  :global([data-theme="athanor"]) .thumb {
    border-radius: 0;
  }
</style>
