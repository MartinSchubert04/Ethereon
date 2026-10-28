<script lang="ts" generics="T">
  // Control segmentado con una pildora que se desliza a la opcion elegida.
  let {
    value = $bindable(),
    options,
    onchange,
  }: { value: T; options: { value: T; label: string }[]; onchange?: (v: T) => void } = $props();

  const index = $derived(Math.max(0, options.findIndex((o) => o.value === value)));
</script>

<div class="seg2" role="radiogroup" style="--n: {options.length}; --i: {index}">
  <span class="pill" aria-hidden="true"></span>
  {#each options as o}
    <button
      type="button"
      role="radio"
      aria-checked={o.value === value}
      class:on={o.value === value}
      onclick={() => {
        // Con onchange el que decide es el padre (puede pedir confirmacion y no cambiar).
        if (onchange) onchange(o.value);
        else value = o.value;
      }}>{o.label}</button
    >
  {/each}
</div>

<style>
  .seg2 {
    position: relative;
    display: grid;
    grid-template-columns: repeat(var(--n), 1fr);
    height: 36px;
    padding: 3px;
    background: var(--panel-2);
    border: var(--border) solid var(--line);
    border-radius: var(--radius-sm);
  }
  .pill {
    position: absolute;
    top: 3px;
    bottom: 3px;
    left: 3px;
    width: calc((100% - 6px) / var(--n));
    transform: translateX(calc(var(--i) * 100%));
    background: var(--panel);
    border-radius: calc(var(--radius-sm) - 2px);
    box-shadow:
      0 0 0 var(--border) var(--line-strong),
      0 2px 8px -2px rgba(0, 0, 0, 0.35);
    transition: transform 0.28s cubic-bezier(0.34, 1.3, 0.64, 1);
  }
  button {
    position: relative;
    z-index: 1;
    font-size: 12.5px;
    font-weight: 560;
    color: var(--text-2);
    transition: color 0.2s;
    white-space: nowrap;
    padding: 0 8px;
  }
  button:hover {
    color: var(--text);
  }
  button.on {
    color: var(--text);
  }
  :global([data-theme="athanor"]) .pill,
  :global([data-theme="poolsuite"]) .pill {
    background: var(--accent);
    box-shadow: none;
  }
  :global([data-theme="athanor"]) button.on,
  :global([data-theme="poolsuite"]) button.on {
    color: var(--accent-ink);
  }
  :global([data-theme="athanor"]) .pill {
    transition-timing-function: steps(4);
  }
</style>
