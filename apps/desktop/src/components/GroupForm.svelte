<script lang="ts">
  import { untrack } from "svelte";
  // El formulario trabaja sobre una copia: se monta de nuevo cada vez que se abre.
  import Drawer from "./Drawer.svelte";
  import PolicyEditor from "./PolicyEditor.svelte";
  import { api, defaultPolicy, type Group } from "../lib/api";
  import { attempt } from "../lib/state.svelte";
  import { ask } from "../lib/confirm.svelte";

  let { group, onclose }: { group: Group | null; onclose: () => void } = $props();
  const init = untrack(() => group);
  const original = init?.name ?? null;
  let name = $state(init?.name ?? "");
  let policy = $state(init ? structuredClone($state.snapshot(init.policy)) : defaultPolicy());

  async function save() {
    if (await attempt(() => api.groupSave({ name: name.trim(), policy }, original), "Group saved")) onclose();
  }
  async function remove() {
    if (original && (await ask({ title: `Delete group ${original}?`, body: "Its hosts stay, without a group.", confirm: "Delete group", danger: true }))) {
      if (await attempt(() => api.groupDelete(original))) onclose();
    }
  }
</script>

<Drawer title={original ? `Group ${original}` : "New group"} {onclose} onsave={save}>
  <label class="field">Name<input bind:value={name} placeholder="production" /></label>
  <div class="section-title">Rules for every host in the group</div>
  <PolicyEditor bind:policy />
  {#snippet footer()}
    {#if original}<button class="btn danger" onclick={remove}>Delete</button>{/if}
    <span class="grow"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" onclick={save}>Save</button>
  {/snippet}
</Drawer>
