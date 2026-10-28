<script lang="ts">
  // Editor de reglas de un host o grupo.
  import { MODE_LABEL, type Mode, type Policy } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import Seg from "./Seg.svelte";
  import Switch from "./Switch.svelte";

  let { policy = $bindable() }: { policy: Policy } = $props();

  const modes: Mode[] = ["read_only", "approval", "free", "off"];
  const help: Record<Mode, string> = {
    read_only: "Only commands in the allowed list run. Nothing that writes files.",
    approval: "Every command waits until you approve it in Ethereon.",
    free: "Anything that is not forbidden runs. For test machines.",
    off: "The agent cannot see or use this host.",
  };

  const lines = (s: string) =>
    s
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);
</script>

<div class="stack">
  <Seg bind:value={policy.mode} options={modes.map((m) => ({ value: m, label: MODE_LABEL[m] }))} />
  <div class="faint">{help[policy.mode]}</div>

  {#if policy.mode === "read_only"}
    <label class="field">
      Allowed commands <span class="hint">One rule per line. Empty uses the default list.</span>
      <textarea
        class="mono"
        rows="6"
        placeholder={app.snap?.default_allow.join("\n")}
        value={policy.allow.join("\n")}
        oninput={(e) => (policy.allow = lines(e.currentTarget.value))}
      ></textarea>
    </label>
  {/if}

  {#if policy.mode !== "off"}
    <label class="field">
      Also forbidden here <span class="hint">Added to the global list in Rules.</span>
      <textarea
        class="mono"
        rows="3"
        placeholder="systemctl stop *"
        value={policy.deny.join("\n")}
        oninput={(e) => (policy.deny = lines(e.currentTarget.value))}
      ></textarea>
    </label>
    <div class="grid3">
      <label class="field">Timeout (s)<input type="number" min="1" bind:value={policy.timeout_secs} /></label>
      <label class="field">Max output (KB)<input type="number" min="1" bind:value={policy.max_output_kb} /></label>
      <label class="field">Commands / min<input type="number" min="1" bind:value={policy.max_per_minute} /></label>
    </div>
    <Switch bind:checked={policy.allow_sudo}>Allow sudo (Ethereon types the password)</Switch>
  {/if}
</div>

<style>
  .grid3 {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
</style>
