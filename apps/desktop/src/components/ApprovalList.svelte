<script lang="ts">
  // Los pedidos que esperan aprobacion, con sus acciones. Se usa en la vista Approvals y en
  // la pestaña o panel de aprobaciones (que se puede sacar a otra ventana o poner al lado de
  // una terminal). `compact`: para un panel angosto, con menos aire. Un pedido de archivo
  // (`a.file`) muestra que archivo, en que direccion y de que tamano, y solo se aprueba o
  // se rechaza: no hay comando que editar ni patron de sesion.
  import { FileDown, FileUp, Inbox, TriangleAlert } from "@lucide/svelte";
  import { human } from "./FilePanel.svelte";
  import { api, type Approval } from "../lib/api";
  import { app, attempt } from "../lib/state.svelte";
  import { flip } from "svelte/animate";
  import { slide } from "svelte/transition";
  import { flipMs, rise } from "../lib/motion";

  let { compact = false }: { compact?: boolean } = $props();

  // Texto editado y patron por pedido (se pierden si el pedido se va).
  const edits = $state<Record<number, string>>({});
  const patterns = $state<Record<number, string>>({});
  const notes = $state<Record<number, string>>({});

  const cmd = (a: Approval) => edits[a.id] ?? a.command;
  const age = (iso: string) => {
    const s = Math.max(0, Math.round((Date.now() - Date.parse(iso)) / 1000));
    return s < 60 ? `${s}s ago` : `${Math.round(s / 60)}m ago`;
  };

  function approve(a: Approval) {
    const c = cmd(a).trim();
    attempt(() => api.resolve(a.id, c === a.command ? { action: "approve" } : { action: "approve_edited", command: c }));
  }
  function approvePattern(a: Approval) {
    const p = (patterns[a.id] ?? a.command).trim();
    attempt(() => api.resolve(a.id, { action: "approve_pattern", pattern: p }), `Allowed "${p}" for this session`);
  }
  function reject(a: Approval) {
    attempt(() => api.resolve(a.id, { action: "reject", note: notes[a.id] }));
  }
</script>

{#if !app.approvals.length}
  <div class="empty" class:compact>
    <Inbox size={compact ? 24 : 30} />
    <h3>Nothing waiting</h3>
    <div>You also get a desktop notification and a sound when something arrives (volume in Settings).</div>
  </div>
{:else}
  <div class="stack">
    {#each app.approvals as a (a.id)}
      <div class="card item" class:compact in:rise out:slide={{ duration: flipMs() }} animate:flip={{ duration: flipMs() }}>
        <div class="row head">
          <b>{a.host}</b>
          {#if a.env !== "none"}<span class="chip {a.env}">{a.env}</span>{/if}
          {#if a.sudo}<span class="chip bad">sudo</span>{/if}
          <span class="grow"></span>
          <span class="faint">{a.device} · {age(a.created)}</span>
        </div>
        {#if a.reason}<div class="reason">"{a.reason}"</div>{/if}
        {#each a.warnings as w}
          <div class="warn"><TriangleAlert size={14} /> {w}</div>
        {/each}
        {#if a.file}
          {@const f = a.file}
          <div class="file">
            {#if f.direction === "download"}<FileDown size={18} />{:else}<FileUp size={18} />{/if}
            <div class="what">
              <div><b>{f.direction === "download" ? "Download" : "Upload"}</b> · {human(f.size)}{#if f.replaces} · <span class="replace">replaces the existing file</span>{/if}</div>
              <div class="mono path">{f.path}</div>
              {#if f.local}<div class="faint">From this PC: <span class="mono">{f.local}</span></div>{/if}
              {#if f.direction === "download"}<div class="faint">The agent gets the text with secrets masked.</div>{/if}
            </div>
          </div>
          <div class="row actions">
            <button class="btn primary" onclick={() => approve(a)}>Approve</button>
            <span class="grow"></span>
            <input class="note" placeholder="Note for the agent" value={notes[a.id] ?? ""} oninput={(e) => (notes[a.id] = e.currentTarget.value)} />
            <button class="btn danger" onclick={() => reject(a)}>Reject</button>
          </div>
        {:else}
        <textarea
          class="mono cmd"
          rows={Math.min(8, cmd(a).split("\n").length + 1)}
          value={cmd(a)}
          oninput={(e) => (edits[a.id] = e.currentTarget.value)}
          spellcheck="false"
        ></textarea>
        <div class="row actions">
          <button class="btn primary" onclick={() => approve(a)}>
            {cmd(a).trim() === a.command ? "Approve" : "Approve edited"}
          </button>
          <input
            class="mono pattern"
            value={patterns[a.id] ?? a.command}
            oninput={(e) => (patterns[a.id] = e.currentTarget.value)}
            title="Commands matching this pattern (* = anything) run without asking for the rest of this agent session"
          />
          <button class="btn" onclick={() => approvePattern(a)}>Allow pattern this session</button>
          <span class="grow"></span>
          <input class="note" placeholder="Note for the agent" value={notes[a.id] ?? ""} oninput={(e) => (notes[a.id] = e.currentTarget.value)} />
          <button class="btn danger" onclick={() => reject(a)}>Reject</button>
        </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .item {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .item.compact {
    padding: 12px;
    gap: 8px;
  }
  .head b {
    font-size: 14px;
  }
  .reason {
    color: var(--text-2);
    font-style: italic;
  }
  .warn {
    display: flex;
    gap: 7px;
    align-items: center;
    color: var(--warn);
    font-size: 12.5px;
  }
  .cmd {
    min-height: 44px;
  }
  .file {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--panel-2);
    border: var(--border) solid var(--line);
    color: var(--text);
  }
  .file :global(svg) {
    flex: none;
    margin-top: 1px;
    color: var(--accent);
  }
  .what {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .path {
    word-break: break-all;
  }
  .replace {
    color: var(--warn);
  }
  .actions {
    flex-wrap: wrap;
  }
  .pattern {
    width: 220px;
  }
  .note {
    width: 180px;
  }
  /* Angosto: los campos ocupan lo que haya en vez de un ancho fijo. */
  .compact .pattern,
  .compact .note {
    flex: 1 1 140px;
    width: auto;
    min-width: 0;
  }
  .empty.compact {
    padding: 28px 12px;
  }
</style>
