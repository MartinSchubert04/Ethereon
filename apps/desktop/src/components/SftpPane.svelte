<script lang="ts">
  // Pestaña de archivos de un host: esta PC a la izquierda, el servidor a la derecha.
  // Se pasa arrastrando de un panel al otro, desde el Explorador de Windows, o con
  // Espacio / el boton del medio. Las transferencias corren en segundo plano.
  import { onDestroy, onMount } from "svelte";
  import { Channel } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { ArrowLeft, ArrowRight, Check, CircleAlert, LoaderCircle, RotateCw, X } from "@lucide/svelte";
  import FilePanel, { human, type Fs } from "./FilePanel.svelte";
  import { api, errText, type XferEvent } from "../lib/api";
  import { testHost } from "../lib/hostkey.svelte";
  import { app, toast, type TermTab } from "../lib/state.svelte";
  import { ask } from "../lib/confirm.svelte";
  import { rise } from "../lib/motion";

  let { tab, active }: { tab: TermTab; active: boolean } = $props();

  let id = $state(0);
  let status = $state<"connecting" | "ready" | "failed">("connecting");
  let failure = $state("");
  let focused = $state<"local" | "remote">("remote");
  let localPanel = $state<FilePanel>();
  let remotePanel = $state<FilePanel>();
  let wrap: HTMLDivElement;

  interface Xfer {
    tid: number;
    upload: boolean;
    label: string;
    done: number;
    total: number;
    file: string;
    state: "running" | "done" | "error" | "cancelled";
    message: string;
    started: number;
  }
  let xfers = $state<Xfer[]>([]);

  const local: Fs = {
    side: "local",
    label: "This PC",
    sep: "\\",
    list: api.localList,
    mkdir: api.localMkdir,
    rename: api.localRename,
    remove: api.localRemove,
    home: api.localHome,
  };
  const remote: Fs = {
    side: "remote",
    get label() {
      return tab.host;
    },
    sep: "/",
    list: (p) => api.sftpList(id, p),
    mkdir: (p) => api.sftpMkdir(id, p),
    rename: (a, b) => api.sftpRename(id, a, b),
    remove: (p) => api.sftpRemove(id, p),
    home: () => Promise.resolve(home),
    real: (p) => api.sftpRealpath(id, p),
  };
  let home = "/";

  async function connect() {
    status = "connecting";
    failure = "";
    if (!(await testHost(tab.host, true))) {
      status = "failed";
      failure = "The host key was not accepted.";
      return setAlive(false);
    }
    try {
      const o = await api.sftpOpen(tab.host);
      id = o.id;
      home = o.home;
      status = "ready";
      setAlive(true);
      await Promise.all([localPanel?.start(), remotePanel?.start()]);
    } catch (e) {
      status = "failed";
      failure = errText(e);
      setAlive(false);
    }
  }

  function setAlive(v: boolean) {
    const t = app.terms.find((x) => x.key === tab.key);
    if (t) t.alive = v;
  }

  const base = (p: string) => p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? p;

  /** Pasa `paths` del lado `from` a la carpeta `into` del otro lado. */
  async function transfer(from: "local" | "remote", paths: string[], into?: string | null) {
    if (!paths.length || status !== "ready") return;
    const upload = from === "local";
    const target = upload ? remotePanel : localPanel;
    const dest = into ?? target?.current() ?? "";
    if (!dest) return toast(upload ? "Open a folder on the server first" : "Open a local folder first (not the drive list)", true);
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
    const label = paths.length === 1 ? base(paths[0]) : `${paths.length} items`;
    const ch = new Channel<XferEvent>();
    const x: Xfer = { tid: 0, upload, label, done: 0, total: 0, file: "", state: "running", message: "", started: Date.now() };
    xfers = [x, ...xfers];
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
      item.tid = await api.sftpTransfer(id, upload, paths, dest, ch);
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

  // Soltar archivos del Explorador de Windows sobre el panel del servidor.
  let unlisten: (() => void) | undefined;
  let osOver = $state(false);
  onMount(async () => {
    connect();
    unlisten = await getCurrentWebview().onDragDropEvent((ev) => {
      if (!active || status !== "ready") return;
      const p = ev.payload;
      const at = (pos: { x: number; y: number }) => {
        const r = devicePixelRatio || 1;
        return document.elementFromPoint(pos.x / r, pos.y / r);
      };
      if (p.type === "over" || p.type === "enter") {
        const el = at(p.position);
        osOver = !!el?.closest('[data-side="remote"]') && wrap.contains(el);
      } else if (p.type === "drop") {
        osOver = false;
        const el = at(p.position);
        if (el && wrap.contains(el) && el.closest('[data-side="remote"]')) {
          const row = el.closest<HTMLElement>(".row[data-dir]");
          transfer("local", p.paths, row?.dataset.path ?? null);
        }
      } else osOver = false;
    });
  });

  onDestroy(() => {
    unlisten?.();
    for (const x of xfers) if (x.state === "running" && x.tid) api.sftpCancel(x.tid);
    if (id) api.sftpClose(id);
  });

  // Al bloquear el vault la sesion SFTP deja de servir; al desbloquear se abre otra y
  // los paneles quedan donde estaban.
  let wasUnlocked = true;
  $effect(() => {
    const now = !!app.status?.unlocked;
    if (now && !wasUnlocked && status !== "connecting") reconnect();
    wasUnlocked = now;
  });

  async function reconnect() {
    try {
      const o = await api.sftpOpen(tab.host);
      if (id) api.sftpClose(id);
      id = o.id;
      home = o.home;
      status = "ready";
      setAlive(true);
      if (!localPanel?.current() && !remotePanel?.current()) await Promise.all([localPanel?.start(), remotePanel?.start()]);
      else await Promise.all([localPanel?.refresh(), remotePanel?.refresh()]);
    } catch (e) {
      status = "failed";
      failure = errText(e);
      setAlive(false);
    }
  }

  // Al volver a la pestaña, el foco va al panel que estaba activo.
  $effect(() => {
    if (active && status === "ready") queueMicrotask(() => wrap?.querySelector<HTMLElement>(`[data-side="${focused}"]`)?.focus());
  });
</script>

<div class="sftp" bind:this={wrap} class:os-over={osOver}>
  {#if status !== "ready"}
    <div class="state" in:rise>
      {#if status === "connecting"}
        <LoaderCircle size={22} class="spin" />
        <div>Opening SFTP on <b>{tab.host}</b>...</div>
      {:else}
        <CircleAlert size={22} />
        <div class="bad">{failure}</div>
        <button class="btn" onclick={connect}><RotateCw size={14} /> Try again</button>
      {/if}
    </div>
  {/if}

  <div class="panes" class:hidden={status !== "ready"}>
    <FilePanel
      bind:this={localPanel}
      fs={local}
      active={focused === "local"}
      onfocus={() => (focused = "local")}
      ontransfer={(p) => transfer("local", p)}
      ondropitems={(p, side, dir) => side === "remote" && transfer("local", p, dir)}
    />
    <div class="mid">
      <button class="icon-btn" title="Upload what is selected on the left (Space)" onclick={() => transfer("local", localPanel?.selection() ?? [])}><ArrowRight size={16} /></button>
      <button class="icon-btn" title="Download what is selected on the right (Space)" onclick={() => transfer("remote", remotePanel?.selection() ?? [])}><ArrowLeft size={16} /></button>
    </div>
    <FilePanel
      bind:this={remotePanel}
      fs={remote}
      active={focused === "remote"}
      onfocus={() => (focused = "remote")}
      ontransfer={(p) => transfer("remote", p)}
      ondropitems={(p, side, dir) => side === "local" && transfer("remote", p, dir)}
    />
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
            <span class="dir">{#if x.upload}<ArrowRight size={13} />{:else}<ArrowLeft size={13} />{/if}</span>
            <span class="xl ellipsis" title={x.message || x.file}>
              {x.upload ? "Upload" : "Download"} {x.label}
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
  .state {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    color: var(--text-2);
    text-align: center;
    max-width: 480px;
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
  .panes.hidden {
    display: none;
  }
  .mid {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 6px;
    color: var(--text-3);
  }
  .os-over :global([data-side="remote"]) {
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
