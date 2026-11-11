<script lang="ts">
  // Editor de reglas de un host o grupo, incluido si el agente puede pasar archivos por
  // SFTP (`files`, apagado por defecto; no aparece en una consola serial).
  import { MODE_LABEL, type Mode, type Policy } from "../lib/api";
  import { app } from "../lib/state.svelte";
  import Seg from "./Seg.svelte";
  import Switch from "./Switch.svelte";
  import SizeInput from "./SizeInput.svelte";
  import { ask } from "../lib/confirm.svelte";
  import { pop } from "../lib/motion";

  // `serial`: el host es una consola serial, asi que la lista base de solo lectura es la de
  // equipos de red (show, display, get...) y no la de shell.
  let { policy = $bindable(), serial = false }: { policy: Policy; serial?: boolean } = $props();

  const modes: Mode[] = ["read_only", "approval", "guarded", "unrestricted", "off"];
  const help: Record<Mode, string> = {
    read_only: "Only commands in the allowed list run. Nothing that writes files.",
    approval: "Every command waits until you approve it in Ethereon.",
    guarded: "Anything that is not forbidden runs, without asking. For test machines.",
    unrestricted: "Anything runs, forbidden commands included. Output is still masked and logged. Only for throwaway machines.",
    off: "The agent cannot see or use this host.",
  };

  // Que puede hacer el agente con archivos en cada modo (ver `transfer.rs`).
  const filesHelp: Record<Mode, string> = {
    read_only: "The agent can download text files, with secrets masked. It cannot upload.",
    approval: "Every download and upload waits until you approve it, with the file, direction and size.",
    guarded: "Downloads and uploads run without asking, except forbidden paths (like .ssh).",
    unrestricted: "Downloads and uploads run without asking, forbidden paths included. Downloads are still masked.",
    off: "",
  };

  // Free saca todas las reglas: se confirma antes de elegirlo.
  async function pick(m: Mode) {
    if (m === "unrestricted" && policy.mode !== "unrestricted") {
      const ok = await ask({
        title: "Give the agent total freedom?",
        body: "In Free mode the agent can run anything, including commands on the forbidden list (rm -rf, shutdown, reading ~/.ssh). Output is still masked and every command is logged.",
        confirm: "Use Free",
        danger: true,
      });
      if (!ok) return;
    }
    policy.mode = m;
  }

  const lines = (s: string) =>
    s
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);
</script>

<div class="stack">
  <Seg value={policy.mode} options={modes.map((m) => ({ value: m, label: MODE_LABEL[m] }))} onchange={pick} />
  <div class="faint">{help[policy.mode]}</div>

  {#if policy.mode === "read_only"}
    <label class="field">
      Allowed commands <span class="hint">One rule per line. Empty uses the default list{serial ? " for network devices (show, display, get...)" : ""}.</span>
      <textarea
        class="mono"
        rows="6"
        placeholder={(serial ? app.snap?.default_allow_serial : app.snap?.default_allow)?.join("\n")}
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
    {#if !serial}
      <Switch bind:checked={policy.files}>Allow file transfer (SFTP)</Switch>
      {#if policy.files}
        <div class="files" transition:pop={{ start: 0.98 }}>
          <div class="faint">
            {filesHelp[policy.mode]} SSH hosts only. Binary files are never downloaded, and nothing that contains a secret
            from the vault is uploaded.
          </div>
          <SizeInput label="Max file size" bind:kb={policy.max_file_kb} />
        </div>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .files {
    display: grid;
    grid-template-columns: 1fr 210px;
    gap: 12px;
    align-items: end;
  }
  .grid3 {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
</style>
