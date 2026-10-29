<script lang="ts">
  // Una sesion de terminal (xterm.js) conectada a un host.
  import { onDestroy, onMount, untrack } from "svelte";
  import { Terminal, type ITheme } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { ImageAddon } from "@xterm/addon-image";
  import { Channel } from "@tauri-apps/api/core";
  import { api, errText, type TermEvent } from "../lib/api";
  import { testHost } from "../lib/hostkey.svelte";
  import { app, leaving, toast, type TermTab } from "../lib/state.svelte";
  import { comboOf, matches, prefs, pretty } from "../lib/prefs.svelte";
  import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { cssVar } from "../lib/themes";
  import { boldWeight, termFont } from "../lib/termfonts";
  import "../lib/sixel";
  import { adopt, drop, own, type Live } from "../lib/live";

  // `bare`: dentro de un mosaico, sin margen ni tarjeta propia (el marco lo pone TilePane).
  let { tab, active, bare = false }: { tab: TermTab; active: boolean; bare?: boolean } = $props();

  let el: HTMLDivElement;
  // La terminal viva (ver lib/live.ts): puede venir de otro panel de esta ventana.
  let L: Live;
  let term: Terminal;
  let fit: FitAddon;

  // Este panel en el estado, buscado por clave. Cuando una pestaña pasa a mosaico, su
  // terminal sigue montada pero la representa otro objeto (el primer panel), y la prop puede
  // seguir apuntando al viejo: se busca siempre el actual.
  const key = untrack(() => tab.key);
  function me(): TermTab {
    for (const t of app.terms) {
      if (t.key === key && !t.layout) return t;
      const m = t.members?.find((x) => x.key === key);
      if (m) return m;
    }
    return tab;
  }

  // Los 16 colores ANSI, si el tema los da (--term-ansi, en orden: negro rojo verde amarillo
  // azul magenta cian blanco, normales y despues brillantes). Si no, los de xterm.
  const ANSI = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"] as const;
  function ansi(): Partial<ITheme> {
    const list = cssVar("--term-ansi").split(/\s+/).filter(Boolean);
    if (list.length !== 16) return {};
    const out: Record<string, string> = {};
    ANSI.forEach((n, i) => {
      out[n] = list[i];
      out[`bright${n[0].toUpperCase()}${n.slice(1)}`] = list[i + 8];
    });
    return out;
  }

  function theme(): ITheme {
    const bg = cssVar("--term-bg");
    return {
      ...ansi(),
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
    term.options.fontFamily = termFont();
    term.options.fontWeightBold = boldWeight();
    term.options.fontSize = fontSize();
    fitNow();
  };

  // Ajusta filas y columnas al espacio. Cuando cambia la fuente, xterm actualiza el tamaño
  // de la celda recien en el proximo cuadro: se vuelve a calcular ahi y, si no coincide, se
  // corrige (si no, sobra media fila y lo de abajo queda cortado).
  function fitNow() {
    // Mientras se repite el historial queda con el tamaño de antes (ver attachSession).
    if (!term || !el?.offsetWidth || L?.replaying) return;
    fit.fit();
    requestAnimationFrame(() => {
      if (!el?.offsetWidth) return;
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
    const f = termFont();
    term.options.fontFamily = "monospace";
    term.options.fontFamily = f;
    term.options.fontWeightBold = boldWeight();
    fitNow();
  };

  function decode(b64: string): Uint8Array {
    const s = atob(b64);
    const out = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
    return out;
  }

  // Lo que se escribe en el estado va a L.me(): si la terminal se mudo de panel mientras
  // conectaba, el que la muestra ahora es otro.
  async function connect() {
    const live = L;
    if (live.me().attach) return attachSession();
    live.me().connecting = true;
    live.me().error = undefined;
    live.me().failedAt = undefined;
    if (live.me().local) return connectLocal();
    term.writeln(`\x1b[2mConnecting to ${live.me().host}...\x1b[0m`);
    // La primera vez hay que confirmar la huella (y la de cada salto).
    const fail = { text: "", at: undefined as string | undefined };
    if (!(await testHost(live.me().host, true, (text, at) => Object.assign(fail, { text, at })))) {
      term.writeln(`\x1b[31m${fail.text || "Not connected."}\x1b[0m \x1b[2m${pretty(prefs.binds.refresh)} tries again\x1b[0m`);
      live.me().error = fail.text || undefined;
      live.me().failedAt = fail.at;
      setAlive(false);
      return;
    }
    const ch = channel();
    try {
      fit.fit();
      const [c, r] = [term.cols, term.rows];
      opened(await api.termOpen(live.me().host, c, r, ch), c, r);
    } catch (e) {
      term.writeln(`\x1b[31m${errText(e)}\x1b[0m \x1b[2m${pretty(prefs.binds.refresh)} tries again\x1b[0m`);
      live.me().error = errText(e);
      setAlive(false);
    }
  }

  // La sesion ya esta abierta. Si mientras arrancaba la terminal cambio de tamaño (cargo la
  // fuente, se agrando la ventana), ese aviso se perdio porque todavia no habia id: se manda
  // ahora. Si no, la shell dibuja con otro tamaño y las lineas se pisan.
  function opened(sid: number, cols: number, rows: number) {
    const live = L;
    live.id = sid;
    live.me().sid = sid;
    live.me().connecting = false;
    live.me().error = undefined;
    if (term.cols !== cols || term.rows !== rows) api.termResize(sid, term.cols, term.rows).catch(() => {});
    if (active) term.focus();
  }

  /** El canal de la sesion le pasa cada mensaje al panel que muestra la terminal ahora. */
  function channel() {
    const live = L;
    const ch = new Channel<TermEvent>();
    ch.onmessage = (m) => live.handle(m);
    return ch;
  }

  function handle(m: TermEvent) {
    if (m.kind === "data") term.write(decode(m.b64));
    // Cuando xterm termino de procesar todo lo repetido, vuelve a escuchar.
    // Ahi recien se ajusta al panel: xterm reacomoda las lineas y la shell se entera.
    else if (m.kind === "replayed")
      term.write("", () => {
        L.replaying = false;
        fitNow();
        if (L.id) api.termResize(L.id, term.cols, term.rows).catch(() => {});
      });
    else {
      term.writeln(`\r\n\x1b[2m[session closed${m.code != null ? `, exit ${m.code}` : ""} · ${pretty(prefs.binds.refresh)} reconnects]\x1b[0m`);
      setAlive(false);
      L.id = 0;
    }
  }

  async function connectLocal() {
    const live = L;
    try {
      fit.fit();
      const [c, r] = [term.cols, term.rows];
      opened(await api.termOpenLocal(live.me().local!, c, r, live.me().cwd ?? null, channel()), c, r);
    } catch (e) {
      term.writeln(`\x1b[31m${errText(e)}\x1b[0m \x1b[2m${pretty(prefs.binds.refresh)} tries again\x1b[0m`);
      live.me().error = errText(e);
      setAlive(false);
    }
  }

  // Viene de otra ventana: toma la sesion que esa ventana solto. Primero llega lo ultimo que
  // se vio, despues sigue en vivo; al final se ajusta el tamaño a esta ventana.
  async function attachSession() {
    const live = L;
    const sid = live.me().attach!;
    const size = live.me().size;
    live.me().attach = undefined;
    live.me().size = undefined;
    // El historial se repite con el tamaño que tenia la terminal cuando se dibujo: con otro,
    // lo que la shell ubico con posiciones fijas (el prompt, un splash) quedaria desarmado.
    if (size) term.resize(size.cols, size.rows);
    live.replaying = true;
    try {
      await api.termAttach(sid, channel());
      live.id = sid;
      live.me().sid = sid;
      // La terminal que la solto ya se desmonto: desde ahora cerrar esta cierra la sesion.
      setTimeout(() => leaving.delete(sid), 3000);
      setAlive(true);
      // Si la repeticion ya termino antes de saber el id, el ajuste no le llego a la shell.
      if (!live.replaying) api.termResize(sid, term.cols, term.rows).catch(() => {});
      if (active) term.focus();
    } catch {
      live.replaying = false;
      term.writeln("\x1b[2m[this session already ended]\x1b[0m");
      setAlive(false);
    }
  }

  // Ctrl R en una pestaña sin conexion vuelve a conectar, en la misma pestaña.
  function onRefresh(e: Event) {
    if ((e as CustomEvent).detail !== key || L.id) return;
    term.writeln(`\r\n\x1b[2mReconnecting...\x1b[0m`);
    setAlive(true);
    connect();
  }

  function setAlive(v: boolean) {
    const t = L.me();
    t.alive = v;
    if (!v) t.connecting = false;
  }

  /** Una terminal nueva, con sus complementos. */
  function create(): Live {
    const t = new Terminal({
      fontFamily: termFont(),
      fontWeightBold: boldWeight(),
      // La negrita tambien aclara el color, como en Windows Terminal.
      drawBoldTextInBrightColors: true,
      fontSize: fontSize(),
      // Con 1 los bloques (█ ▀ ▄) y las lineas de caja se tocan entre filas, como en
      // cualquier terminal; con mas aire quedan rayas entre una fila y la otra.
      lineHeight: 1,
      cursorBlink: true,
      allowTransparency: true,
      theme: theme(),
      scrollback: 5000,
    });
    const f = new FitAddon();
    t.loadAddon(f);
    t.open(el);
    // WebGL dibuja los bloques y las lineas de caja a mano, pixel exacto, en vez de usar
    // los de la fuente (que casi nunca llenan la celda y dejan costuras). Si la placa de
    // video no lo deja, queda el renderer comun.
    try {
      const gl = new WebglAddon();
      gl.onContextLoss(() => gl.dispose());
      t.loadAddon(gl);
    } catch {}
    // Imagenes en la terminal: Sixel (lo que usa Windows Terminal) y el protocolo de iTerm2
    // (WezTerm y otros). Con esto se ven los splash, chafa -f sixel, img2sixel, etc.
    t.loadAddon(new ImageAddon({ sixelSupport: true, iipSupport: true, storageLimit: 64 }));
    const live: Live = { term: t, fit: f, id: 0, replaying: false, handle, me, keys: () => true, selection: () => {}, parked: false };
    // Lo que la terminal genera va a la sesion, la muestre el panel que la muestre.
    t.attachCustomKeyEventHandler((e) => live.keys(e));
    t.onSelectionChange(() => live.selection());
    t.onData((d) => live.id && !live.replaying && api.termWrite(live.id, d));
    // A la shell le llega el tamaño final, no cada paso de una animacion o de un arrastre:
    // con cada cambio redibuja el prompt, y muchos seguidos se pisan.
    let rt: ReturnType<typeof setTimeout> | undefined;
    t.onResize(() => {
      clearTimeout(rt);
      rt = setTimeout(() => live.id && api.termResize(live.id, t.cols, t.rows), 60);
    });
    return live;
  }

  onMount(() => {
    // ¿Viene de otro panel de esta ventana? Entonces es la misma terminal, que se mueve aca.
    const from = me().adopt;
    const got = from != null ? adopt(from, key) : null;
    if (got) {
      L = got;
      me().adopt = undefined;
      el.appendChild(got.term.element!);
    } else {
      L = create();
      own(key, L);
    }
    term = L.term;
    fit = L.fit;
    L.handle = handle;
    L.me = me;
    L.keys = onTermKey;
    L.selection = () => {
      if (prefs.term.copyOnSelect && term.hasSelection()) copySelection(false);
    };
    if (got) me().sid = got.id || undefined;
    el.addEventListener("contextmenu", onContextMenu);
    const ro = new ResizeObserver(() => fitNow());
    ro.observe(el);
    window.addEventListener("themechange", onTheme);
    document.fonts.addEventListener("loadingdone", refont);
    document.fonts.load(`${fontSize()}px ${termFont()}`).then(refont, () => {});
    window.addEventListener("ethereon:refresh", onRefresh);
    if (got) {
      // El tema o la fuente pueden haber cambiado mientras estaba en otro lado.
      onTheme();
      term.refresh(0, term.rows - 1);
    } else connect();
    return () => ro.disconnect();
  });

  // Cambiar el tamaño de letra en Settings se aplica en vivo.
  $effect(() => {
    prefs.term.fontSize;
    prefs.term.fontFamily;
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
    if (!L) return;
    // Estacionada: otro panel de esta ventana la toma, sigue viva.
    if (L.parked) return;
    drop(key, L);
    // Si se esta pasando a otra ventana, la sesion sigue: la toma la otra.
    if (L.id && !me().moving && !leaving.has(L.id)) api.termClose(L.id);
    L.term.dispose();
  });
</script>

<div class="pane" class:bare>
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
  .bare {
    padding: 0;
  }
  .bare .term {
    padding: 0;
    border: 0;
    border-radius: 0;
    box-shadow: none;
    clip-path: none;
    background: transparent;
  }
  .term :global(.xterm-viewport) {
    background: transparent !important;
  }
</style>
