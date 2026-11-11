<script lang="ts">
  // Barra de arriba al estilo Termius: es la barra de titulo de la ventana (no hay marco
  // de Windows) y a la vez la tira de pestañas. Se arrastra desde cualquier hueco y doble
  // clic maximiza (data-tauri-drag-region).
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { ArrowDownToLine, Bell, Copy, FolderOpen, Inbox, Layers, Lock, LayoutGrid, Menu, Minus, Plus, Square, SquareTerminal, X } from "@lucide/svelte";
  import { moveTab, otherWindows, windowAtCursor } from "../lib/windows";
  import { invoke } from "@tauri-apps/api/core";
  import { api } from "../lib/api";
  import Logo from "./Logo.svelte";
  import { hostIcon } from "../lib/icons";
  import { prefs } from "../lib/prefs.svelte";
  import Scramble from "./Scramble.svelte";
  import { onMount } from "svelte";
  import { app, closeTab, goToTab, openApprovals, toggleSidebar } from "../lib/state.svelte";
  import { flipMs, pop } from "../lib/motion";
  import { flip } from "svelte/animate";
  import { tick } from "svelte";
  import { barTabAt, drop, mergeInto, splittable, tileAt } from "../lib/tiles.svelte";
  import { dbg, sw } from "../lib/debug.svelte";

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
  // marca (mosaico). Mientras se arrastra queda la que se veia; quedarse un momento encima
  // de otra pestaña muestra esa, para soltar ahi. Al soltar sin sumarla a nada, vuelve la
  // que se veia al empezar.
  let td: {
    key: number;
    sx: number;
    sy: number;
    started: boolean;
    out: boolean;
    /** Lo que se veia al empezar a arrastrar. */
    orig: { view: typeof app.view; active: number };
    /** La pestaña bajo el mouse y desde cuando (para mostrarla si se queda ahi). */
    hover?: number;
    hoverTimer?: number;
    /** La pestaña en la barra, y de donde se la agarro (para que vaya bajo el mouse). */
    el: HTMLElement;
    grab: number;
    pointer: number;
  } | null = null;
  let tabsEl = $state<HTMLDivElement>()!;
  /** Cuanto se corre la que se arrastra de su lugar en la fila: va bajo el mouse. */
  let dx = $state(0);
  /** El mouse salio de la barra (hacia las terminales o fuera de la ventana): la pestaña
   *  vuelve a su lugar y la sigue un rotulo. */
  let away = $state(false);
  let ghost = $state<{ x: number; y: number; title: string } | null>(null);
  let lastX = 0;

  /** Pone la que se arrastra bajo el mouse, sin salirse de la fila de pestañas. Cuenta con
   *  offsetLeft (la posicion en la fila, sin el corrimiento ni las animaciones). */
  function follow(x: number) {
    if (!td?.started || !td.el.isConnected) return;
    const row = [...tabsEl.querySelectorAll<HTMLElement>(".tab[data-key]")];
    if (!row.length) return;
    const base = tabsEl.getBoundingClientRect().left - tabsEl.scrollLeft;
    const last = row[row.length - 1];
    const want = Math.min(last.offsetLeft + last.offsetWidth - td.el.offsetWidth, Math.max(row[0].offsetLeft, x - base - td.grab));
    dx = want - td.el.offsetLeft;
  }
  /** Cuanto hay que quedarse encima de una pestaña para que se muestre. */
  const HOVER_SHOW = 450;
  /** La otra ventana a la que se le esta mandando donde esta el cursor (para que muestre su
   *  pestaña de abajo). */
  let hoverWin: string | null = null;
  function hoverWindow(label: string | null) {
    if (hoverWin && hoverWin !== label) invoke("drag_hover", { label: hoverWin, leave: true }).catch(() => {});
    hoverWin = label;
    if (label) invoke("drag_hover", { label, leave: false }).catch(() => {});
  }
  let dragKey = $state<number | null>(null);
  let dragOut = $state(false);
  /** Otra ventana de Ethereon bajo el cursor: soltar ahi pasa la pestaña a esa ventana. */
  let dropOn = $state<{ label: string; name: string } | null>(null);
  let lastAsk = 0;
  let suppressClick = false;

  function tabDown(e: PointerEvent, key: number) {
    if (e.button !== 0 || (e.target as HTMLElement).closest(".x")) return;
    const el = e.currentTarget as HTMLElement;
    const grab = e.clientX - el.getBoundingClientRect().left;
    td = { key, sx: e.clientX, sy: e.clientY, started: false, out: false, orig: { view: app.view, active: app.activeTerm }, el, grab, pointer: e.pointerId };
    dbg("pestaña: apretar", { key, view: app.view, active: app.activeTerm, foco: document.activeElement?.className }, true);
    // Que la pestaña no le saque el foco a la terminal.
    if (!sw.focus) e.preventDefault();
    // Con captura siguen llegando los movimientos aunque el mouse salga de la ventana.
    if (sw.capture) el.setPointerCapture(e.pointerId);
  }

  function tabMove(e: PointerEvent) {
    if (!td) return;
    if (!td.started && Math.hypot(e.clientX - td.sx, e.clientY - td.sy) > 6) {
      td.started = true;
      dragKey = td.key;
      if (!sw.capture) td.el.setPointerCapture(e.pointerId);
      dbg("pestaña: empieza arrastre", { key: td.key }, true);
    }
    if (!td.started) return;
    const inside = e.clientY >= -16 && e.clientX >= -16 && e.clientX <= innerWidth + 16 && e.clientY <= innerHeight + 16;
    const barBottom = tabsEl.closest(".bar")?.getBoundingClientRect().bottom ?? 46;
    lastX = e.clientX;
    away = !inside || e.clientY > barBottom + 8;
    ghost = away && inside ? { x: e.clientX, y: e.clientY, title: app.terms.find((t) => t.key === td!.key)?.title ?? "" } : null;
    if (away) dx = 0;
    else follow(e.clientX);
    const dragged = app.terms.find((t) => t.key === td!.key);
    if (inside && e.clientY > barBottom + 8 && dragged && splittable(dragged) && app.terms.length > 1) {
      hoverTab(null);
      const hit = tileAt(e.clientX, e.clientY);
      drop.hint = hit && hit.tab !== td.key ? hit : null;
      if (drop.hint) {
        td.out = false;
        dragOut = false;
        dropOn = null;
        hoverWindow(null);
        return;
      }
    } else {
      drop.hint = null;
    }
    td.out = e.clientY < -16 || e.clientY > 110 || e.clientX < -16 || e.clientX > innerWidth + 16;
    dragOut = td.out;
    if (td.out) {
      hoverTab(null);
      // Cada tanto: ¿esta encima de otra ventana de Ethereon?
      const now = Date.now();
      if (now - lastAsk > 120) {
        lastAsk = now;
        windowAtCursor().then((w) => {
          if (!td) return;
          dropOn = w;
          hoverWindow(w?.label ?? null);
        });
      }
      return;
    }
    dropOn = null;
    hoverWindow(null);
    if (e.clientY > barBottom + 8) return;
    // Por la posicion de cada una en la fila: la que se arrastra va corrida bajo el mouse y
    // las otras pueden estar deslizandose.
    const overKey = barTabAt(e.clientX);
    hoverTab(overKey !== td.key ? overKey : null);
    if (overKey == null) return;
    const from = app.terms.findIndex((t) => t.key === td!.key);
    const to = app.terms.findIndex((t) => t.key === overKey);
    if (from < 0 || to < 0 || from === to) return;
    // Se cambian de lugar recien en el ultimo cuarto de la otra (del lado hacia donde se
    // va). El medio queda para quedarse encima y verla antes de soltar. Pasado ese punto el
    // mouse queda sobre el lugar de la que se arrastra, asi que no se vuelven a cambiar solas
    // aunque tengan distinto ancho.
    const over = tabsEl.querySelector<HTMLElement>(`.tab[data-key="${overKey}"]`);
    if (!over) return;
    const left = tabsEl.getBoundingClientRect().left - tabsEl.scrollLeft + over.offsetLeft;
    const w = over.offsetWidth;
    if ((from < to && e.clientX < left + w * 0.75) || (from > to && e.clientX > left + w * 0.25)) return;
    const [t] = app.terms.splice(from, 1);
    app.terms.splice(to, 0, t);
    // Cambio su lugar en la fila: recalcular el corrimiento para que no salte. Al moverla en
    // el DOM el WebView le saca la captura del mouse: se la vuelve a dar, si no lo que pasa
    // fuera de la ventana se pierde.
    tick().then(() => {
      if (!td?.started) return;
      try {
        td.el.setPointerCapture(td.pointer);
      } catch {}
      follow(lastX);
    });
  }

  /** El mouse esta sobre otra pestaña mientras se arrastra: si se queda, se muestra esa. */
  function hoverTab(key: number | null) {
    if (!td || td.hover === (key ?? undefined)) return;
    clearTimeout(td.hoverTimer);
    td.hover = key ?? undefined;
    drop.tabOver = key;
    td.hoverTimer = undefined;
    if (key == null) return;
    td.hoverTimer = window.setTimeout(() => {
      if (!td?.started || td.hover !== key || !app.terms.some((t) => t.key === key)) return;
      dbg("pestaña: mostrar la de abajo", { key });
      app.activeTerm = key;
      app.view = "terminal";
    }, HOVER_SHOW);
  }

  async function tabUp(e: PointerEvent) {
    // Escucha en toda la ventana: un clic en cualquier lado no tiene que tocar `drop` (lo usan
    // tambien el arrastre de paneles y mover con el teclado).
    if (!td) return;
    const d = td;
    clearTimeout(d?.hoverTimer);
    drop.tabOver = null;
    // La otra ventana que mostraba su pestaña: si no se suelta ahi, vuelve a lo que tenia.
    const hw = hoverWin;
    hoverWin = null;
    const leave = () => hw && invoke("drag_hover", { label: hw, leave: true }).catch(() => {});
    if (!d?.started || !d.out) leave();
    td = null;
    dragKey = null;
    dragOut = false;
    dropOn = null;
    dx = 0;
    away = false;
    ghost = null;
    const hint = drop.hint;
    drop.hint = null;
    dbg("pestaña: soltar", { key: d?.key, started: d?.started, out: d?.out, hint, orig: d?.orig, view: app.view, active: app.activeTerm }, true);
    if (!d?.started) return;
    suppressClick = true;
    setTimeout(() => (suppressClick = false), 0);
    if (hint) return mergeInto(d.key, hint.tab, hint.leaf, hint.side);
    if (!d.out) {
      // No se sumo a ninguna: vuelve lo que se veia al empezar.
      if (app.terms.some((t) => t.key === d.orig.active)) app.activeTerm = d.orig.active;
      app.view = d.orig.view;
      return;
    }
    // Soltada sobre otra ventana de Ethereon: se pasa a esa. Si no, ventana nueva ahi.
    const target = await windowAtCursor();
    if (hw && hw !== target?.label) leave();
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
  /** Cierra el menu y hace `f` con la pestaña de ese menu (despues de cerrarlo ya no esta). */
  function act(f: (key: number) => unknown) {
    const key = menu?.key;
    menu = null;
    if (key != null) f(key);
  }
  const menuTab = $derived(menu ? app.terms.find((t) => t.key === menu!.key) : undefined);
  function toggle(field: "locked" | "collapsed") {
    const t = menuTab;
    menu = null;
    if (t) t[field] = !t[field];
  }
</script>

<!-- El arrastre de pestañas se sigue en la ventana: la pestaña puede perder la captura del
     mouse al cambiar de lugar, y el soltar no se tiene que perder. -->
<svelte:window
  onpointermove={tabMove}
  onpointerup={tabUp}
  onpointercancel={tabUp}
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

    <div class="tabs" data-tauri-drag-region bind:this={tabsEl}>
      {#if !detached}
        <button class="tab vault" class:on={!onTerm} title="Vault (Ctrl 1)" onclick={() => goToTab(0)}>
          <img src="/icon.svg" alt="" width="16" height="16" />
          <span>Vault</span>
        </button>
      {/if}
      {#each app.terms as t, i (t.key)}
        <div
          in:pop={{ start: 0.85 }}
          animate:flip={{ duration: dragKey === t.key ? 0 : flipMs() }}
          class="tab"
          style:transform={dragKey === t.key && dx ? `translateX(${dx}px)` : undefined}
          class:on={onTerm && app.activeTerm === t.key}
          class:dragging={dragKey === t.key}
          class:away={dragKey === t.key && away}
          class:out={dragKey === t.key && dragOut}
          class:collapsed={t.collapsed}
          class:dropover={drop.tabOver === t.key}
          data-key={t.key}
          role="button"
          tabindex="0"
          aria-label={t.title}
          title={`${t.title} · Ctrl ${detached ? i + 1 : i + 2}`}
          onpointerdown={(e) => tabDown(e, t.key)}
          oncontextmenu={(e) => tabMenu(e, t.key)}
          onclick={() => (dbg("pestaña: clic", { key: t.key, ignorado: suppressClick }, true), !suppressClick && goToTab(i + 1))}
          onkeydown={(e) => e.key === "Enter" && goToTab(i + 1)}
          onauxclick={(e) => e.button === 1 && closeTab(t.key)}
        >
          <span class="dot" class:dead={!t.alive}>
            {#if t.layout}<LayoutGrid size={14} />{:else if t.members}<Layers size={14} />{:else if t.sftp}<FolderOpen size={14} />{:else if t.approvals}<Inbox size={14} />{:else if t.local}<Logo id={prefs.term.shellIcons[t.local] ?? "terminal"} size={14} />{:else}<Logo id={hostIcon(app.snap?.hosts.find((h) => h.name === t.host))} size={14} />{/if}
          </span>
          {#if !t.collapsed}
            <span class="label ellipsis"><Scramble text={t.title} /></span>
            {#if t.members}<span class="n">· {t.members.length}</span>{:else if t.approvals && app.approvals.length}<span class="n">· {app.approvals.length}</span>{/if}
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

  {#if ghost}
    <div class="ghost" style="left:{ghost.x + 14}px;top:{ghost.y + 10}px">{ghost.title}</div>
  {/if}

  {#if menu}
    <div class="tabmenu card" style="left: {menu.x}px" role="menu">
      <button role="menuitem" onclick={() => act((k) => moveTab(k, "new"))}>Move to new window</button>
      {#each menu.wins as w (w.label)}
        <button role="menuitem" onclick={() => act((k) => moveTab(k, w.label))}>Move to {w.name}</button>
      {/each}
      <hr />
      <button role="menuitem" onclick={() => toggle("locked")}>{menuTab?.locked ? "Unlock tab" : "Lock tab (can't be closed)"}</button>
      <button role="menuitem" onclick={() => toggle("collapsed")}>{menuTab?.collapsed ? "Expand tab" : "Collapse to icon"}</button>
      <hr />
      <button role="menuitem" disabled={menuTab?.locked} onclick={() => act(closeTab)}>Close tab</button>
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
    /* Para medir las pestañas en la fila (offsetLeft) y que no se corte la sombra de la que
       se arrastra. */
    position: relative;
    padding: 6px 0;
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
    max-width: 240px;
    transition:
      background 0.12s,
      color 0.12s,
      opacity 0.15s,
      transform 0.2s cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  /* La que se arrastra va levantada, sobre las otras, y pegada al mouse (sin transicion). */
  .tab.dragging {
    z-index: 3;
    cursor: grabbing;
    background: var(--panel-2);
    color: var(--text);
    border-color: var(--line-strong);
    box-shadow: var(--pop-shadow);
  }
  .tab.dragging:not(.away) {
    transition:
      background 0.12s,
      color 0.12s;
  }
  /* El mouse se fue de la barra: vuelve a su lugar, tenue, y la sigue el rotulo. */
  .tab.away {
    opacity: 0.45;
    box-shadow: none;
  }
  .ghost {
    position: fixed;
    z-index: 50;
    max-width: 260px;
    padding: 4px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 999px;
    background: var(--panel-2);
    color: var(--text);
    font: 600 11.5px var(--mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: none;
    box-shadow: var(--pop-shadow);
    animation: lift 0.14s ease-out;
  }
  @keyframes lift {
    from {
      opacity: 0;
      transform: scale(0.92);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .tab,
    .ghost {
      transition: none;
      animation: none;
    }
  }
  .tab.out {
    opacity: 0.35;
    outline: 1px dashed var(--accent);
  }
  /* La pestaña sobre la que se esta arrastrando otra: si el cursor se queda, se muestra. */
  .tab.dropover {
    border-color: var(--accent);
    color: var(--text);
  }
  /* El contador va aparte del título para que los "…" no se lo coman. */
  .tab .n {
    color: var(--text-3);
    font-weight: 500;
    flex: none;
    margin-left: -4px;
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
