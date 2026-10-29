<script lang="ts">
  // Pestaña de grupo, como Termius: a la izquierda la lista de terminales con su estado y a
  // la derecha la elegida. Todas quedan montadas, asi no se pierde ninguna sesion al cambiar.
  import { LoaderCircle, SquareTerminal, X } from "@lucide/svelte";
  import TermPane from "./TermPane.svelte";
  import { closeMember, type TermTab } from "../lib/state.svelte";

  let { tab, active }: { tab: TermTab; active: boolean } = $props();

  const members = $derived(tab.members ?? []);
  const status = (m: TermTab) => (m.connecting ? "Connecting" : m.alive ? "Connected" : "Disconnected");

  // Flechas arriba/abajo en la lista cambian de terminal.
  function onKey(e: KeyboardEvent) {
    const i = members.findIndex((m) => m.key === tab.activeMember);
    if (e.key === "ArrowDown" && i < members.length - 1) tab.activeMember = members[i + 1].key;
    else if (e.key === "ArrowUp" && i > 0) tab.activeMember = members[i - 1].key;
    else return;
    e.preventDefault();
  }
</script>

<div class="ws">
  <div class="list card" role="listbox" tabindex="0" aria-label="Terminals" onkeydown={onKey}>
    <header>
      <span class="title">Terminals</span>
      <span class="count">· {members.length}</span>
    </header>
    {#each members as m (m.key)}
      <div
        class="item"
        class:on={m.key === tab.activeMember}
        role="option"
        aria-selected={m.key === tab.activeMember}
        tabindex="-1"
        onclick={() => (tab.activeMember = m.key)}
        onkeydown={() => {}}
        onauxclick={(e) => e.button === 1 && closeMember(tab, m.key)}
      >
        <span class="ic" class:dead={!m.alive && !m.connecting}><SquareTerminal size={14} /></span>
        <span class="txt">
          <span class="name ellipsis">{m.title}</span>
          <span class="st" class:bad={!m.alive && !m.connecting}>
            {status(m)}{#if m.connecting}<LoaderCircle size={11} class="spin" />{/if}
          </span>
        </span>
        <button class="x" title="Close" aria-label="Close {m.title}" onclick={(e) => (e.stopPropagation(), closeMember(tab, m.key))}>
          <X size={13} />
        </button>
      </div>
    {/each}
  </div>
  <div class="panes">
    {#each members as m (m.key)}
      <div class="member" class:shown={m.key === tab.activeMember}>
        <TermPane tab={m} active={active && m.key === tab.activeMember} />
      </div>
    {/each}
  </div>
</div>

<style>
  .ws {
    height: 100%;
    display: grid;
    grid-template-columns: 250px minmax(0, 1fr);
    gap: 0;
    padding: 12px 0 0 12px;
    min-height: 0;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 8px;
    margin-bottom: 12px;
    overflow: auto;
    outline: none;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 5px;
    padding: 4px 8px 10px;
    font-size: 12.5px;
  }
  .title {
    font-weight: 650;
    color: var(--text-2);
  }
  .count {
    color: var(--text-3);
  }
  .item {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) 22px;
    align-items: center;
    gap: 8px;
    padding: 7px 6px 7px 8px;
    border-radius: var(--radius-sm);
    cursor: default;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.on {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .ic {
    display: grid;
    color: var(--accent);
  }
  .ic.dead {
    color: var(--text-3);
  }
  .txt {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;
  }
  .name {
    font-weight: 600;
    font-size: 13px;
  }
  .st {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .st.bad {
    color: var(--danger);
  }
  .x {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    color: var(--text-3);
    opacity: 0;
    transition: opacity 0.12s;
  }
  .item:hover .x,
  .item.on .x {
    opacity: 1;
  }
  .x:hover {
    background: var(--hover);
    color: var(--text);
  }
  .panes {
    position: relative;
    min-width: 0;
    min-height: 0;
  }
  .member {
    position: absolute;
    inset: 0;
    visibility: hidden;
  }
  .member.shown {
    visibility: visible;
  }
  /* La terminal de adentro ya trae su margen de 12 px; a la izquierda lo da la lista. */
  .member :global(.pane) {
    padding-top: 0;
  }
</style>
