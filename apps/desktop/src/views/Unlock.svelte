<script lang="ts">
  import { LoaderCircle, Lock } from "@lucide/svelte";
  import { api, errText } from "../lib/api";
  import { app, refresh } from "../lib/state.svelte";

  const creating = $derived(!app.status?.exists);
  let password = $state("");
  let confirm = $state("");
  let error = $state("");
  let busy = $state(false);

  async function submit(e: Event) {
    e.preventDefault();
    error = "";
    if (creating && password !== confirm) {
      error = "The passwords do not match.";
      return;
    }
    busy = true;
    try {
      await (creating ? api.create(password) : api.unlock(password));
      password = confirm = "";
      await refresh();
    } catch (err) {
      error = errText(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="wrap">
  <form class="card box" onsubmit={submit}>
    <div class="logo titlebar"><span><Lock size={20} /></span></div>
    <h1>{creating ? "Create your vault" : "Ethereon is locked"}</h1>
    <p class="muted">
      {#if creating}
        Your hosts, keys and rules are encrypted with this password. Nobody can recover it, so keep it somewhere safe.
      {:else}
        Enter the master password. While it is locked, agents get no access.
      {/if}
    </p>
    <!-- svelte-ignore a11y_autofocus -->
    <input type="password" placeholder="Master password" bind:value={password} autofocus />
    {#if creating}
      <input type="password" placeholder="Repeat it" bind:value={confirm} />
    {/if}
    {#if error}<div class="err">{error}</div>{/if}
    <button class="btn primary big" disabled={busy || password.length < (creating ? 8 : 1)}>
      {#if busy}<LoaderCircle size={15} class="spin" />{/if}
      {creating ? "Create vault" : "Unlock"}
    </button>
    {#if creating}<div class="faint small">At least 8 characters.</div>{/if}
  </form>
</div>

<style>
  .wrap {
    height: 100%;
    display: grid;
    place-items: center;
    padding: 20px;
  }
  .box {
    width: min(380px, 100%);
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-shadow: var(--pop-shadow);
  }
  .logo {
    display: grid;
    place-items: center;
    height: 46px;
    border-radius: var(--radius-sm);
    margin-bottom: 4px;
  }
  .logo span {
    display: grid;
    place-items: center;
    width: 46px;
    height: 46px;
    border-radius: 12px;
    background: var(--primary-bg);
    color: var(--accent-ink);
    border: var(--border) solid var(--line);
  }
  h1 {
    margin: 0;
    font-size: 19px;
    text-align: center;
  }
  p {
    margin: 0 0 6px;
    text-align: center;
    font-size: 12.5px;
    line-height: 1.5;
  }
  .big {
    height: 38px;
  }
  .err {
    color: var(--danger);
    font-size: 12.5px;
  }
  .small {
    font-size: 11.5px;
    text-align: center;
  }
</style>
