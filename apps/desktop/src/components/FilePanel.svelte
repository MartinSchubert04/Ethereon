<script lang="ts" module>
  import type { FileEntry } from "../lib/api";

  /** Lo que un panel necesita de su disco: el local o el del servidor. */
  export interface Fs {
    /** El disco de esta PC o el de un servidor por SFTP. */
    kind: "local" | "remote";
    label: string;
    sep: "/" | "\\";
    list(path: string): Promise<FileEntry[]>;
    mkdir(path: string): Promise<void>;
    rename(from: string, to: string): Promise<void>;
    remove(paths: string[]): Promise<void>;
    home(): Promise<string>;
    /** Ruta "real" (resuelve ~, .., enlaces). Solo el remoto la necesita. */
    real?(path: string): Promise<string>;
  }

  export function join(fs: Fs, base: string, name: string): string {
    if (!base) return name; // la lista de unidades
    return base.endsWith(fs.sep) ? base + name : base + fs.sep + name;
  }

  export function parent(fs: Fs, path: string): string {
    if (fs.sep === "/") {
      const p = path.replace(/\/+$/, "");
      const i = p.lastIndexOf("/");
      return i <= 0 ? "/" : p.slice(0, i);
    }
    // Windows: "C:\" sube a la lista de unidades ("").
    if (/^[A-Za-z]:\\?$/.test(path)) return "";
    const p = path.replace(/\\+$/, "");
    const i = p.lastIndexOf("\\");
    const up = p.slice(0, i);
    return /^[A-Za-z]:$/.test(up) ? up + "\\" : up;
  }

  export function human(n: number): string {
    if (n < 1024) return `${n} B`;
    const u = ["KB", "MB", "GB", "TB"];
    let i = -1;
    do {
      n /= 1024;
      i++;
    } while (n >= 1024 && i < u.length - 1);
    return `${n < 10 ? n.toFixed(1) : Math.round(n)} ${u[i]}`;
  }
</script>

<script lang="ts">
  import {
    ArrowUp,
    Eye,
    EyeOff,
    File,
    Folder,
    FolderPlus,
    HardDrive,
    House,
    Link2,
    RefreshCw,
    Search,
    Server,
    Laptop,
  } from "@lucide/svelte";
  import { tick } from "svelte";
  import { errText } from "../lib/api";
  import { ask } from "../lib/confirm.svelte";
  import { toast } from "../lib/state.svelte";
  import { pop } from "../lib/motion";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import Select from "./Select.svelte";

  let {
    fs,
    slot,
    sources = [],
    source = null,
    onsource,
    otherLabel = "the other side",
    active = false,
    onfocus,
    ontransfer,
    ondropitems,
  }: {
    fs: Fs;
    /** Que panel es (izquierdo o derecho): es lo que se mira al soltar algo. */
    slot: "left" | "right";
    /** Para cambiar el origen desde el titulo: esta PC (null) o un host. */
    sources?: { value: string | null; label: string; hint?: string }[];
    source?: string | null;
    onsource?: (v: string | null) => void;
    otherLabel?: string;
    active?: boolean;
    onfocus?: () => void;
    /** Mandar lo elegido al otro panel. */
    ontransfer?: (paths: string[]) => void;
    /** Algo se solto sobre otro panel: `target` es su lado y la carpeta (o null = la actual). */
    ondropitems?: (paths: string[], targetSide: string, targetDir: string | null) => void;
  } = $props();

  let path = $state("");
  let typed = $state("");
  let entries = $state<FileEntry[]>([]);
  let loading = $state(false);
  let error = $state("");
  let filter = $state("");
  let showHidden = $state(false);
  let sort = $state<{ by: "name" | "size" | "mtime"; desc: boolean }>({ by: "name", desc: false });
  let selected = $state<string[]>([]);
  let anchor = -1;
  let renaming = $state<string | null>(null);
  let renameTo = $state("");
  let menu = $state<{ x: number; y: number } | null>(null);
  let root: HTMLDivElement;
  let listEl = $state<HTMLDivElement>();

  const shown = $derived.by(() => {
    const q = filter.toLowerCase();
    const l = entries.filter((e) => (showHidden || !e.name.startsWith(".")) && (!q || e.name.toLowerCase().includes(q)));
    const k = sort.by;
    l.sort((a, b) => {
      if (a.dir !== b.dir) return a.dir ? -1 : 1; // carpetas primero, siempre
      const v =
        k === "name"
          ? a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" })
          : k === "size"
            ? a.size - b.size
            : (a.mtime ?? 0) - (b.mtime ?? 0);
      return sort.desc ? -v : v;
    });
    return l;
  });

  const crumbs = $derived.by(() => {
    if (!path) return [];
    const parts = path.split(fs.sep).filter(Boolean);
    const out: { name: string; path: string }[] = [];
    let acc = fs.sep === "/" ? "/" : "";
    if (fs.sep === "/") out.push({ name: "/", path: "/" });
    for (const p of parts) {
      acc = fs.sep === "/" ? join(fs, acc, p) : acc ? join(fs, acc, p) : p + "\\";
      out.push({ name: p, path: acc });
    }
    return out;
  });

  export async function go(p: string, keepSelection = false) {
    loading = true;
    error = "";
    try {
      if (fs.real && p) p = await fs.real(p);
      const l = await fs.list(p);
      entries = l;
      if (p !== path) {
        filter = "";
        if (!keepSelection) selected = [];
      }
      path = p;
      typed = p;
      selected = selected.filter((n) => l.some((e) => e.name === n));
    } catch (e) {
      error = errText(e);
      typed = path;
    } finally {
      loading = false;
    }
  }

  export const refresh = () => go(path, true);
  export const current = () => path;
  export const names = () => entries.map((e) => e.name);
  export const selection = () => selPaths();

  export async function start() {
    await go(await fs.home());
  }

  const full = (name: string) => join(fs, path, name);
  const selPaths = () => selected.map(full);

  function open(e: FileEntry) {
    if (e.dir) go(full(e.name));
    else ontransfer?.([full(e.name)]);
  }

  function clickRow(ev: MouseEvent, e: FileEntry, i: number) {
    root.focus();
    if (ev.shiftKey && anchor >= 0) {
      const [a, b] = [Math.min(anchor, i), Math.max(anchor, i)];
      selected = shown.slice(a, b + 1).map((x) => x.name);
    } else if (ev.ctrlKey || ev.metaKey) {
      selected = selected.includes(e.name) ? selected.filter((n) => n !== e.name) : [...selected, e.name];
      anchor = i;
    } else {
      selected = [e.name];
      anchor = i;
    }
  }

  function moveSel(d: number, extend: boolean) {
    if (!shown.length) return;
    const cur = selected.length ? shown.findIndex((x) => x.name === selected[selected.length - 1]) : -1;
    const i = Math.min(shown.length - 1, Math.max(0, cur + d));
    if (extend && anchor >= 0) {
      const [a, b] = [Math.min(anchor, i), Math.max(anchor, i)];
      selected = shown.slice(a, b + 1).map((x) => x.name);
      selected = [...selected.filter((n) => n !== shown[i].name), shown[i].name];
    } else {
      selected = [shown[i].name];
      anchor = i;
    }
    tick().then(() => listEl?.querySelector(`[data-name="${CSS.escape(shown[i].name)}"]`)?.scrollIntoView({ block: "nearest" }));
  }

  async function newFolder() {
    menu = null;
    let name = "New folder";
    for (let n = 2; entries.some((e) => e.name === name); n++) name = `New folder ${n}`;
    try {
      await fs.mkdir(full(name));
      await refresh();
      selected = [name];
      startRename(name);
    } catch (e) {
      toast(errText(e), true);
    }
  }

  async function startRename(name?: string) {
    menu = null;
    const n = name ?? selected[0];
    if (!n) return;
    renaming = n;
    renameTo = n;
    await tick();
    const input = listEl?.querySelector<HTMLInputElement>("input.rename");
    input?.focus();
    // Selecciona el nombre sin la extension, como el Explorador.
    const dot = n.lastIndexOf(".");
    input?.setSelectionRange(0, dot > 0 ? dot : n.length);
  }

  async function commitRename() {
    const from = renaming;
    renaming = null;
    const to = renameTo.trim();
    if (!from || !to || to === from) return root.focus();
    try {
      await fs.rename(full(from), full(to));
      await refresh();
      selected = [to];
    } catch (e) {
      toast(errText(e), true);
    }
    root.focus();
  }

  async function removeSel() {
    menu = null;
    if (!selected.length) return;
    const what = selected.length === 1 ? `"${selected[0]}"` : `${selected.length} items`;
    const dirs = entries.filter((e) => selected.includes(e.name) && e.dir).length;
    const ok = await ask({
      title: `Delete ${what}?`,
      body: `${dirs ? "Folders are deleted with everything inside. " : ""}This cannot be undone${fs.kind === "local" ? " (it does not go to the Recycle Bin)" : ""}.`,
      confirm: "Delete",
      danger: true,
    });
    root.focus();
    if (!ok) return;
    try {
      await fs.remove(selPaths());
      toast(`Deleted ${what}`);
    } catch (e) {
      toast(errText(e), true);
    }
    refresh();
  }

  // Escribir salta al primer elemento que empieza asi, como el Explorador: "d" va a la
  // primera "d...", otra "d" a la siguiente, y "do" rapido a la primera "do...".
  let seek = "";
  let seekAt = 0;
  function typeAhead(ch: string) {
    const now = Date.now();
    const fresh = now - seekAt > 800;
    seekAt = now;
    seek = fresh ? ch : seek + ch;
    const q = seek.toLowerCase();
    const same = [...q].every((c) => c === q[0]);
    const cur = shown.findIndex((x) => x.name === selected[selected.length - 1]);
    // Misma letra repetida: recorre las que empiezan con esa letra.
    const prefix = same ? q[0] : q;
    const from = same ? cur + 1 : Math.max(cur, 0);
    const order = [...shown.slice(from), ...shown.slice(0, from)];
    const hit = order.find((x) => x.name.toLowerCase().startsWith(prefix));
    if (!hit) return;
    const i = shown.indexOf(hit);
    selected = [hit.name];
    anchor = i;
    tick().then(() => listEl?.querySelector(`[data-name="${CSS.escape(hit.name)}"]`)?.scrollIntoView({ block: "nearest" }));
  }

  function onKey(e: KeyboardEvent) {
    if (renaming || (e.target as HTMLElement).closest("input")) return;
    const k = e.key;
    const typing = k.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey;
    // Un espacio en medio de un nombre que se esta escribiendo es parte del nombre.
    if (typing && (k !== " " || (seek && Date.now() - seekAt < 800))) {
      typeAhead(k);
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (k === "ArrowDown") moveSel(1, e.shiftKey);
    else if (k === "ArrowUp") moveSel(-1, e.shiftKey);
    else if (k === "Home") moveSel(-1e9, e.shiftKey);
    else if (k === "End") moveSel(1e9, e.shiftKey);
    else if (k === " " || (e.ctrlKey && k === "Enter")) selected.length && ontransfer?.(selPaths());
    else if (k === "Tab" && !e.shiftKey && !e.ctrlKey) {
      // Tab salta al otro panel (no recorre los botones de este).
      const other = root.closest(".panes")?.querySelector<HTMLElement>(`[data-side]:not([data-side="${slot}"])`);
      if (!other) return;
      other.focus();
    } else if (k === "Enter") {
      const it = shown.find((x) => x.name === selected[selected.length - 1]);
      if (it) open(it);
    } else if (k === "Backspace" || (e.altKey && k === "ArrowLeft")) go(parent(fs, path));
    else if (k === "Delete") removeSel();
    else if (k === "F2") startRename();
    else if (k === "F5") refresh();
    else if (e.ctrlKey && e.code === "KeyA") selected = shown.map((x) => x.name);
    else if (e.ctrlKey && e.shiftKey && e.code === "KeyN") newFolder();
    else if (e.ctrlKey && e.code === "KeyF") root.querySelector<HTMLInputElement>("input.filter")?.focus();
    else if (e.ctrlKey && e.code === "KeyH") showHidden = !showHidden;
    else if (k === "Escape") (selected = []), (menu = null);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  function onContext(e: MouseEvent, entry?: FileEntry) {
    e.preventDefault();
    onfocus?.();
    if (entry && !selected.includes(entry.name)) selected = [entry.name];
    if (!entry) selected = [];
    const w = 210, h = 240;
    menu = { x: Math.min(e.clientX, innerWidth - w - 8), y: Math.min(e.clientY, innerHeight - h - 8) };
  }

  // ---- arrastrar al otro panel (con el puntero: el arrastre HTML lo come Tauri en Windows)

  let drag = $state<{ x: number; y: number; started: boolean; paths: string[]; count: number } | null>(null);
  let downAt = { x: 0, y: 0 };

  function onRowDown(ev: PointerEvent, e: FileEntry, i: number) {
    if (ev.button !== 0 || renaming) return;
    // Al apretar sobre algo no elegido se elige ya (para poder arrastrarlo). Con Ctrl o
    // Shift decide el clic, si no se alternaria dos veces.
    if (!selected.includes(e.name) && !ev.ctrlKey && !ev.shiftKey && !ev.metaKey) clickRow(ev, e, i);
    downAt = { x: ev.clientX, y: ev.clientY };
    drag = { x: ev.clientX, y: ev.clientY, started: false, paths: [], count: 0 };
  }

  function onMove(ev: PointerEvent) {
    if (!drag) return;
    if (!drag.started && Math.hypot(ev.clientX - downAt.x, ev.clientY - downAt.y) > 6) {
      drag.started = true;
      drag.paths = selPaths();
      drag.count = selected.length;
    }
    drag.x = ev.clientX;
    drag.y = ev.clientY;
    if (drag.started) {
      document.querySelectorAll(".fp.drop-over, .row.drop-over").forEach((n) => n.classList.remove("drop-over"));
      const t = document.elementFromPoint(ev.clientX, ev.clientY);
      const panel = t?.closest<HTMLElement>("[data-side]");
      if (panel && panel.dataset.side !== slot) {
        const row = t?.closest<HTMLElement>(".row[data-dir]");
        (row ?? panel).classList.add("drop-over");
      }
    }
  }

  function onUp(ev: PointerEvent) {
    if (!drag) return;
    const d = drag;
    drag = null;
    document.querySelectorAll(".drop-over").forEach((n) => n.classList.remove("drop-over"));
    if (!d.started) return;
    const t = document.elementFromPoint(ev.clientX, ev.clientY);
    const panel = t?.closest<HTMLElement>("[data-side]");
    if (!panel || panel.dataset.side === slot) return;
    const row = t?.closest<HTMLElement>(".row[data-dir]");
    ondropitems?.(d.paths, panel.dataset.side!, row?.dataset.path ?? null);
  }

  const date = (s: number | null) =>
    s ? new Date(s * 1000).toLocaleString(undefined, { year: "2-digit", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false }) : "";

  function sortBy(by: "name" | "size" | "mtime") {
    sort = sort.by === by ? { by, desc: !sort.desc } : { by, desc: by !== "name" };
  }

  const total = $derived(shown.filter((e) => selected.includes(e.name)).reduce((a, e) => a + (e.dir ? 0 : e.size), 0));
</script>

<svelte:window
  onpointermove={onMove}
  onpointerup={onUp}
  onpointerdown={(e) => menu && !(e.target as HTMLElement).closest(".ctx") && (menu = null)}
/>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="fp card"
  class:active
  data-side={slot}
  data-kind={fs.kind}
  tabindex="0"
  role="region"
  aria-label={fs.label}
  bind:this={root}
  onkeydown={onKey}
  onfocusin={() => onfocus?.()}
  onpointerdown={() => onfocus?.()}
>
  <header>
    {#if sources.length}
      <div class="who pick" title="Change what this side shows">
        <Select ghost value={source} options={sources} onchange={(v) => onsource?.(v)}>
          {#snippet lead()}{#if fs.kind === "local"}<Laptop size={14} />{:else}<Server size={14} />{/if}{/snippet}
        </Select>
      </div>
    {:else}
      <span class="who">
        {#if fs.kind === "local"}<Laptop size={14} />{:else}<Server size={14} />{/if}
        <span class="ellipsis">{fs.label}</span>
      </span>
    {/if}
    <span class="grow"></span>
    <label class="find inbox">
      <Search size={13} />
      <input class="filter" placeholder="Filter" bind:value={filter} onkeydown={(e) => e.key === "Escape" && ((filter = ""), root.focus())} />
    </label>
    <button class="icon-btn" title={showHidden ? "Hide hidden files (Ctrl H)" : "Show hidden files (Ctrl H)"} onclick={() => (showHidden = !showHidden)}>
      {#if showHidden}<Eye size={15} />{:else}<EyeOff size={15} />{/if}
    </button>
    <button class="icon-btn" title="New folder (Ctrl Shift N)" onclick={newFolder}><FolderPlus size={15} /></button>
    <button class="icon-btn" title="Refresh (F5)" onclick={refresh}><RefreshCw size={15} class={loading ? "spin" : ""} /></button>
  </header>

  <div class="pathbar">
    <button class="icon-btn" title="Up (Backspace)" disabled={fs.kind === "remote" ? path === "/" : !path} onclick={() => go(parent(fs, path))}><ArrowUp size={15} /></button>
    <button class="icon-btn" title={fs.kind === "local" ? "Start folder (Settings → Files)" : "Home folder"} onclick={start}><House size={15} /></button>
    <div class="crumbs">
      <input
        class="mono tight"
        bind:value={typed}
        spellcheck="false"
        onkeydown={(e) => {
          if (e.key === "Enter") (go(typed.trim()), root.focus());
          if (e.key === "Escape") ((typed = path), root.focus());
        }}
        onblur={() => (typed = path)}
        title="Type a path and press Enter"
      />
      <div class="trail" aria-hidden="true">
        {#if fs.kind === "local"}
          <button class="crumb" onpointerdown={(e) => (e.preventDefault(), go(""))}><HardDrive size={12} /></button>
        {/if}
        {#each crumbs as c, i}
          {#if i > 0 || fs.kind === "local"}<span class="slash">›</span>{/if}
          <button class="crumb" onpointerdown={(e) => (e.preventDefault(), go(c.path))}>{c.name}</button>
        {/each}
      </div>
    </div>
  </div>

  <div class="cols">
    <button class:on={sort.by === "name"} onclick={() => sortBy("name")}>Name {sort.by === "name" ? (sort.desc ? "↓" : "↑") : ""}</button>
    <button class="r" class:on={sort.by === "size"} onclick={() => sortBy("size")}>Size {sort.by === "size" ? (sort.desc ? "↓" : "↑") : ""}</button>
    <button class="r" class:on={sort.by === "mtime"} onclick={() => sortBy("mtime")}>Modified {sort.by === "mtime" ? (sort.desc ? "↓" : "↑") : ""}</button>
  </div>

  <div class="list" bind:this={listEl} role="listbox" aria-multiselectable="true" tabindex="-1" oncontextmenu={(e) => onContext(e)}>
    {#if error}
      <div class="msg bad">{error}</div>
    {:else if !shown.length && !loading}
      <div class="msg faint">{filter ? "Nothing matches." : "Empty folder. Drop files here."}</div>
    {/if}
    {#each shown as e, i (e.name)}
      <div
        class="row"
        class:sel={selected.includes(e.name)}
        class:dim={e.name.startsWith(".")}
        data-name={e.name}
        data-dir={e.dir ? "" : undefined}
        data-path={e.dir ? full(e.name) : undefined}
        role="option"
        aria-selected={selected.includes(e.name)}
        tabindex="-1"
        onclick={(ev) => clickRow(ev, e, i)}
        ondblclick={() => open(e)}
        onpointerdown={(ev) => onRowDown(ev, e, i)}
        oncontextmenu={(ev) => (ev.stopPropagation(), onContext(ev, e))}
        onkeydown={() => {}}
      >
        <span class="ic" class:folder={e.dir}>
          {#if e.dir}<Folder size={15} />{:else}<File size={15} />{/if}
          {#if e.link}<span class="lnk"><Link2 size={9} /></span>{/if}
        </span>
        {#if renaming === e.name}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="rename tight"
            bind:value={renameTo}
            onkeydown={(ev) => {
              ev.stopPropagation();
              if (ev.key === "Enter") commitRename();
              if (ev.key === "Escape") ((renaming = null), root.focus());
            }}
            onblur={commitRename}
            onclick={(ev) => ev.stopPropagation()}
            onpointerdown={(ev) => ev.stopPropagation()}
          />
        {:else}
          <span class="name ellipsis">{e.name}</span>
        {/if}
        <span class="size">{e.dir ? "" : human(e.size)}</span>
        <span class="date">{date(e.mtime)}</span>
      </div>
    {/each}
  </div>

  <footer>
    {#if selected.length}
      {selected.length} selected{total ? ` · ${human(total)}` : ""}
    {:else}
      {shown.length} {shown.length === 1 ? "item" : "items"}
    {/if}
  </footer>
</div>

{#if menu}
  <div class="ctx card" style="left: {menu.x}px; top: {menu.y}px" transition:pop={{ start: 0.95 }} role="menu">
    {#if selected.length}
      {@const one = selected.length === 1 ? entries.find((e) => e.name === selected[0]) : null}
      {#if one?.dir}<button role="menuitem" onclick={() => ((menu = null), one && open(one))}>Open<kbd>Enter</kbd></button>{/if}
      <button role="menuitem" onclick={() => ((menu = null), ontransfer?.(selPaths()))}>
        <span class="ellipsis">Copy to {otherLabel}</span><kbd>Space</kbd>
      </button>
      {#if selected.length === 1}<button role="menuitem" onclick={() => startRename()}>Rename<kbd>F2</kbd></button>{/if}
      <button role="menuitem" onclick={() => ((menu = null), writeText(selPaths().join("\n")), toast("Path copied"))}>Copy path</button>
      <button role="menuitem" class="danger" onclick={removeSel}>Delete<kbd>Del</kbd></button>
      <hr />
    {/if}
    <button role="menuitem" onclick={newFolder}>New folder<kbd>Ctrl Shift N</kbd></button>
    <button role="menuitem" onclick={() => ((menu = null), refresh())}>Refresh<kbd>F5</kbd></button>
    <button role="menuitem" onclick={() => ((menu = null), (showHidden = !showHidden))}>{showHidden ? "Hide" : "Show"} hidden files<kbd>Ctrl H</kbd></button>
  </div>
{/if}

{#if drag?.started}
  <div class="ghost card" style="left: {drag.x + 14}px; top: {drag.y + 10}px">
    {#if drag.count === 1}<File size={13} /> {selected[0]}{:else}<File size={13} /> {drag.count} items{/if}
  </div>
{/if}

<style>
  .fp {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    outline: none;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .fp.active {
    border-color: color-mix(in srgb, var(--accent) 55%, var(--line));
  }
  .fp:global(.drop-over) {
    box-shadow: 0 0 0 2px var(--accent) inset;
  }
  header {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 8px 6px 12px;
  }
  .who {
    display: flex;
    align-items: center;
    gap: 7px;
    font-weight: 620;
    min-width: 0;
    color: var(--text);
  }
  .who.pick {
    min-width: 0;
    margin-left: -6px;
  }
  .who :global(svg) {
    color: var(--accent);
    flex: none;
  }
  .find {
    height: 30px;
    padding: 0 8px;
    width: 140px;
    margin-right: 4px;
  }
  .find input {
    height: 26px;
    font-size: 12px;
  }
  .pathbar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px 8px;
  }
  .crumbs {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 30px;
  }
  .crumbs input {
    position: absolute;
    inset: 0;
    height: 30px;
    font-size: 12px;
    opacity: 0;
  }
  .crumbs input:focus {
    opacity: 1;
  }
  .trail {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px;
    border-radius: var(--radius-sm);
    background: var(--panel-2);
    overflow: hidden;
    white-space: nowrap;
    pointer-events: none;
    justify-content: flex-end; /* si no entra, se ve el final de la ruta */
  }
  .trail > * {
    flex: none;
  }
  .crumbs input:focus + .trail {
    visibility: hidden;
  }
  .crumb {
    pointer-events: auto;
    display: inline-flex;
    align-items: center;
    font-size: 12px;
    padding: 2px 5px;
    border-radius: 5px;
    color: var(--text-2);
  }
  .crumb:hover {
    background: var(--hover);
    color: var(--text);
  }
  .slash {
    color: var(--text-3);
    font-size: 12px;
  }
  .cols,
  .row {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) 72px 120px;
    align-items: center;
    gap: 8px;
  }
  .cols {
    grid-template-columns: minmax(0, 1fr) 72px 120px;
    padding: 4px 12px 4px 42px;
    border-top: var(--border) solid var(--line);
    border-bottom: var(--border) solid var(--line);
  }
  .cols button {
    text-align: left;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .cols button.on {
    color: var(--text-2);
  }
  .cols .r,
  .size,
  .date {
    text-align: right;
  }
  .list {
    flex: 1;
    overflow: hidden auto;
    padding: 4px;
    outline: none;
    user-select: none;
  }
  .row {
    height: 30px;
    padding: 0 8px;
    border-radius: var(--radius-sm);
    cursor: default;
    font-size: 13px;
  }
  .row:hover {
    background: var(--hover);
  }
  .row.sel {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .fp:not(.active) .row.sel {
    background: color-mix(in srgb, var(--text) 9%, transparent);
  }
  .row:global(.drop-over) {
    box-shadow: 0 0 0 2px var(--accent) inset;
  }
  .row.dim .name {
    opacity: 0.6;
  }
  .ic {
    position: relative;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .ic.folder {
    color: var(--accent);
  }
  .lnk {
    position: absolute;
    right: -3px;
    bottom: -3px;
    display: grid;
    background: var(--panel);
    border-radius: 50%;
    color: var(--text-2);
  }
  .size,
  .date {
    font-size: 11.5px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rename {
    height: 24px;
    padding: 0 6px;
    font-size: 13px;
  }
  .msg {
    padding: 16px;
    font-size: 13px;
  }
  .msg.bad {
    color: var(--danger);
  }
  footer {
    padding: 6px 12px;
    font-size: 11.5px;
    color: var(--text-3);
    border-top: var(--border) solid var(--line);
  }
  :global(.spin) {
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .ctx {
    position: fixed;
    z-index: 95;
    min-width: 210px;
    padding: 5px;
    display: flex;
    flex-direction: column;
    background: var(--sidebar);
    box-shadow: var(--pop-shadow);
    transform-origin: top left;
  }
  .ctx button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 7px 10px;
    border-radius: var(--radius-sm);
    font-size: 13px;
    text-align: left;
  }
  .ctx button:hover {
    background: var(--hover);
  }
  .ctx button.danger {
    color: var(--danger);
  }
  .ctx kbd {
    font: inherit;
    font-size: 11px;
    color: var(--text-3);
  }
  .ctx hr {
    border: 0;
    border-top: var(--border) solid var(--line);
    margin: 4px 2px;
  }
  .ghost {
    position: fixed;
    z-index: 96;
    pointer-events: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    font-size: 12.5px;
    background: var(--sidebar);
    box-shadow: var(--pop-shadow);
  }
  @media (max-width: 900px) {
    .date,
    .cols button:last-child {
      display: none;
    }
    .row {
      grid-template-columns: 22px minmax(0, 1fr) 72px;
    }
    .cols {
      grid-template-columns: minmax(0, 1fr) 72px;
    }
  }
</style>
