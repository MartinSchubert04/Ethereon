<script lang="ts">
  // Una sesion de terminal (xterm.js) conectada a un host.
  import { onDestroy, onMount } from "svelte";
  import { Terminal, type ITheme } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { ImageAddon } from "@xterm/addon-image";
  import { Channel } from "@tauri-apps/api/core";
  import { api, errText, type TermEvent } from "../lib/api";
  import { testHost } from "../lib/hostkey.svelte";
  import { app, toast, type TermTab } from "../lib/state.svelte";
  import { comboOf, matches, prefs, pretty } from "../lib/prefs.svelte";
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
    fitNow();
  };

  // Ajusta filas y columnas al espacio. Cuando cambia la fuente, xterm actualiza el tamaño
  // de la celda recien en el proximo cuadro: se vuelve a calcular ahi y, si no coincide, se
  // corrige (si no, sobra media fila y lo de abajo queda cortado).
  function fitNow() {
    if (!term || !el?.offsetWidth) return;
    fit.fit();
    requestAnimationFrame(() => {
      const d = fit.proposeDimensions();
      if (d && d.cols > 0 && d.rows > 0 && (d.cols !== term.cols || d.rows !== term.rows)) term.resize(d.cols, d.rows);
    });
  }

  // La terminal mide las letras al abrirse. Si la fuente del tema (por ejemplo la de
  // Athanor) todavia no cargo, mide con otra, calcula mal cuantas filas entran y lo de
  // abajo queda fuera de la vista. Cuando terminan de cargar las fuentes se vuelve a medir:
  // poner la misma fuente no hace nada, por eso se cambia un instante y se devuelve.
  const refont = () => {
    if (!term) return;
    const f = cssVar("--mono");
    term.options.fontFamily = "monospace";
    term.options.fontFamily = f;
    fitNow();
  };

  function decode(b64: string): Uint8Array {
    const s = atob(b64);
    const out = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
    return out;
  }

  async function connect() {
    if (tab.local) return connectLocal();
    term.writeln(`\x1b[2mConnecting to ${tab.host}...\x1b[0m`);
    // La primera vez hay que confirmar la huella (y la de cada salto).
    if (!(await testHost(tab.host, true))) {
      term.writeln(`\x1b[31mNot connected.\x1b[0m \x1b[2m${pretty(prefs.binds.refresh)} tries again\x1b[0m`);
      setAlive(false);
      return;
    }
    const ch = channel();
    try {
      fit.fit();
      id = await api.termOpen(tab.host, term.cols, term.rows, ch);
      term.focus();
    } catch (e) {
      term.writeln(`\x1b[31m${errText(e)}\x1b[0m \x1b[2m${pretty(prefs.binds.refresh)} tries again\x1b[0m`);
      setAlive(false);
    }
  }

  function channel() {
    const ch = new Channel<TermEvent>();
    ch.onmessage = (m) => {
      if (m.kind === "data") term.write(decode(m.b64));
      else {
        term.writeln(`\r\n\x1b[2m[session closed${m.code != null ? `, exit ${m.code}` : ""} · ${pretty(prefs.binds.refresh)} reconnects]\x1b[0m`);
        setAlive(false);
        id = 0;
      }
    };
    return ch;
  }

  async function connectLocal() {
    try {
      fit.fit();
      id = await api.termOpenLocal(tab.local!, term.cols, term.rows, tab.cwd ?? null, channel());
      term.focus();
    } catch (e) {
      term.writeln(`\x1b[31m${errText(e)}\x1b[0m \x1b[2m${pretty(prefs.binds.refresh)} tries again\x1b[0m`);
      setAlive(false);
    }
  }

  // Ctrl R en una pestaña sin conexion vuelve a conectar, en la misma pestaña.
  function onRefresh(e: Event) {
    if ((e as CustomEvent).detail !== tab.key || id) return;
    term.writeln(`\r\n\x1b[2mReconnecting...\x1b[0m`);
    setAlive(true);
    connect();
  }

  function setAlive(v: boolean) {
    const t = app.terms.find((x) => x.key === tab.key);
    if (t) t.alive = v;
  }

  onMount(() => {
    term = new Terminal({
      fontFamily: cssVar("--mono"),
      fontSize: fontSize(),
      // Con 1 los bloques (█ ▀ ▄) y las lineas de caja se tocan entre filas, como en
      // cualquier terminal; con mas aire quedan rayas entre una fila y la otra.
      lineHeight: 1,
      cursorBlink: true,
      allowTransparency: true,
      theme: theme(),
      scrollback: 5000,
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    // WebGL dibuja los bloques y las lineas de caja a mano, pixel exacto, en vez de usar
    // los de la fuente (que casi nunca llenan la celda y dejan costuras). Si la placa de
    // video no lo deja, queda el renderer comun.
    try {
      const gl = new WebglAddon();
      gl.onContextLoss(() => gl.dispose());
      term.loadAddon(gl);
    } catch {}
    // Imagenes en la terminal: Sixel (lo que usa Windows Terminal) y el protocolo de iTerm2
    // (WezTerm y otros). Con esto se ven los splash, chafa -f sixel, img2sixel, etc.
    term.loadAddon(new ImageAddon({ sixelSupport: true, iipSupport: true, storageLimit: 64 }));
    term.attachCustomKeyEventHandler(onTermKey);
    term.onSelectionChange(() => {
      if (prefs.term.copyOnSelect && term.hasSelection()) copySelection(false);
    });
    el.addEventListener("contextmenu", onContextMenu);
    term.onData((d) => id && api.termWrite(id, d));
    term.onResize(({ cols, rows }) => id && api.termResize(id, cols, rows));
    const ro = new ResizeObserver(() => {
      fitNow();
    });
    ro.observe(el);
    window.addEventListener("themechange", onTheme);
    document.fonts.addEventListener("loadingdone", refont);
    document.fonts.load(`${fontSize()}px ${cssVar("--mono")}`).then(refont, () => {});
    window.addEventListener("ethereon:refresh", onRefresh);
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
        fitNow();
        term.focus();
      });
    }
  });

  onDestroy(() => {
    window.removeEventListener("themechange", onTheme);
    document.fonts.removeEventListener("loadingdone", refont);
    window.removeEventListener("ethereon:refresh", onRefresh);
    if (id) api.termClose(id);
    term?.dispose();
  });
</script>

<div class="pane">
  <!-- El relleno va en .term y la terminal en .host, sin relleno: FitAddon mide la altura
       del contenedor directo y, con border-box, contaria el relleno como espacio libre
       (sobraba una fila y lo de abajo quedaba cortado). -->
  <div class="term card"><div class="host" bind:this={el}></div></div>
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
  .host {
    height: 100%;
    overflow: hidden;
  }
  .term :global(.xterm-viewport) {
    background: transparent !important;
  }
</style>
