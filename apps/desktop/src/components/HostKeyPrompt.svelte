<script lang="ts">
  // Confirmar una huella nueva o una que cambio.
  import { Fingerprint, TriangleAlert } from "@lucide/svelte";
  import type { SshError } from "../lib/api";
  import { fadeQuick, pop } from "../lib/motion";

  let {
    error,
    ontrust,
    oncancel,
  }: { error: Extract<SshError, { kind: "host_key_unknown" | "host_key_changed" }>; ontrust: () => void; oncancel: () => void } =
    $props();
  const changed = $derived(error.kind === "host_key_changed");
</script>

<div class="scrim" role="presentation" onclick={oncancel} transition:fadeQuick|global></div>
<div class="wrap-center"><div class="dialog card" class:changed role="dialog" transition:pop|global>
  <div class="icon">
    {#if changed}<TriangleAlert size={22} />{:else}<Fingerprint size={22} />{/if}
  </div>
  {#if error.kind === "host_key_changed"}
    <h3>The host key of {error.host} changed</h3>
    <p class="muted">
      This can mean the server was reinstalled, or that someone is in the middle. Only trust the new key if you know
      why it changed.
    </p>
    <div class="fp"><span class="faint">Before</span><code>{error.expected}</code></div>
    <div class="fp"><span class="faint">Now</span><code>{error.got}</code></div>
  {:else}
    <h3>First connection to {error.host}</h3>
    <p class="muted">Check that this fingerprint matches the server before trusting it.</p>
    <div class="fp"><span class="faint">{error.algorithm}</span><code>{error.fingerprint}</code></div>
  {/if}
  <div class="fp"><span class="faint">Address</span><code>{error.addr}</code></div>
  <div class="row actions">
    <button class="btn" onclick={oncancel}>Cancel</button>
    <button class="btn {changed ? 'danger solid' : 'primary'}" onclick={ontrust}>
      {changed ? "Trust the new key" : "Trust"}
    </button>
  </div>
</div></div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    z-index: 60;
  }
  /* Centrado con grid: el transform queda libre para la animacion. */
  .wrap-center {
    position: fixed;
    inset: 0;
    z-index: 61;
    display: grid;
    place-items: center;
    pointer-events: none;
  }
  .dialog {
    pointer-events: auto;
    width: min(520px, calc(100vw - 32px));
    padding: 22px;
    background: var(--sidebar);
    box-shadow: var(--pop-shadow);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .icon {
    color: var(--accent);
  }
  .changed .icon {
    color: var(--danger);
  }
  h3 {
    margin: 0;
    font-size: 16px;
  }
  p {
    margin: 0;
  }
  .fp {
    display: grid;
    grid-template-columns: 90px 1fr;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .fp code {
    user-select: text;
    word-break: break-all;
  }
  .actions {
    justify-content: flex-end;
    margin-top: 8px;
  }
</style>
