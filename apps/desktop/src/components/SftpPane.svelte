<script lang="ts">
  // Pestaña de archivos con dos lados. Cada lado es esta PC o cualquier host, y se cambia
  // desde su titulo: subir, bajar, de un servidor a otro o dentro de esta PC. Se pasa
  // arrastrando entre paneles, desde el Explorador de Windows, con Espacio o con las
  // flechas del medio. Las transferencias corren en segundo plano.
  import { onDestroy, onMount, tick } from "svelte";
  import { Channel } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { desktopDir } from "@tauri-apps/api/path";
  import { ArrowLeft, ArrowRight, Check, CircleAlert, Laptop, LoaderCircle, RotateCw, Server, X } from "@lucide/svelte";
  import FilePanel, { human, type Fs } from "./FilePanel.svelte";
  import Select from "./Select.svelte";
  import { api, errText, type XferEvent } from "../lib/api";
  import { testHost } from "../lib/hostkey.svelte";
  import { app, toast, type TermTab } from "../lib/state.svelte";
  import { ask } from "../lib/confirm.svelte";
  import { rise } from "../lib/motion";
  import { prefs } from "../lib/prefs.svelte";

  let { tab, active }: { tab: TermTab; active: boolean } = $props();

  type Slot = "left" | "right";
  interface Side {
    /** null = esta PC */
    host: string | null;
    id: number;
    home: string;
    status: "connecting" | "ready" | "failed";
    failure: string;
    /** Cambia cuando cambia el origen: el panel se arma de nuevo. */
    gen: number;
  }
  let gen = 0;
  const side = (host: string | null): Side => ({ host, id: 0, home: "/", status: host ? "connecting" : "ready", failure: "", gen: ++gen });
  // svelte-ignore state_referenced_locally
  let sides = $state<Record<Slot, Side>>({ left: side(null), right: side(tab.host) });
  let panels = $state<Record<Slot, FilePanel | undefined>>({ left: undefined, right: undefined });
  let focused = $state<Slot>("right");
  let wrap: HTMLDivElement;
  const other = (s: Slot): Slot => (s === "left" ? "right" : "left");
  const label = (s: Slot) => sides[s].host ?? "This PC";

  const sources = $derived([
    { value: null as string | null, label: "This PC" },
    ...(app.snap?.hosts ?? []).map((h) => ({ value: h.name as string | null, label: h.name, hint: h.group ?? "" })),
  ]);

  // ---- origen de cada lado

  // Donde arranca esta PC (Settings → Files). Si la carpeta no existe, la del usuario.
  async function localStart(): Promise<string> {
    const f = prefs.files;
    try {
      const p = f.localStart === "custom" && f.customPath ? f.customPath : f.localStart === "home" ? "" : await desktopDir();
      if (p) {
        await api.localList(p);
        return p;
      }
    } catch {}
    return api.localHome();
  }

  function makeFs(slot: Slot, host: string | null): Fs {
    if (host === null)
      return {
        kind: "local",
        label: "This PC",
        sep: "\\",
        list: api.localList,
        mkdir: api.localMkdir,
        rename: api.localRename,
        remove: api.localRemove,
        home: localStart,
      };
    // El id se lee en cada llamada: al reconectar cambia sin rearmar el panel.
    return {
      kind: "remote",
      label: host,
      sep: "/",
      list: (p) => api.sftpList(sides[slot].id, p),
      mkdir: (p) => api.sftpMkdir(sides[slot].id, p),
      rename: (a, b) => api.sftpRename(sides[slot].id, a, b),
      remove: (p) => api.sftpRemove(sides[slot].id, p),
      home: () => Promise.resolve(sides[slot].home),
      real: (p) => api.sftpRealpath(sides[slot].id, p),
    };
  }
  const fsLeft = $derived(makeFs("left", sides.left.host));
  const fsRight = $derived(makeFs("right", sides.right.host));

  /** Conecta un lado (o reconecta). Con `keep`, el panel se queda en la carpeta en la que estaba. */
  async function open(slot: Slot, keep = false) {
    const s = sides[slot];
    if (s.host === null) {
      s.status = "ready";
    } else {
      const host = s.host;
      if (!keep) s.status = "connecting";
      s.failure = "";
      if (!(await testHost(host, true))) {
        s.status = "failed";
        s.failure = "The host key was not accepted.";
        return sync();
      }
      try {
        const o = await api.sftpOpen(host);
        if (sides[slot].host !== host) return void api.sftpClose(o.id); // se cambio mientras tanto
        if (s.id) api.sftpClose(s.id);
        s.id = o.id;
        s.home = o.home;
        s.status = "ready";
      } catch (e) {
        s.status = "failed";
        s.failure = errText(e);
        return sync();
      }
    }
    sync();
    await tick();
    const p = panels[slot];
    if (keep && p?.current()) await p.refresh();
    else await p?.start();
  }

  function setSource(slot: Slot, host: string | null) {
    if (host === sides[slot].host) return;
    if (sides[slot].id) api.sftpClose(sides[slot].id);
    sides[slot] = side(host);
    open(slot);
  }

  /** El titulo de la pestaña y si esta "viva" siguen a los dos lados. */
  function sync() {
    const t = app.terms.find((x) => x.key === tab.key);
    if (!t) return;
    const l = sides.left.host,
      r = sides.right.host;
    t.title = l && r ? `${l} ⇄ ${r}` : `${r ?? l ?? "This PC"} · SFTP`;
    t.alive = (["left", "right"] as Slot[]).every((s) => sides[s].status !== "failed");
  }

  // ---- transferencias

  interface Xfer {
    tid: number;
    from: Slot;
    verb: string;
    label: string;
    done: number;
    total: number;
    file: string;
    state: "running" | "done" | "error" | "cancelled";
    message: string;
    started: number;
  }
  let xfers = $state<Xfer[]>([]);

  const base = (p: string) => p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? p;

  /** Pasa `paths` del lado `from` a la carpeta `into` del otro (o a la que tiene abierta). */
  async function transfer(from: Slot, paths: string[], into?: string | null, external = false) {
    const to = other(from);
    const src = sides[from],
      dst = sides[to];
    if (!paths.length || dst.status !== "ready" || (!external && src.status !== "ready")) return;
    const target = panels[to];
    const dest = into ?? target?.current() ?? "";
    if (!dest) return toast(dst.host ? "Open a folder on that side first" : "Open a folder first (not the drive list)", true);
    // Si ya existe algo con el mismo nombre, se pregunta antes de pisarlo.
    if (!into || into === target?.current()) {
      const names = new Set(target?.names() ?? []);
      const clash = paths.map(base).filter((n) => names.has(n));
      if (clash.length) {
        const ok = await ask({
          title: clash.length === 1 ? `Replace "${clash[0]}"?` : `Replace ${clash.length} items?`,
          body: `They already exist in ${dest}. Folders are merged, files are overwritten.`,
          confirm: "Replace",
          danger: true,
        });
        if (!ok) return;
      }
    }
    const fromLocal = external || !src.host;
    const verb = fromLocal && dst.host ? "Upload" : !fromLocal && !dst.host ? "Download" : "Copy";
    const ch = new Channel<XferEvent>();
    xfers = [
      { tid: 0, from, verb, label: paths.length === 1 ? base(paths[0]) : `${paths.length} items`, done: 0, total: 0, file: "", state: "running", message: "", started: Date.now() },
      ...xfers,
    ];
    const item = xfers[0];
    ch.onmessage = (m) => {
      if (m.kind === "progress") {
        item.done = m.done;
        item.total = m.total;
        if (m.file) item.file = m.file;
      } else if (m.kind === "done") {
        item.state = "done";
        item.done = item.total;
        target?.refresh();
      } else {
        item.state = m.message === "cancelled" ? "cancelled" : "error";
        item.message = m.message;
        target?.refresh();
      }
    };
    try {
      item.tid = await api.filesTransfer(fromLocal ? null : src.id, dst.host ? dst.id : null, paths, dest, ch);
    } catch (e) {
      item.state = "error";
      item.message = errText(e);
    }
  }

  const pct = (x: Xfer) => (x.total ? Math.min(100, (x.done / x.total) * 100) : x.state === "done" ? 100 : 0);
  function speed(x: Xfer) {
    const s = (Date.now() - x.started) / 1000;
    return s > 0.5 ? `${human(x.done / s)}/s` : "";
  }
  const running = $derived(xfers.filter((x) => x.state === "running").length);

  // ---- soltar archivos del Explorador sobre cualquiera de los dos paneles

  let unlisten: (() => void) | undefined;
  let osOver = $state<Slot | null>(null);
  onMount(async () => {
    open("left");
    open("right");
    unlisten = await getCurrentWebview().onDragDropEvent((ev) => {
      if (!active) return;
      const p = ev.payload;
      const at = (pos: { x: number; y: number }) => {
        const r = devicePixelRatio || 1;
        const el = document.elementFromPoint(pos.x / r, pos.y / r);
        return el && wrap.contains(el) ? el : null;
      };
      if (p.type === "over" || p.type === "enter") {
        osOver = ((at(p.position)?.closest<HTMLElement>("[data-side]")?.dataset.side as Slot) ?? null) || null;
      } else if (p.type === "drop") {
        osOver = null;
        const el = at(p.position);
        const slot = el?.closest<HTMLElement>("[data-side]")?.dataset.side as Slot | undefined;
        if (slot) transfer(other(slot), p.paths, el?.closest<HTMLElement>(".row[data-dir]")?.dataset.path ?? null, true);
      } else osOver = null;
    });
  });

  // Ctrl R: un lado que no conecto reintenta; los conectados recargan su carpeta.
  function onRefresh(e: Event) {
    if ((e as CustomEvent).detail !== tab.key) return;
    for (const s of ["left", "right"] as Slot[]) {
      if (sides[s].status === "failed") open(s);
      else if (sides[s].status === "ready") panels[s]?.refresh();
    }
  }
  window.addEventListener("ethereon:refresh", onRefresh);

  onDestroy(() => {
    window.removeEventListener("ethereon:refresh", onRefresh);
    unlisten?.();
    for (const x of xfers) if (x.state === "running" && x.tid) api.sftpCancel(x.tid);
    for (const s of ["left", "right"] as Slot[]) if (sides[s].id) api.sftpClose(sides[s].id);
  });

  // Al bloquear el vault las sesiones dejan de servir; al desbloquear se abren otras y los
  // paneles quedan donde estaban.
  let wasUnlocked = true;
  $effect(() => {
    const now = !!app.status?.unlocked;
    if (now && !wasUnlocked) for (const s of ["left", "right"] as Slot[]) if (sides[s].host) open(s, true);
    wasUnlocked = now;
  });

  // Al volver a la pestaña, el foco va al panel que estaba activo.
  $effect(() => {
    if (active) queueMicrotask(() => wrap?.querySelector<HTMLElement>(`[data-side="${focused}"]`)?.focus());
  });
</script>

{#snippet pane(slot: Slot)}
  {@const s = sides[slot]}
  {#key s.gen}
    {#if s.status === "ready"}
      <FilePanel
        bind:this={panels[slot]}
        fs={slot === "left" ? fsLeft : fsRight}
        {slot}
        {sources}
        source={s.host}
        onsource={(h) => setSource(slot, h)}
        otherLabel={label(other(slot))}
        active={focused === slot}
        onfocus={() => (focused = slot)}
        ontransfer={(p) => transfer(slot, p)}
        ondropitems={(p, target, dir) => target !== slot && transfer(slot, p, dir)}
      />
    {:else}
      <div class="fp-wait card" data-side={slot} in:rise>
        <div class="pick">
          <Select ghost value={s.host} options={sources} onchange={(h) => setSource(slot, h)}>
            {#snippet lead()}{#if s.host}<Server size={14} />{:else}<Laptop size={14} />{/if}{/snippet}
          </Select>
        </div>
        <div class="state">
          {#if s.status === "connecting"}
            <LoaderCircle size={22} class="spin" />
            <div>Opening SFTP on <b>{s.host}</b>...</div>
          {:else}
            <CircleAlert size={22} />
            <div class="bad">{s.failure}</div>
            <button class="btn" onclick={() => open(slot)}><RotateCw size={14} /> Try again</button>
          {/if}
        </div>
      </div>
    {/if}
  {/key}
{/snippet}

<div class="sftp" bind:this={wrap} class:over-left={osOver === "left"} class:over-right={osOver === "right"}>
  <div class="panes">
    {@render pane("left")}
    <div class="mid">
      <button class="icon-btn" title="Copy what is selected on the left to the right (Space)" onclick={() => transfer("left", panels.left?.selection() ?? [])}
        ><ArrowRight size={16} /></button
      >
      <button class="icon-btn" title="Copy what is selected on the right to the left (Space)" onclick={() => transfer("right", panels.right?.selection() ?? [])}
        ><ArrowLeft size={16} /></button
      >
    </div>
    {@render pane("right")}
  </div>

  {#if xfers.length}
    <div class="xfers card" in:rise>
      <div class="xhead">
        <span>Transfers{running ? ` · ${running} running` : ""}</span>
        <span class="grow"></span>
        {#if xfers.some((x) => x.state !== "running")}
          <button class="btn sm ghost" onclick={() => (xfers = xfers.filter((x) => x.state === "running"))}>Clear finished</button>
        {/if}
      </div>
      <div class="xlist">
        {#each xfers as x (x)}
          <div class="xfer" class:bad={x.state === "error"}>
            <span class="dir">{#if x.from === "left"}<ArrowRight size={13} />{:else}<ArrowLeft size={13} />{/if}</span>
            <span class="xl ellipsis" title={x.message || x.file}>
              {x.verb} {x.label}
              {#if x.state === "running" && x.file && x.file !== x.label}<span class="faint"> · {x.file}</span>{/if}
              {#if x.state === "error"}<span> · {x.message}</span>{/if}
              {#if x.state === "cancelled"}<span class="faint"> · cancelled</span>{/if}
            </span>
            <span class="bar"><span style="width: {pct(x)}%"></span></span>
            <span class="num">
              {#if x.state === "running"}{human(x.done)} / {human(x.total)} {speed(x)}{:else if x.state === "done"}{human(x.total)}{/if}
            </span>
            {#if x.state === "running"}
              <button class="icon-btn" title="Cancel" onclick={() => x.tid && api.sftpCancel(x.tid)}><X size={13} /></button>
            {:else if x.state === "done"}
              <span class="ok"><Check size={14} /></span>
            {:else}
              <span></span>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .sftp {
    height: 100%;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }
  .fp-wait {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .fp-wait .pick {
    padding: 8px 8px 6px 6px;
  }
  .state {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 16px;
    color: var(--text-2);
    text-align: center;
    max-width: 420px;
  }
  .state .bad {
    color: var(--danger);
  }
  .panes {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    gap: 6px;
  }
  .mid {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 6px;
    color: var(--text-3);
  }
  .over-left :global([data-side="left"]),
  .over-right :global([data-side="right"]) {
    box-shadow: 0 0 0 2px var(--accent) inset;
  }
  .xfers {
    max-height: 190px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .xhead {
    display: flex;
    align-items: center;
    padding: 6px 8px 6px 12px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
    border-bottom: var(--border) solid var(--line);
  }
  .xlist {
    overflow: auto;
    padding: 4px;
  }
  .xfer {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) 140px 170px 26px;
    align-items: center;
    gap: 10px;
    padding: 4px 8px;
    font-size: 12.5px;
  }
  .xfer.bad .xl {
    color: var(--danger);
  }
  .dir {
    display: grid;
    color: var(--accent);
  }
  .bar {
    height: 6px;
    border-radius: 99px;
    background: var(--panel-2);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: inherit;
    transition: width 0.2s;
  }
  .num {
    font-size: 11.5px;
    color: var(--text-3);
    text-align: right;
    white-space: nowrap;
  }
  .ok {
    display: grid;
    place-items: center;
    color: var(--ok);
  }
</style>
