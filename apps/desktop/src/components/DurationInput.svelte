<script lang="ts">
  // Un tiempo con su unidad: el numero y a la derecha "min" u "h". Se guarda en minutos; la
  // unidad es solo para escribirlo sin hacer cuentas (como SizeInput con KB, MB y GB).
  import Select from "./Select.svelte";

  let {
    minutes = $bindable(),
    min = 0,
    label,
    onchange,
  }: {
    minutes: number;
    /** El minimo en minutos (0 suele querer decir "nunca"). */
    min?: number;
    label: string;
    /** Cuando se termina de escribir (al salir del campo o con Enter) o se cambia la unidad. */
    onchange?: (minutes: number) => void;
  } = $props();

  type Unit = "min" | "h";
  const factor: Record<Unit, number> = { min: 1, h: 60 };
  const units = (Object.keys(factor) as Unit[]).map((u) => ({ value: u, label: u }));

  /** Al abrir: en horas si da justo (120 min se ve como 2 h). */
  const fit = (v: number): Unit => (v >= 60 && v % 60 === 0 ? "h" : "min");

  // svelte-ignore state_referenced_locally
  let unit = $state<Unit>(fit(minutes));
  // svelte-ignore state_referenced_locally
  let shown = $state<number | null>(minutes / factor[unit]);

  /** El ultimo valor que puso este campo: si `minutes` cambia por otro lado, se vuelve a
   *  mostrar en su unidad. */
  // svelte-ignore state_referenced_locally
  let mine = minutes;
  $effect(() => {
    if (minutes === mine) return;
    mine = minutes;
    unit = fit(minutes);
    shown = minutes / factor[unit];
  });

  /** El numero escrito (puede tener decimales: 1.5 h) pasado a minutos enteros. */
  function apply() {
    if (shown == null || !(shown >= 0)) return;
    minutes = mine = Math.max(min, Math.round(shown * factor[unit]));
  }

  function commit() {
    apply();
    shown = minutes / factor[unit];
    onchange?.(minutes);
  }
</script>

<div class="dur">
  <input
    type="number"
    min="0"
    step="any"
    aria-label={label}
    bind:value={shown}
    oninput={apply}
    onchange={commit}
    onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
  />
  <div class="unit">
    <Select bind:value={unit} options={units} onchange={commit} />
  </div>
</div>

<style>
  .dur {
    display: flex;
    gap: 6px;
  }
  .dur input {
    width: 90px;
  }
  .unit {
    flex: none;
    width: 76px;
  }
</style>
