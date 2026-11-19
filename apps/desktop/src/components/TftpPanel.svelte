<script lang="ts">
  // Servidor TFTP: una carpeta de esta PC que los equipos de red bajan (firmware, configs)
  // con `copy tftp: flash:`, y adonde pueden subir si se permite.
  import { ArrowDownToLine, ArrowUpFromLine, Copy, FolderOpen, Play, Square } from "@lucide/svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import Switch from "./Switch.svelte";
  import { api, type TftpEvent, type TftpStatus } from "../lib/api";
  import { copyPlain } from "../lib/secrets";
  import { attempt } from "../lib/state.svelte";

  // La ultima carpeta y opciones, para no elegirlas cada vez (no es nada de seguridad: el
  // servidor solo arranca con un clic).
  const KEY = "ethereon.tftp";
  function load(): { dir: string; port: number; write: boolean } {
    try {
      return { dir: "", port: 69, write: false, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
    } catch {
      return { dir: "", port: 69, write: false };
    }
  }
  const saved = load();
  let dir = $state(saved.dir);
  let port = $state(saved.port);
  let write = $state(saved.write);
  let status = $state<TftpStatus | null>(null);
  let log = $state<(TftpEvent & { at: string })[]>([]);

  onMount(() => {
    api.tftpStatus().then((s) => {
      status = s;
      if (s.running) ({ dir, port, write } = s);
    });
    const un = listen<TftpEvent>("tftp", (e) => {
      log = [{ ...e.payload, at: new Date().toLocaleTimeString() }, ...log].slice(0, 60);
    });
    return () => void un.then((f) => f());
  });

  async function pick() {
    const d = (await open({ directory: true, defaultPath: dir || undefined })) as string | null;
    if (d) dir = d;
  }

  async function start() {
    const p = Number(port) || 69;
    await attempt(async () => {
      status = await api.tftpStart(dir, p, write);
      try {
        localStorage.setItem(KEY, JSON.stringify({ dir, port: p, write }));
      } catch {}
    }, "TFTP server started");
  }

  async function stop() {
    status = await api.tftpStop();
  }

  const size = (n: number) => (n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)} MB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KB` : `${n} B`);
  const url = (ip: string) => `tftp://${ip}${status && status.port !== 69 ? `:${status.port}` : ""}/`;
</script>

<div class="card tftp">
  <div class="row">
    <h3>TFTP server</h3>
    <span class="chip {status?.running ? 'ok' : ''}">{status?.running ? "running" : "stopped"}</span>
    <span class="grow"></span>
    {#if status?.running}
      <button class="btn" onclick={stop}><Square size={13} /> Stop</button>
    {:else}
      <button class="btn primary" onclick={start} disabled={!dir}><Play size={13} /> Start</button>
    {/if}
  </div>
  <div class="sub muted">Switches, routers and APs download firmware or configs from this folder (<code>copy tftp: flash:</code>).</div>

  <div class="form">
    <div class="field">Folder
      <span class="with-btn">
        <input class="mono" bind:value={dir} placeholder="C:\TFTP" disabled={status?.running} aria-label="Folder" />
        <button class="btn" onclick={pick} disabled={status?.running}><FolderOpen size={13} /> Choose</button>
      </span>
    </div>
    <label class="field port">Port<input type="number" min="1" max="65535" bind:value={port} disabled={status?.running} /></label>
    <div class="field">
      <Switch bind:checked={write} disabled={status?.running}>Allow uploads</Switch>
      <span class="hint">Devices can save files here (backups of their config). Off: read-only.</span>
    </div>
  </div>

  {#if status?.running}
    <div class="addrs">
      <span class="muted">Point the device to</span>
      {#each status.addrs as ip}
        <button class="addr mono" title="Copy" onclick={() => copyPlain(ip, "Address")}>{url(ip)} <Copy size={12} /></button>
      {/each}
    </div>
    <div class="faint">Windows Firewall may ask to allow Ethereon the first time.</div>
  {/if}

  {#if log.length}
    <table class="list log">
      <thead><tr><th>Time</th><th>Device</th><th></th><th>File</th><th>Result</th></tr></thead>
      <tbody>
        {#each log as e}
          <tr>
            <td class="faint">{e.at}</td>
            <td class="mono">{e.peer}</td>
            <td title={e.write ? "Upload from the device" : "Download to the device"}>
              {#if e.write}<ArrowUpFromLine size={13} />{:else}<ArrowDownToLine size={13} />{/if}
            </td>
            <td class="mono">{e.file}</td>
            <td>
              {#if e.kind === "start"}<span class="muted">started{e.size !== null ? ` · ${size(e.size)}` : ""}</span>
              {:else if e.kind === "done"}<span class="ok">{size(e.bytes)} in {(e.ms / 1000).toFixed(1)} s</span>
              {:else}<span class="bad">{e.error}</span>{/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .tftp {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h3 {
    margin: 0;
    font-size: 15px;
  }
  .form {
    display: grid;
    grid-template-columns: 1fr 110px;
    gap: 10px 12px;
    align-items: start;
  }
  .form > :last-child {
    grid-column: 1 / -1;
  }
  .with-btn {
    display: flex;
    gap: 6px;
  }
  .with-btn input {
    flex: 1;
    min-width: 0;
  }
  .addrs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .addr {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    border: var(--border) solid var(--line);
    background: var(--panel-2);
    color: var(--text);
    cursor: pointer;
  }
  .log td {
    vertical-align: middle;
  }
  .ok {
    color: var(--ok);
  }
  .bad {
    color: var(--danger);
  }
</style>
