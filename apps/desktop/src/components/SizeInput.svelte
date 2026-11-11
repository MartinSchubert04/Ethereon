<script lang="ts">
  // Un tamaño con su unidad: el numero y a la derecha KB, MB o GB. Se guarda en KB (como en
  // la regla); la unidad es solo para escribirlo sin hacer cuentas.
  import Select from "./Select.svelte";

  let { kb = $bindable(), label }: { kb: number; label: string } = $props();

  type Unit = "KB" | "MB" | "GB";
  const factor: Record<Unit, number> = { KB: 1, MB: 1024, GB: 1024 * 1024 };
  const units = (Object.keys(factor) as Unit[]).map((u) => ({ value: u, label: u }));

  /** Al abrir: la unidad mas grande en la que el valor da justo (1024 KB se ve como 1 MB). */
  function fit(v: number): Unit {
    if (v >= factor.GB && v % factor.GB === 0) return "GB";
    if (v >= factor.MB && v % factor.MB === 0) return "MB";
    return "KB";
  }
  // svelte-ignore state_referenced_locally
  let unit = $state<Unit>(fit(kb));
  // svelte-ignore state_referenced_locally
  let shown = $state<number | null>(kb / factor[unit]);

  /** El ultimo valor que puso este campo: si `kb` cambia por otro lado (otra regla en el mismo
   *  editor), se vuelve a mostrar en su unidad. */
  // svelte-ignore state_referenced_locally
  let mine = kb;
  $effect(() => {
    if (kb === mine) return;
    mine = kb;
    unit = fit(kb);
    shown = kb / factor[unit];
  });

  /** El numero escrito (puede tener decimales: 1.5 MB) pasado a KB enteros, minimo 1. */
  function apply() {
    if (shown == null || !(shown > 0)) return;
    kb = mine = Math.max(1, Math.round(shown * factor[unit]));
  }
</script>

<div class="field">
  {label}
  <div class="row">
    <input type="number" min="0" step="any" aria-label={label} bind:value={shown} oninput={apply} onblur={() => (shown = kb / factor[unit])} />
    <div class="unit">
      <Select bind:value={unit} options={units} onchange={apply} />
    </div>
  </div>
</div>

<style>
  .row {
    display: flex;
    gap: 6px;
  }
  .row input {
    flex: 1;
    min-width: 0;
  }
  .unit {
    flex: none;
    width: 78px;
  }
</style>
