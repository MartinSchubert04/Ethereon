<script lang="ts">
  // Una sesion de terminal (xterm.js) conectada a un host.
  import { onDestroy, onMount } from "svelte";
  import { Terminal, type ITheme } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { Channel } from "@tauri-apps/api/core";
  import { api, errText, type TermEvent } from "../lib/api";
  import { testHost } from "../lib/hostkey.svelte";
  import { app, type TermTab } from "../lib/state.svelte";
  import { cssVar } from "../lib/themes";

  let { tab, active }: { tab: TermTab; active: boolean } = $props();

  let el: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let id = 0;

  function theme(): ITheme {
    const bg = cssVar("--term-bg");
    return {
      background: bg.startsWith("rgba") ? "rgba(0,0,0,0)" : bg,
      foreground: cssVar("--term-fg"),
      cursor: cssVar("--term-cursor"),
      cursorAccent: bg,
      selectionBackground: cssVar("--term-selection"),
    };
  }

  const onTheme = () => term && (term.options.theme = theme());

  function decode(b64: string): Uint8Array {
    const s = atob(b64);
    const out = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
    return out;
  }

  async function connect() {
    term.writeln(`\x1b[2mConnecting to ${tab.host}...\x1b[0m`);
    // La primera vez hay que confirmar la huella (y la de cada salto).
    if (!(await testHost(tab.host, true))) {
      term.writeln("\x1b[31mNot connected.\x1b[0m");
      setAlive(false);
      return;
    }
    const ch = new Channel<TermEvent>();
    ch.onmessage = (m) => {
      if (m.kind === "data") term.write(decode(m.b64));
      else {
        term.writeln(`\r\n\x1b[2m[session closed${m.code != null ? `, exit ${m.code}` : ""}]\x1b[0m`);
        setAlive(false);
        id = 0;
      }
    };
    try {
      fit.fit();
      id = await api.termOpen(tab.host, term.cols, term.rows, ch);
      term.focus();
    } catch (e) {
      term.writeln(`\x1b[31m${errText(e)}\x1b[0m`);
      setAlive(false);
    }
  }

  function setAlive(v: boolean) {
    const t = app.terms.find((x) => x.key === tab.key);
    if (t) t.alive = v;
  }

  onMount(() => {
    term = new Terminal({
      fontFamily: cssVar("--mono"),
      fontSize: 13,
      lineHeight: 1.25,
      cursorBlink: true,
      allowTransparency: true,
      theme: theme(),
      scrollback: 5000,
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    term.onData((d) => id && api.termWrite(id, d));
    term.onResize(({ cols, rows }) => id && api.termResize(id, cols, rows));
    const ro = new ResizeObserver(() => {
      if (el.offsetWidth) fit.fit();
    });
    ro.observe(el);
    window.addEventListener("themechange", onTheme);
    connect();
    return () => ro.disconnect();
  });

  $effect(() => {
    if (active && term) {
      queueMicrotask(() => {
        fit.fit();
        term.focus();
      });
    }
  });

  onDestroy(() => {
    window.removeEventListener("themechange", onTheme);
    if (id) api.termClose(id);
    term?.dispose();
  });
</script>

<div class="pane">
  <div class="term card" bind:this={el}></div>
</div>

<style>
  .pane {
    height: 100%;
    padding: 12px;
  }
  .term {
    height: 100%;
    padding: 10px 4px 4px 12px;
    background: var(--term-bg);
    overflow: hidden;
  }
  .term :global(.xterm-viewport) {
    background: transparent !important;
  }
</style>
