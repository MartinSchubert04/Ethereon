<script lang="ts">
  // Barra de arriba al estilo Termius: es la barra de titulo de la ventana (no hay marco
  // de Windows) y a la vez la tira de pestañas. Se arrastra desde cualquier hueco y doble
  // clic maximiza (data-tauri-drag-region).
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { ArrowDownToLine, Bell, Copy, FolderOpen, Inbox, Layers, Lock, LayoutGrid, Menu, Minus, Plus, Square, SquareTerminal, X } from "@lucide/svelte";
  import { moveTab, otherWindows, windowAtCursor } from "../lib/windows";
  import { api } from "../lib/api";
  import { onMount } from "svelte";
  import { app, closeTab, goToTab, openApprovals, toggleSidebar } from "../lib/state.svelte";
  import { pop } from "../lib/motion";
  import { drop, mergeInto, splittable, tileAt } from "../lib/tiles.svelte";

  let { locked = false }: { locked?: boolean } = $props();

  const win = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    const sync = async () => (maximized = await win.isMaximized());
    sync();
    const un = win.onResized(sync);
    return () => un.then((f) => f());
  });

  const onTerm = $derived(app.view === "terminal");
  /** Ventana de terminales sacada de la principal: sin Vault ni "New tab". */
  const detached = app.detached;

  // ---- arrastrar pestañas: reordenar, o soltar fuera de la ventana para abrirla aparte

  // Bajarla a las terminales la suma a la pestaña que se ve, al costado del panel que se
  // marca (mosaico). Si la que se arrastra es la que se ve, se muestra la de al lado, como
  // al arrancar una pestaña en el navegador.
  let td: { key: number; sx: number; sy: number; started: boolean; out: boolean; shown?: number } | null = null;
  let dragKey = $state<number | null>(null);
  let dragOut = $state(false);
  /** Otra ventana de Ethereon bajo el cursor: soltar ahi pasa la pestaña a esa ventana. */
  let dropOn = $state<{ label: string; name: string } | null>(null);
  let lastAsk = 0;
  let suppressClick = false;

  function tabDown(e: PointerEvent, key: number) {
    if (e.button !== 0 || (e.target as HTMLElement).closest(".x")) return;
    td = { key, sx: e.clientX, sy: e.clientY, started: false, out: false };
    // Con captura siguen llegando los movimientos aunque el mouse salga de la ventana.
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function tabMove(e: PointerEvent) {
    if (!td) return;
    if (!td.started && Math.hypot(e.clientX - td.sx, e.clientY - td.sy) > 6) {
      td.started = true;
      dragKey = td.key;
    }
    if (!td.started) return;
    const inside = e.clientY >= -16 && e.clientX >= -16 && e.clientX <= innerWidth + 16 && e.clientY <= innerHeight + 16;
    const barBottom = (e.currentTarget as HTMLElement).closest(".bar")?.getBoundingClientRect().bottom ?? 46;
    const dragged = app.terms.find((t) => t.key === td!.key);
    if (inside && e.clientY > barBottom + 8 && dragged && splittable(dragged) && app.terms.length > 1) {
      if (app.activeTerm === td.key) {
        const i = app.terms.findIndex((t) => t.key === td!.key);
        td.shown = td.key;
        app.activeTerm = (app.terms[i + 1] ?? app.terms[i - 1]).key;
        app.view = "terminal";
      }
      const hit = tileAt(e.clientX, e.clientY);
      drop.hint = hit && hit.tab !== td.key ? hit : null;
      if (drop.hint) {
        td.out = false;
        dragOut = false;
        dropOn = null;
        return;
      }
    } else {
      drop.hint = null;
      if (td.shown && e.clientY <= barBottom + 8) {
        app.activeTerm = td.shown;
        td.shown = undefined;
      }
    }
    td.out = e.clientY < -16 || e.clientY > 110 || e.clientX < -16 || e.clientX > innerWidth + 16;
    dragOut = td.out;
    if (td.out) {
      // Cada tanto: ¿esta encima de otra ventana de Ethereon?
      const now = Date.now();
      if (now - lastAsk > 120) {
        lastAsk = now;
        windowAtCursor().then((w) => td && (dropOn = w));
      }
      return;
    }
    dropOn = null;
    const over = document
      .elementsFromPoint(e.clientX, 23)
      .find((el) => el instanceof HTMLElement && el.classList.contains("tab") && el.dataset.key) as HTMLElement | undefined;
    if (!over) return;
    const from = app.terms.findIndex((t) => t.key === td!.key);
    const to = app.terms.findIndex((t) => t.key === Number(over.dataset.key));
    if (from < 0 || to < 0 || from === to) return;
    const [t] = app.terms.splice(from, 1);
    app.terms.splice(to, 0, t);
  }

  async function tabUp(e: PointerEvent) {
    const d = td;
    td = null;
    dragKey = null;
    dragOut = false;
    dropOn = null;
    const hint = drop.hint;
    drop.hint = null;
    if (!d?.started) return;
    suppressClick = true;
    setTimeout(() => (suppressClick = false), 0);
    if (hint) return mergeInto(d.key, hint.tab, hint.leaf, hint.side);
    if (!d.out) {
      if (d.shown) app.activeTerm = d.shown;
      return;
    }
    // Soltada sobre otra ventana de Ethereon: se pasa a esa. Si no, ventana nueva ahi.
    const target = await windowAtCursor();
    if (target) await moveTab(d.key, target.label);
    else await moveTab(d.key, "new", { x: e.screenX, y: e.screenY });
  }

  // ---- clic derecho en una pestaña

  let menu = $state<{ x: number; key: number; wins: { label: string; name: string }[] } | null>(null);
  async function tabMenu(e: MouseEvent, key: number) {
    e.preventDefault();
    const wins = await otherWindows();
    menu = { x: Math.min(e.clientX, innerWidth - 240), key, wins };
  }
  function act(f: () => void) {
    menu = null;
    f();
  }
  const menuTab = $derived(menu ? app.terms.find((t) => t.key === menu!.key) : undefined);
  function toggle(field: "locked" | "collapsed") {
    const t = menuTab;
    menu = null;
    if (t) t[field] = !t[field];
  }
</script>

<svelte:window
  onpointerdown={(e) => menu && !(e.target as HTMLElement).closest(".tabmenu") && (menu = null)}
  onkeydown={(e) => menu && e.key === "Escape" && (menu = null)}
/>

<header class="bar" data-tauri-drag-region>
  {#if locked}
    <div class="brand" data-tauri-drag-region>
      <img src="/icon.svg" alt="" width="18" height="18" data-tauri-drag-region />
      <span data-tauri-drag-region>Ethereon</span>
    </div>
  {:else}
    {#if detached}
      <img class="mark" src="/icon.svg" alt="" width="18" height="18" data-tauri-drag-region />
    {:else}
      <button class="ham" title="Sidebar (Ctrl B)" onclick={toggleSidebar}><Menu size={17} /></button>
    {/if}

    <div class="tabs" data-tauri-drag-region>
      {#if !detached}
        <button class="tab vault" class:on={!onTerm} title="Vault (Ctrl 1)" onclick={() => goToTab(0)}>
          <img src="/icon.svg" alt="" width="16" height="16" />
          <span>Vault</span>
        </button>
      {/if}
      {#each app.terms as t, i (t.key)}
        <div
          in:pop={{ start: 0.85 }}
          class="tab"
          class:on={onTerm && app.activeTerm === t.key}
          class:dragging={dragKey === t.key}
          class:out={dragKey === t.key && dragOut}
          class:collapsed={t.collapsed}
          data-key={t.key}
          role="button"
          tabindex="0"
          aria-label={t.title}
          title={t.collapsed ? `${t.title} · Ctrl ${detached ? i + 1 : i + 2}` : `Ctrl ${detached ? i + 1 : i + 2}`}
          onpointerdown={(e) => tabDown(e, t.key)}
          onpointermove={tabMove}
          onpointerup={tabUp}
          oncontextmenu={(e) => tabMenu(e, t.key)}
          onclick={() => !suppressClick && goToTab(i + 1)}
          onkeydown={(e) => e.key === "Enter" && goToTab(i + 1)}
          onauxclick={(e) => e.button === 1 && closeTab(t.key)}
        >
          <span class="dot" class:dead={!t.alive}>
            {#if t.layout}<LayoutGrid size={14} />{:else if t.members}<Layers size={14} />{:else if t.sftp}<FolderOpen size={14} />{:else if t.approvals}<Inbox size={14} />{:else}<SquareTerminal size={14} />{/if}
          </span>
          {#if !t.collapsed}
            <span class="label ellipsis">{t.title}{#if t.members}<span class="n">{" · "}{t.members.length}</span>{:else if t.approvals && app.approvals.length}<span class="n">{" · "}{app.approvals.length}</span>{/if}</span>
          {/if}
          {#if t.locked}
            <span class="lock" title="Locked"><Lock size={t.collapsed ? 10 : 11} /></span>
          {:else if !t.collapsed}
            <button class="x" aria-label="Close tab" title="Close (Ctrl W)" onclick={(e) => (e.stopPropagation(), closeTab(t.key))}>
              <X size={12} />
            </button>
          {/if}
        </div>
      {/each}
      {#if !detached}
        <button class="tab new" title="New tab (Ctrl T)" onclick={() => (app.quickOpen = true)}>
          <Plus size={15} /><span>New tab</span>
        </button>
      {/if}
    </div>
    {#if dragKey && drop.hint}
      <span class="out-hint">Release to put it {drop.hint.side === "left" || drop.hint.side === "right" ? `to the ${drop.hint.side}` : drop.hint.side === "top" ? "above" : "below"}</span>
    {:else if dragOut}
      <span class="out-hint">{dropOn ? `Release to move it to ${dropOn.name}` : "Release to open it in a new window"}</span>
    {/if}
  {/if}

  <div class="drag" data-tauri-drag-region></div>

  {#if menu}
    <div class="tabmenu card" style="left: {menu.x}px" role="menu">
      <button role="menuitem" onclick={() => act(() => moveTab(menu!.key, "new"))}>Move to new window</button>
      {#each menu.wins as w (w.label)}
        <button role="menuitem" onclick={() => act(() => moveTab(menu!.key, w.label))}>Move to {w.name}</button>
      {/each}
      <hr />
      <button role="menuitem" onclick={() => toggle("locked")}>{menuTab?.locked ? "Unlock tab" : "Lock tab (can't be closed)"}</button>
      <button role="menuitem" onclick={() => toggle("collapsed")}>{menuTab?.collapsed ? "Expand tab" : "Collapse to icon"}</button>
      <hr />
      <button role="menuitem" disabled={menuTab?.locked} onclick={() => act(() => closeTab(menu!.key))}>Close tab</button>
    </div>
  {/if}

  {#if !detached && app.update?.status === "ready"}
    <button
      class="update"
      title="Ethereon {app.update.version}: restarts and locks the vault. Installs by itself when nobody is using it."
      onclick={() => api.updateNow()}
    >
      <ArrowDownToLine size={13} /> Update
    </button>
  {/if}
  {#if !locked && app.approvals.length}
    <!-- En la principal lleva a la vista Approvals; en una ventana aparte, a la pestaña de
         aprobaciones de esa ventana (la abre si no tiene). -->
    <button class="pending" onclick={() => (detached ? openApprovals() : (goToTab(0), (app.view = "approvals")))} title="Commands waiting for approval">
      <Bell size={13} /> {app.approvals.length}
    </button>
  {/if}
  <div class="controls">
    <button class="win" title="Minimize" onclick={() => win.minimize()}><Minus size={15} /></button>
    <button class="win" title={maximized ? "Restore" : "Maximize"} onclick={() => win.toggleMaximize()}>
      {#if maximized}<Copy size={12} />{:else}<Square size={12} />{/if}
    </button>
    <button class="win close" title="Close to tray" onclick={() => win.close()}><X size={16} /></button>
  </div>
</header>

<style>
  .bar {
    height: 46px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 6px;
    background: var(--titlebar-bg, color-mix(in srgb, var(--bg) 82%, #000));
    border-bottom: var(--border) solid var(--line);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 0 10px;
    font-weight: 700;
    font-size: 13.5px;
  }
  .ham {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: var(--radius-sm);
    color: var(--text-2);
  }
  .ham:hover {
    background: var(--hover);
    color: var(--text);
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 10px 0 12px;
    border-radius: var(--radius-sm);
    background: var(--panel);
    border: var(--border) solid transparent;
    color: var(--text-2);
    font-weight: 560;
    font-size: 13px;
    white-space: nowrap;
    cursor: pointer;
    min-width: 0;
    max-width: 200px;
    transition: background 0.12s, color 0.12s;
  }
  .tab.dragging {
    opacity: 0.75;
    cursor: grabbing;
  }
  .tab.out {
    opacity: 0.35;
    outline: 1px dashed var(--accent);
  }
  .tab .n {
    color: var(--text-3);
    font-weight: 500;
  }
  .mark {
    margin: 0 6px 0 8px;
  }
  .out-hint {
    font-size: 12px;
    color: var(--accent);
    white-space: nowrap;
    padding: 0 8px;
  }
  .tabmenu {
    position: fixed;
    top: 44px;
    z-index: 95;
    min-width: 220px;
    padding: 5px;
    display: flex;
    flex-direction: column;
    background: var(--sidebar);
    box-shadow: var(--pop-shadow);
  }
  .tabmenu button {
    text-align: left;
    padding: 7px 10px;
    border-radius: var(--radius-sm);
    font-size: 13px;
  }
  .tabmenu button:hover:not(:disabled) {
    background: var(--hover);
  }
  .tabmenu button:disabled {
    color: var(--text-3);
    cursor: default;
  }
  .tabmenu hr {
    border: 0;
    border-top: var(--border) solid var(--line);
    margin: 4px 2px;
  }
  .tab:hover {
    color: var(--text);
    background: var(--panel-2);
  }
  .tab.on {
    background: var(--panel-2);
    color: var(--text);
    border-color: var(--line-strong);
  }
  .tab.vault {
    padding-right: 14px;
    flex: none;
  }
  .tab.new {
    background: none;
    color: var(--text-3);
    padding: 0 10px;
    flex: none;
  }
  .tab.new:hover {
    color: var(--text);
    background: var(--hover);
  }
  .label {
    min-width: 0;
  }
  .dot {
    display: grid;
    color: var(--accent);
    flex: none;
  }
  /* Sobre una pestaña activa pintada con el acento, el icono va en el color de tinta. */
  :global([data-theme="athanor"]) .tab.on .dot,
  :global([data-theme="poolsuite"]) .tab.on .dot {
    color: var(--accent-ink);
  }
  .dot.dead {
    color: var(--text-3);
  }
  .x {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 4px;
    flex: none;
    opacity: 0.55;
  }
  .x:hover {
    opacity: 1;
    background: var(--hover);
  }
  /* Colapsada: solo el icono (y el candado chico en la esquina si esta bloqueada). */
  .tab.collapsed {
    position: relative;
    flex: none;
    gap: 0;
    width: 36px;
    padding: 0;
    justify-content: center;
  }
  .lock {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    flex: none;
    color: var(--text-3);
  }
  .tab.collapsed .lock {
    position: absolute;
    right: 2px;
    bottom: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--panel-2);
    color: var(--text-2);
  }
  .drag {
    flex: 1;
    align-self: stretch;
  }
  .pending {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 700;
    color: #1a1a1a;
    background: var(--warn);
  }
  .update {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 11px;
    border-radius: var(--radius-sm);
    font-size: 12px;
    font-weight: 650;
    color: var(--text);
    background: var(--panel-2);
    border: var(--border) solid var(--line-strong);
  }
  .update:hover {
    border-color: var(--accent);
  }
  .controls {
    display: flex;
    align-self: stretch;
  }
  .win {
    width: 46px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    transition: background 0.12s, color 0.12s;
  }
  .win:hover {
    background: var(--hover);
    color: var(--text);
  }
  .win.close:hover {
    background: #c42b1c;
    color: #fff;
  }

  /* Poolsuite: barra de ventana del Mac clasico (rayas) y pestañas con borde negro. */
  :global([data-theme="poolsuite"]) .bar {
    background: var(--sidebar);
  }
  :global([data-theme="poolsuite"]) .drag {
    background: var(--titlebar);
    background-size: 100% 13px;
    background-repeat: no-repeat;
    background-position: center;
    margin: 0 8px;
  }
  :global([data-theme="poolsuite"]) .tab:not(.new) {
    border-color: var(--line);
  }
  :global([data-theme="poolsuite"]) .tab.on {
    background: var(--accent);
    color: var(--accent-ink);
    box-shadow: 2px 2px 0 var(--line);
  }
</style>
