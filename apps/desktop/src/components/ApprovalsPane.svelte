<script lang="ts">
  // Aprobaciones como pestaña o como panel del mosaico, en cualquier ventana. La lista llega
  // por el mismo evento "approvals" en todas las ventanas, asi que esta siempre al dia.
  import { Lock } from "@lucide/svelte";
  import ApprovalList from "./ApprovalList.svelte";
  import { app } from "../lib/state.svelte";

  // `bare`: dentro de un mosaico, donde el marco ya pone el nombre y el fondo.
  let { bare = false }: { bare?: boolean } = $props();
</script>

<div class="pane" class:bare>
  {#if !app.status?.unlocked}
    <div class="empty compact">
      <Lock size={24} />
      <h3>Vault is locked</h3>
      <div>Unlock it in the main window to see what is waiting.</div>
    </div>
  {:else if !bare}
    <div class="head">
      <span class="title">Approvals</span>
      <span class="sub">{app.approvals.length ? `${app.approvals.length} waiting` : "Nothing waiting"}</span>
    </div>
  {/if}
  {#if app.status?.unlocked}<ApprovalList compact />{/if}
</div>

<style>
  .pane {
    position: absolute;
    inset: 0;
    overflow: auto;
    padding: 14px 16px 24px;
    background: var(--bg);
  }
  .pane.bare {
    position: relative;
    inset: auto;
    height: 100%;
    padding: 4px 6px 6px 0;
    background: transparent;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 12px;
  }
  .title {
    font-family: var(--display);
    font-size: 16px;
    font-weight: 650;
  }
  .sub {
    color: var(--text-2);
    font-size: 12px;
  }
  .empty.compact {
    padding: 28px 12px;
  }
</style>
