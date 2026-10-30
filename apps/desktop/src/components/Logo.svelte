<script lang="ts">
  // Un icono del catalogo de lib/icons.ts: un logo de simple-icons o uno de lucide.
  import { ICONS } from "../lib/icons";

  let { id, size = 14 }: { id: string; size?: number } = $props();
  const def = $derived(ICONS[id] ?? ICONS.terminal);
</script>

{#if def.si}
  <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" role="img" aria-label={def.label}>
    <title>{def.label}</title>
    <path d={def.si.path} />
  </svg>
{:else if def.lucide}
  <def.lucide {size} />
{/if}
