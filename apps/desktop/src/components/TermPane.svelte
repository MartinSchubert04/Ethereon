<script lang="ts">
  // Una sesion de terminal (xterm.js) conectada a un host.
  import { onDestroy, onMount } from "svelte";
  import { Terminal, type ITheme } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { Channel } from "@tauri-apps/api/core";
  import { api, errText, type TermEvent } from "../lib/api";
  import { testHost } from "../lib/hostkey.svelte";
  import { app, toast, type TermTab } from "../lib/state.svelte";
  import { comboOf, matches, prefs } from "../lib/prefs.svelte";
  import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
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

  const fontSize = () => prefs.term.fontSize || parseInt(cssVar("--term-font-size")) || 13;

  // ---- portapapeles (cada comportamiento se prende o apaga en Settings → Terminal)

  async function copySelection(announce = true) {
    const text = term.getSelection();
    if (!text) return;
    await writeText(text);
    if (announce) toast("Copied");
  }

  async function paste() {
    const text = await readText().catch(() => "");
    if (text) term.paste(text); // respeta el "bracketed paste" de la shell
    term.focus();
  }

  function onTermKey(e: KeyboardEvent): boolean {
    if (e.type !== "keydown") return true;
    if (matches(e, "termCopy")) {
      e.preventDefault();
      copySelection();
      return false;
    }
    if (matches(e, "termPaste")) {
      e.preventDefault();
      paste();
      return false;
    }
    const c = comboOf(e);
    if (c === "Ctrl+C" && prefs.term.ctrlCCopy && term.hasSelection()) {
      e.preventDefault();
      copySelection();
      term.clearSelection();
      return false;
    }
    if (c === "Ctrl+V") {
      // Se frena siempre el pegado nativo del navegador: o pega Ethereon, o la shell
      // recibe Ctrl V.
      e.preventDefault();
      if (prefs.term.ctrlVPaste) {
        paste();
        return false;
      }
    }
    return true;
  }

  function onContextMenu(e: MouseEvent) {
    if (!prefs.term.rightClick) return;
    e.preventDefault();
    if (term.hasSelection()) {
      copySelection();
      term.clearSelection();
    } else paste();
  }
  const onTheme = () => {
    if (!term) return;
    term.options.theme = theme();
    term.options.fontFamily = cssVar("--mono");
    term.options.fontSize = fontSize();
    fit.fit();
  };

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
      fontSize: fontSize(),
      lineHeight: 1.25,
      cursorBlink: true,
      allowTransparency: true,
      theme: theme(),
      scrollback: 5000,
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    term.attachCustomKeyEventHandler(onTermKey);
    term.onSelectionChange(() => {
      if (prefs.term.copyOnSelect && term.hasSelection()) copySelection(false);
    });
    el.addEventListener("contextmenu", onContextMenu);
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

  // Cambiar el tamaño de letra en Settings se aplica en vivo.
  $effect(() => {
    prefs.term.fontSize;
    if (term) queueMicrotask(onTheme);
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
