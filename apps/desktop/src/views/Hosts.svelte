<script lang="ts">
  import { FileInput, FolderInput, FolderOpen, Layers, GripVertical, Pencil, Plug, Plus, Search, Server, Upload } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { importLocalSsh, importPath, importPick } from "../lib/importer";
  import HostForm from "../components/HostForm.svelte";
  import GroupForm from "../components/GroupForm.svelte";
  import Logo from "../components/Logo.svelte";
  import { hostIcon } from "../lib/icons";
  import { api, MODE_LABEL, userOf, type Group, type Host } from "../lib/api";
  import { app, attempt, openGroup, openSftp, openTerminal, toast } from "../lib/state.svelte";
  import { testHost } from "../lib/hostkey.svelte";
  import { pop } from "../lib/motion";
  import { ask } from "../lib/confirm.svelte";

  let query = $state("");
  let editing = $state<Host | null | undefined>(undefined); // undefined = cerrado, null = nuevo
  let editingGroup = $state<Group | null | undefined>(undefined);

  // Ctrl N desde cualquier lado abre un host nuevo.
  $effect(() => {
    if (app.pendingNew === "host") {
      app.pendingNew = null;
      editing = null;
    }
  });
  let importOpen = $state(false);

  const hosts = $derived(
    (app.snap?.hosts ?? []).filter((h) => {
      const q = query.toLowerCase();
      return !q || [h.name, h.hostname, h.description, ...h.tags].some((x) => x.toLowerCase().includes(q));
    }),
  );
  const groups = $derived.by(() => {
    const names = [...(app.snap?.groups ?? []).map((g) => g.name)];
    const out: { name: string | null; group: Group | null; hosts: Host[] }[] = names.map((n) => ({
      name: n,
      group: app.snap!.groups.find((g) => g.name === n)!,
      hosts: hosts.filter((h) => h.group === n),
    }));
    const loose = hosts.filter((h) => !h.group || !names.includes(h.group));
    // Mientras se arrastra, "No group" aparece aunque este vacio: tambien es un destino.
    if (loose.length || !out.length || drag?.started) out.push({ name: null, group: null, hosts: loose });
    return out;
  });

  // ---------------------------------------------------------- arrastrar entre grupos
  //
  // Con el mouse (no con el drag and drop del navegador, que el WebView le pasa a
  // Windows): una copia de la tarjeta sigue al cursor y el grupo de abajo se resalta.

  interface Drag {
    host: Host;
    x: number;
    y: number;
    startX: number;
    startY: number;
    offX: number;
    offY: number;
    w: number;
    started: boolean;
    /** Nombre del grupo de abajo; null = sin grupo; undefined = ninguno. */
    over: string | null | undefined;
    /** Delante de que host cae (null = al final del grupo). */
    before: string | null;
    /** Donde se dibuja la linea que marca el lugar. */
    mark: { name: string; side: "before" | "after" } | null;
  }
  let drag = $state<Drag | null>(null);
  let justDragged = false;

  function dragStart(e: PointerEvent, h: Host) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    drag = {
      host: h,
      x: e.clientX,
      y: e.clientY,
      startX: e.clientX,
      startY: e.clientY,
      offX: e.clientX - r.left,
      offY: e.clientY - r.top,
      w: r.width,
      started: false,
      over: undefined,
      before: null,
      mark: null,
    };
  }

  function dragMove(e: PointerEvent) {
    if (!drag) return;
    drag.x = e.clientX;
    drag.y = e.clientY;
    if (!drag.started && Math.hypot(e.clientX - drag.startX, e.clientY - drag.startY) > 6) drag.started = true;
    if (!drag.started) return;
    const under = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
    const zone = under?.closest<HTMLElement>("[data-drop]");
    drag.over = zone ? (zone.dataset.drop === "" ? null : zone.dataset.drop) : undefined;
    // El lugar: delante o detras de la tarjeta de abajo, segun la mitad en la que esta el cursor.
    // Las tarjetas no reciben el mouse mientras se arrastra: se busca por posicion.
    const card = zone
      ? [...zone.querySelectorAll<HTMLElement>(".host[data-name]")].find((c) => {
          const r = c.getBoundingClientRect();
          return e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
        })
      : undefined;
    if (zone && card && card.dataset.name !== drag.host.name) {
      const r = card.getBoundingClientRect();
      const side = e.clientX < r.left + r.width / 2 ? "before" : "after";
      const cards = [...zone.querySelectorAll<HTMLElement>(".host[data-name]")].filter((c) => c.dataset.name !== drag!.host.name);
      const next = cards[cards.indexOf(card) + 1]?.dataset.name ?? null;
      drag.mark = { name: card.dataset.name!, side };
      drag.before = side === "before" ? card.dataset.name! : next;
    } else if (zone && !card) {
      drag.mark = null;
      drag.before = null; // al final del grupo
    } else if (!zone) {
      drag.mark = null;
    }
    // Cerca de los bordes, la pagina se desplaza sola.
    const page = document.querySelector(".hosts-page");
    if (page) {
      const r = page.getBoundingClientRect();
      if (e.clientY < r.top + 50) page.scrollBy(0, -14);
      else if (e.clientY > r.bottom - 50) page.scrollBy(0, 14);
    }
  }

  async function dragEnd() {
    const d = drag;
    drag = null;
    if (!d?.started) return;
    justDragged = true;
    setTimeout(() => (justDragged = false), 50);
    if (d.over === undefined) return;
    await moveHost(d.host, d.over, d.before);
  }

  const groupPolicy = (name: string | null) => app.snap?.groups.find((g) => g.name === name)?.policy;

  async function moveHost(h: Host, to: string | null, place: string | null = null) {
    const sameGroup = (h.group ?? null) === to;
    const before = h.policy?.mode ?? groupPolicy(h.group)?.mode ?? "approval";
    const after = h.policy?.mode ?? groupPolicy(to)?.mode ?? "approval";
    // Pasar a un modo donde el agente no pregunta se confirma.
    const rank = { off: 0, read_only: 1, approval: 2, guarded: 3, unrestricted: 4 } as const;
    if (!sameGroup && rank[after] > rank[before] && rank[after] >= 3) {
      const ok = await ask({
        title: `Move ${h.name} to ${to ?? "No group"}?`,
        body:
          after === "unrestricted"
            ? `${to} is in Free mode: the agent can run anything on ${h.name}, forbidden commands included, without asking you.`
            : `${to} is in Guarded mode: the agent will run commands on ${h.name} without asking you. Forbidden commands are still blocked.`,
        confirm: "Move",
        danger: true,
      });
      if (!ok) return;
    }
    await attempt(() => api.hostMove(h.name, to, place), sameGroup ? undefined : `${h.name} moved to ${to ?? "No group"}`);
  }

  function mode(h: Host) {
    const p = h.policy ?? app.snap?.groups.find((g) => g.name === h.group)?.policy;
    return p?.mode ?? "approval";
  }

  // Soltar un archivo o carpeta sobre Hosts lo importa (el formato se reconoce solo).
  let fileOver = $state(false);
  onMount(() => {
    let off: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent(async (ev) => {
        if (app.view !== "hosts") return;
        const p = ev.payload;
        if (p.type === "enter" || p.type === "over") fileOver = true;
        else if (p.type === "leave") fileOver = false;
        else if (p.type === "drop") {
          fileOver = false;
          for (const path of p.paths) await importPath(path);
        }
      })
      .then((u) => (off = u));
    return () => off?.();
  });
</script>

<div class="page hosts-page" class:is-dragging={drag?.started} class:file-over={fileOver}>
  <div class="page-head">
    <h1>Hosts</h1>
    <span class="sub">{app.snap?.hosts.length ?? 0} saved</span>
    <span class="grow"></span>
    <div class="search inbox">
      <Search size={14} />
      <input placeholder="Search (Ctrl F)" bind:value={query} data-host-search />
    </div>
    <div class="menu-wrap">
      <button class="btn" onclick={() => (importOpen = !importOpen)}><Upload size={14} /> Import</button>
      {#if importOpen}
        <div class="menu card" transition:pop|global>
          <button onclick={() => ((importOpen = false), importPick())}>
            <FileInput size={14} />
            <span class="mi"><b>A file...</b><small>Ethereon backup or export, SSH config, key, Bitwarden. Detected by itself.</small></span>
          </button>
          <button onclick={() => ((importOpen = false), importPick(true))}>
            <FolderInput size={14} />
            <span class="mi"><b>A folder...</b><small>Ethereon OpenSSH export or a .ssh folder</small></span>
          </button>
          <button onclick={() => ((importOpen = false), importLocalSsh())}>
            <FolderInput size={14} />
            <span class="mi"><b>~/.ssh/config of this PC</b><small>Or drop any file on this page</small></span>
          </button>
        </div>
      {/if}
    </div>
    <button class="btn" onclick={() => (editingGroup = null)}>New group</button>
    <button class="btn primary" onclick={() => (editing = null)}><Plus size={15} /> New host</button>
  </div>

  {#if !app.snap?.hosts.length}
    <div class="empty">
      <Server size={30} />
      <h3>No hosts yet</h3>
      <div>Add one, import a file (or drop it here): Ethereon backups and exports, SSH config, keys or Bitwarden.</div>
      <div class="row">
        <button class="btn" onclick={() => importPick()}>Import a file</button>
        <button class="btn primary" onclick={() => (editing = null)}>New host</button>
      </div>
    </div>
  {:else}
    {#each groups as g (g.name ?? "")}
      {#if g.hosts.length || g.group || drag?.started}
        <section
          class="zone"
          class:over={drag?.started && drag.over === g.name}
          class:target={drag?.started && drag.over !== g.name && (drag.host.group ?? null) !== g.name}
          data-drop={g.name ?? ""}
        >
          <div class="group-head">
            <span class="section-title">{g.name ?? "No group"}</span>
            {#if g.group}
              <span class="chip" class:bad={g.group.policy.mode === "unrestricted"} class:warn={g.group.policy.mode === "guarded"}>{MODE_LABEL[g.group.policy.mode]}</span>
              <button class="icon-btn" aria-label="Edit group" onclick={() => (editingGroup = g.group)}><Pencil size={13} /></button>
            {/if}
            {#if g.hosts.length > 1}
              <button
                class="open-all"
                title="Open every host of {g.name ?? 'this list'} in one tab, with the list on the left"
                onclick={() => openGroup(g.name ?? "No group", g.hosts.map((h) => h.name))}
              >
                <Layers size={13} /> Open all
              </button>
            {/if}
          </div>
          <div class="grid">
            {#each g.hosts as h, i (h.name)}
              <div
                class="host card lift stagger"
                class:dragging={drag?.started && drag.host.name === h.name}
                class:ins-before={drag?.started && drag.mark?.name === h.name && drag.mark.side === "before"}
                class:ins-after={drag?.started && drag.mark?.name === h.name && drag.mark.side === "after"}
                data-name={h.name}
                style="--i: {i}"
                role="button"
                tabindex="0"
                onpointerdown={(e) => dragStart(e, h)}
                ondblclick={() => !justDragged && openTerminal(h.name)}
                onkeydown={(e) => e.key === "Enter" && openTerminal(h.name)}
              >
                <div class="top">
                  <span class="grip" title="Drag to reorder or to another group"><GripVertical size={14} /></span>
                  <div class="badge {h.env}"><Logo id={hostIcon(h)} size={16} /></div>
                  <div class="grow info">
                    <div class="name ellipsis">{h.name}</div>
                    <div class="addr mono ellipsis">{userOf(h, app.snap?.credentials)}@{h.hostname}{h.port !== 22 ? `:${h.port}` : ""}</div>
                  </div>
                  <div class="actions">
                    <button class="icon-btn" title="Test connection" onclick={() => testHost(h.name)}><Plug size={14} /></button>
                    <button class="icon-btn" title="Edit" onclick={() => (editing = h)}><Pencil size={14} /></button>
                  </div>
                </div>
                {#if h.description}<div class="desc faint">{h.description}</div>{/if}
                <div class="chips">
                  {#if h.env !== "none"}<span class="chip {h.env}">{h.env}</span>{/if}
                  <span class="chip">{MODE_LABEL[mode(h)]}</span>
                  {#if h.jump.length}<span class="chip">via {h.jump.join(" → ")}</span>{/if}
                  {#each h.tags as t}<span class="chip">{t}</span>{/each}
                </div>
                <div class="connect-row">
                  <button class="btn sm connect" onclick={() => openTerminal(h.name)}>Connect</button>
                  <button class="btn sm" title="Browse files (SFTP)" onclick={() => openSftp(h.name)}><FolderOpen size={13} /> SFTP</button>
                </div>
              </div>
            {/each}
            {#if drag?.started && (drag.host.group ?? null) !== g.name}
              <div class="drop-hint">Drop here to move {drag.host.name}</div>
            {/if}
          </div>
        </section>
      {/if}
    {/each}
  {/if}
</div>

{#if editing !== undefined}
  <HostForm host={editing} onclose={() => (editing = undefined)} />
{/if}
{#if editingGroup !== undefined}
  <GroupForm group={editingGroup} onclose={() => (editingGroup = undefined)} />
{/if}

{#if drag?.started}
  <div class="ghost card" style="left: {drag.x - drag.offX}px; top: {drag.y - drag.offY}px; width: {drag.w}px">
    <div class="top">
      <div class="badge {drag.host.env}"><Logo id={hostIcon(drag.host)} size={16} /></div>
      <div class="grow info">
        <div class="name ellipsis">{drag.host.name}</div>
        <div class="addr mono ellipsis">{drag.host.user}@{drag.host.hostname}</div>
      </div>
    </div>
    <div class="ghost-to">
      {drag.over === undefined || drag.over === (drag.host.group ?? null) ? "Drop on a group" : `→ ${drag.over ?? "No group"}`}
    </div>
  </div>
{/if}

<svelte:window onpointermove={dragMove} onpointerup={dragEnd} onpointercancel={() => (drag = null)} onclick={(e) => importOpen && !(e.target as HTMLElement).closest(".menu-wrap") && (importOpen = false)} />

<style>
  .zone {
    margin: 0 -10px;
    padding: 0 10px 10px;
    border-radius: var(--radius);
    border: 2px dashed transparent;
    transition: background 0.15s, border-color 0.15s;
  }
  .zone.target {
    border-color: color-mix(in srgb, var(--line-strong) 70%, transparent);
  }
  .zone.over {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 7%, transparent);
  }
  .is-dragging {
    user-select: none;
    cursor: grabbing;
  }
  .is-dragging .host {
    pointer-events: none;
  }
  /* La linea que marca donde cae la tarjeta que se arrastra. */
  .host.ins-before {
    box-shadow: inset 4px 0 0 var(--accent);
  }
  .host.ins-after {
    box-shadow: inset -4px 0 0 var(--accent);
  }
  .host.dragging {
    opacity: 0.3;
    transform: scale(0.97);
  }
  .grip {
    display: grid;
    margin: 0 -6px 0 -4px;
    color: var(--text-3);
    cursor: grab;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .host:hover .grip {
    opacity: 1;
  }
  .drop-hint {
    display: grid;
    place-items: center;
    min-height: 92px;
    border-radius: var(--radius);
    border: 2px dashed var(--line-strong);
    color: var(--text-3);
    font-size: 12.5px;
  }
  .zone.over .drop-hint {
    border-color: var(--accent);
    color: var(--accent);
  }
  .ghost {
    position: fixed;
    z-index: 85;
    pointer-events: none;
    padding: 13px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    transform: rotate(2deg) scale(1.03);
    box-shadow: 0 24px 50px -16px rgba(0, 0, 0, 0.7);
    border-color: var(--accent);
    opacity: 0.96;
  }
  .ghost-to {
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
  }
  .search {
    width: 220px;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    transform-origin: top right;
    position: absolute;
    top: 38px;
    right: 0;
    z-index: 20;
    width: 320px;
    padding: 5px;
    background: var(--sidebar);
    box-shadow: var(--pop-shadow);
    display: flex;
    flex-direction: column;
  }
  .menu button {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    text-align: left;
  }
  .menu button:hover {
    background: var(--hover);
  }
  .menu button :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--accent);
  }
  .mi {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .mi b {
    font-weight: 600;
  }
  .mi small {
    font-size: 11.5px;
    color: var(--text-3);
    line-height: 1.35;
  }
  .file-over {
    outline: 2px dashed var(--accent);
    outline-offset: -6px;
    border-radius: var(--radius);
  }
  .group-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .open-all {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    margin-left: 2px;
    border-radius: var(--radius-sm);
    font-size: 12px;
    color: var(--text-3);
    transition: background 0.12s, color 0.12s;
  }
  .open-all:hover {
    background: var(--hover);
    color: var(--text);
  }
  .group-head .section-title {
    margin: 18px 0 10px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
    /* Todas las tarjetas de una fila con la misma altura; los botones quedan abajo. */
    align-items: stretch;
    gap: 12px;
  }
  .host {
    padding: 13px;
    display: flex;
    flex-direction: column;
    gap: 9px;
    cursor: default;
    transition: border-color 0.15s, transform 0.12s;
  }
  .host:hover {
    border-color: var(--line-strong);
  }
  :global([data-theme="poolsuite"]) .host:hover {
    transform: translate(-1px, -1px);
    box-shadow: 4px 4px 0 var(--line);
  }
  .top {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .badge {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    flex: none;
    background: color-mix(in srgb, var(--env-dev) 18%, transparent);
    color: var(--env-dev);
  }
  /* Sin ambiente: neutro, con el color del texto. */
  .badge.none {
    background: var(--panel-2);
    color: var(--text-2);
  }
  .badge.prod {
    background: color-mix(in srgb, var(--env-prod) 18%, transparent);
    color: var(--env-prod);
  }
  .badge.test {
    background: color-mix(in srgb, var(--env-test) 18%, transparent);
    color: var(--env-test);
  }
  .info {
    min-width: 0;
  }
  .name {
    font-weight: 620;
  }
  .addr {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .actions {
    display: flex;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .host:hover .actions,
  .host:focus-within .actions {
    opacity: 1;
  }
  .desc {
    font-size: 12px;
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .connect-row {
    display: flex;
    gap: 6px;
    margin-top: auto;
  }
</style>
