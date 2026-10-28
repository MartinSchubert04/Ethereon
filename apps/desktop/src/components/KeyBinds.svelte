<script lang="ts">
  // Atajos reasignables: clic en uno, apretar la combinacion nueva. Esc cancela,
  // Retroceso lo deja sin atajo.
  import { RotateCcw } from "@lucide/svelte";
  import { ACTIONS, comboOf, prefs, pretty, resetBinds, setBind, type Action } from "../lib/prefs.svelte";

  let recording = $state<Action | null>(null);

  function start(a: Action) {
    recording = a;
    prefs.recording = true;
  }

  function stop() {
    recording = null;
    prefs.recording = false;
  }

  function onKey(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") return stop();
    if (e.key === "Backspace" || e.key === "Delete") {
      prefs.binds[recording] = "";
      return stop();
    }
    const c = comboOf(e);
    if (!c) return; // solo un modificador: se sigue esperando
    // Sin Ctrl ni Alt chocaria con escribir.
    if (!e.ctrlKey && !e.altKey && !/^F\d+$/.test(e.key)) return;
    setBind(recording, c);
    stop();
  }

  const def = (a: Action) => ACTIONS.find((x) => x.id === a)!.def;
</script>

<svelte:window onkeydowncapture={onKey} onblur={stop} />

<div class="binds">
  {#each ACTIONS as a}
    <span class="what">{a.label}</span>
    <button class="key" class:rec={recording === a.id} class:empty={!prefs.binds[a.id]} onclick={() => (recording === a.id ? stop() : start(a.id))}>
      {#if recording === a.id}
        <span class="dot"></span> Press keys...
      {:else}
        {#each pretty(prefs.binds[a.id]).split(" ") as k}<kbd>{k}</kbd>{/each}
      {/if}
    </button>
    <button
      class="icon-btn"
      title="Back to {pretty(a.def)}"
      style:visibility={prefs.binds[a.id] === def(a.id) ? "hidden" : "visible"}
      onclick={() => setBind(a.id, def(a.id))}><RotateCcw size={13} /></button
    >
  {/each}
  <span class="what faint">Go to tab 1 to 9</span>
  <span class="key fixed"><kbd>Ctrl</kbd><kbd>1</kbd><span class="faint">…</span><kbd>9</kbd></span>
  <span></span>
</div>
<div class="row foot">
  <span class="faint grow">Click a shortcut and press the new keys. Esc cancels, Backspace clears it.</span>
  <button class="btn sm ghost" onclick={resetBinds}>Reset all</button>
</div>

<style>
  .binds {
    display: grid;
    grid-template-columns: 1fr auto 28px;
    gap: 6px 12px;
    align-items: center;
  }
  .what {
    color: var(--text-2);
  }
  .key {
    display: inline-flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    min-width: 150px;
    min-height: 30px;
    padding: 3px 6px;
    border-radius: var(--radius-sm);
    border: var(--border) solid transparent;
    transition: border-color 0.15s, background 0.15s;
  }
  button.key:hover {
    border-color: var(--line-strong);
    background: var(--hover);
  }
  .key.rec {
    justify-content: center;
    border-color: var(--accent);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .key.empty kbd {
    color: var(--text-3);
    border-style: dashed;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--danger);
    animation: blink 1s infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.25;
    }
  }
  kbd {
    font-family: var(--mono);
    font-size: 11.5px;
    min-width: 22px;
    text-align: center;
    padding: 2px 7px;
    border-radius: 5px;
    border: var(--border) solid var(--line-strong);
    border-bottom-width: calc(var(--border) + 1px);
    background: var(--panel-2);
    color: var(--text);
  }
  .foot {
    margin-top: 10px;
  }
</style>
