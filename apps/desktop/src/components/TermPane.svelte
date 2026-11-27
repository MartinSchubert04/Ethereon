<script lang="ts">
  // Una sesion de terminal (xterm.js) conectada a un host.
  import { onDestroy, onMount, untrack } from "svelte";
  import { Terminal, type ITheme } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { ImageAddon } from "@xterm/addon-image";
  import { Channel } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { api, errText, type TermEvent } from "../lib/api";
  import { testHost } from "../lib/hostkey.svelte";
  import { app, goToTab, leaving, toast, type TermTab } from "../lib/state.svelte";
  import { comboOf, matches, prefs, pretty } from "../lib/prefs.svelte";
  import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { cssVar } from "../lib/themes";
  import { tabStyle } from "../lib/hostcolors.svelte";
  import { boldWeight, termFont } from "../lib/termfonts";
  import "../lib/sixel";
  import { adopt, drop, own, type Live } from "../lib/live";
  import { watchCwd } from "../lib/cwd";
  import { dbg, dbgGlLost, dbgGone, dbgTerm, sw } from "../lib/debug.svelte";
  import { Highlighter, profileFor, tones } from "../lib/highlight";
  import { fixDefaultBackgrounds } from "../lib/xtermfix";
  import { Suggester } from "../lib/suggest";

  // `bare`: dentro de un mosaico, sin margen ni tarjeta propia (el marco lo pone TilePane).
  // `shown`: la pestaña se ve (en un mosaico se ven todos los paneles, aunque solo uno tenga
  // el foco, que es `active`).
  let { tab, active, bare = false, shown = active }: { tab: TermTab; active: boolean; bare?: boolean; shown?: boolean } = $props();

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
    const list = cssVar("--term-ansi", el).split(/\s+/).filter(Boolean);
    if (list.length !== 16) return {};
    const out: Record<string, string> = {};
    ANSI.forEach((n, i) => {
      out[n] = list[i];
      out[`bright${n[0].toUpperCase()}${n.slice(1)}`] = list[i + 8];
    });
    return out;
  }

  // Los colores se leen desde la terminal y no desde <body>: el host puede tener los suyos
  // (ver lib/hostcolors), puestos como estilo en .pane.
  function theme(): ITheme {
    const bg = cssVar("--term-bg", el);
    return {
      ...ansi(),
      background: bg.startsWith("rgba") ? "rgba(0,0,0,0)" : bg,
      foreground: cssVar("--term-fg", el),
      cursor: cssVar("--term-cursor", el),
      cursorAccent: bg,
      selectionBackground: cssVar("--term-selection", el),
      // Solo algunos esquemas de colores (Settings → Terminal → Colors) lo cambian.
      selectionForeground: cssVar("--term-selection-fg", el) || undefined,
    };
  }

  // Los colores propios del host (cambian si se editan, o con la opacidad). Un efecto corre
  // despues de actualizar el DOM: el estilo nuevo ya esta puesto cuando xterm lo lee.
  const hostLook = $derived(tabStyle(me()));
  $effect(() => {
    void hostLook;
    if (term) {
      term.options.theme = untrack(theme);
      untrack(() => L?.hl?.setPalette(hlColors()));
    }
  });

  // Resaltado de sintaxis: el general (Settings) o el de esta pestaña (clic derecho), con el
  // perfil que le toca segun el host (red, Linux, Windows).
  const hlHost = $derived(app.snap?.hosts.find((h) => h.name === me().host));
  const hlProfiles = $derived((me().highlight ?? prefs.term.syntax) ? profileFor(me(), hlHost) : null);
  $effect(() => {
    const p = hlProfiles;
    if (term && L?.hl) untrack(() => L.hl!.set(p));
  });

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
    else await pasteImage();
    term.focus();
  }

  // Sin texto puede haber una imagen. En una shell local con un programa corriendo (Claude
  // Code) se le pasa la tecla con la que el pega imagenes en Windows, Alt V: lee el
  // portapapeles el mismo y la adjunta ([Image #1]), como en Warp. En el prompt de la shell se
  // pega la ruta de la imagen guardada en la carpeta temporal. Por SSH, el programa del
  // servidor no ve el portapapeles de esta PC: la imagen se sube y se pega su ruta, que Claude
  // Code adjunta igual. En una consola serial, nada.
  let uploading = false;
  async function pasteImage() {
    const t = me();
    if (uploading || app.snap?.hosts.find((h) => h.name === t.host)?.transport === "serial") return;
    if (t.local && L.id && !L.sug?.atPrompt) {
      if (await api.clipboardHasImage().catch(() => false)) api.termWrite(L.id, "\x1bv");
      return;
    }
    uploading = true;
    try {
      const path = t.local ? await api.termPasteImageLocal() : await api.termPasteImage(t.host);
      if (path) term.paste(t.local ? localPath(path, t.local) : /\s/.test(path) ? `"${path}"` : path);
    } catch (e) {
      toast(`Couldn't paste the image: ${errText(e)}`, true);
    } finally {
      uploading = false;
    }
  }

  // ---- archivos soltados desde el Explorador (Tauri se queda con el arrastre: el evento
  // `drop` del navegador no llega, y sin esto no pasaba nada)

  // Como lo hace Windows Terminal: la ruta escrita para la shell, entre comillas si hace
  // falta. Las shells tipo bash la ven en su formato (/mnt/c en WSL, /c en Git Bash).
  const posix = (x: string) => (/^[\w@%+=:,./-]+$/.test(x) ? x : `'${x.replace(/'/g, "'\\''")}'`);
  function localPath(p: string, shell: string): string {
    const unix = (root: string) => p.replace(/^([A-Za-z]):\\/, (_, d) => `${root}${d.toLowerCase()}/`).replace(/\\/g, "/");
    if (shell === "wsl") return posix(unix("/mnt/"));
    if (shell === "gitbash") return posix(unix("/"));
    if (shell === "shell") return posix(p);
    return /[\s&()^;,'`$]/.test(p) ? `"${p}"` : p;
  }

  // Por SSH la ruta de esta PC no le sirve al servidor: se suben los archivos (como una
  // imagen pegada) y se escribe donde quedaron.
  let dropping = false;
  async function dropFiles(paths: string[]) {
    const t = me();
    if (!L.id || !paths.length) return;
    let out: string[];
    if (t.local) out = paths.map((p) => localPath(p, t.local!));
    else {
      if (dropping) return;
      dropping = true;
      toast(paths.length === 1 ? "Uploading the file..." : `Uploading ${paths.length} files...`);
      try {
        out = (await api.termPasteFiles(t.host, paths)).map(posix);
      } catch (e) {
        toast(`Couldn't send the file: ${errText(e)}`, true);
        return;
      } finally {
        dropping = false;
      }
    }
    term.paste(out.join(" "));
    term.focus();
  }

  let dropOver = $state(false);
  let unlistenDrop: (() => void) | undefined;
  let gone = false;
  getCurrentWebview()
    .onDragDropEvent((ev) => {
      const p = ev.payload;
      const inside = (pos: { x: number; y: number }) => {
        if (!shown || !el) return false;
        const r = devicePixelRatio || 1;
        const hit = document.elementFromPoint(pos.x / r, pos.y / r);
        return !!hit && el.parentElement!.contains(hit);
      };
      if (p.type === "enter" || p.type === "over") dropOver = inside(p.position);
      else if (p.type === "drop") {
        const hit = inside(p.position);
        dropOver = false;
        if (hit) dropFiles(p.paths);
      } else dropOver = false;
    })
    .then((u) => (gone ? u() : (unlistenDrop = u)));

  // OSC 52: un programa de la terminal pide copiar ("52;c;<base64>"). Asi copian Claude
  // Code, tmux o vim cuando corren en un servidor. Leer el portapapeles ("?") no se permite.
  function onOsc52(data: string): boolean {
    const b64 = data.slice(data.indexOf(";") + 1);
    if (!prefs.term.osc52Copy || !data.includes(";") || !b64 || b64 === "?") return true;
    try {
      writeText(new TextDecoder().decode(decode(b64))).catch(() => {});
    } catch {}
    return true;
  }

  function onTermKey(e: KeyboardEvent): boolean {
    if (e.type !== "keydown") return true;
    // La tecla de aceptar la sugerencia en gris, solo si hay una a la vista.
    if (L.sug?.accept(e)) return false;
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
    if (matches(e, "termBreak") && isSerial()) {
      e.preventDefault();
      sendBreak();
      return false;
    }
    const c = comboOf(e);
    if (c === "Shift+Enter" && prefs.term.shiftEnterNewline) {
      // xterm manda \r igual que Enter; Esc Enter es lo que Claude Code entiende como
      // salto de linea (lo mismo que configura su /terminal-setup).
      e.preventDefault();
      term.input("\x1b\r");
      return false;
    }
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

  const isSerial = () => !me().local && app.snap?.hosts.find((h) => h.name === me().host)?.transport === "serial";

  // La tecla Break de PuTTY. Tambien desde el clic derecho de la pestaña (evento
  // "ethereon:break" con la clave de la pestaña).
  async function sendBreak() {
    if (!L.id) return toast("The console is not connected", true);
    try {
      await api.termBreak(L.id);
      toast("Break sent");
    } catch (e) {
      toast(errText(e), true);
    }
  }
  function onBreak(e: Event) {
    if ((e as CustomEvent).detail === key) sendBreak();
  }

  function onContextMenu(e: MouseEvent) {
    if (!prefs.term.rightClick) return;
    e.preventDefault();
    if (term.hasSelection()) {
      copySelection();
      term.clearSelection();
    } else paste();
  }
  /** Los colores del resaltado, sacados de la paleta de esta terminal. */
  const hlColors = () => tones(cssVar("--term-ansi", el).split(/\s+/).filter(Boolean), cssVar("--term-bg", el));

  const onTheme = () => {
    if (!term) return;
    term.options.theme = theme();
    L?.hl?.setPalette(hlColors());
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
    // Vuelve de una actualizacion: primero lo que se veia, apagado, y despues conecta.
    const before = live.me().replay;
    if (before) {
      live.me().replay = undefined;
      term.write(`\x1b[2m${before.replace(/\n/g, "\r\n")}\x1b[0m\r\n`);
      term.writeln(`\x1b[2m──── before the update ────\x1b[0m`);
    }
    if (live.me().local) return connectLocal();
    // Registro nuevo para "Show logs": el de la conexion anterior ya no sirve.
    app.connLogs[live.me().host] = [];
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
    // Sesion nueva y viva: si la anterior habia quedado como terminada, ya no.
    setAlive(true);
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
    // La salida llega como ArrayBuffer, salvo que esta ventana haya pasado al IPC de respaldo
    // de Tauri (postMessage): ahi los mensajes de 1 KB o mas llegan como lista de numeros. Pasa
    // si una sola llamada por el protocolo ipc:// falla (por ejemplo, un dialogo modal o una
    // ventana emergente del WebView en medio) y dura hasta cerrar la ventana. Antes eso se
    // tomaba como el fin de la sesion: "The shell ended" con la shell viva.
    if (m instanceof ArrayBuffer) term.write(new Uint8Array(m));
    else if (Array.isArray(m) || ArrayBuffer.isView(m)) term.write(Uint8Array.from(m as ArrayLike<number>));
    // Cuando xterm termino de procesar todo lo repetido, vuelve a escuchar.
    // Ahi recien se ajusta al panel: xterm reacomoda las lineas y la shell se entera.
    else if (m.kind === "replayed")
      term.write("", () => {
        L.replaying = false;
        fitNow();
        if (L.id) api.termResize(L.id, term.cols, term.rows).catch(() => {});
      });
    else if (m.kind === "exit") {
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
      // Las decoraciones con color (el resaltado de sintaxis, lib/highlight.ts) son API
      // "propuesta" de xterm: sin esto registerDecoration tira un error.
      allowProposedApi: true,
      // Los links de la salida (OSC 8, los que imprime Claude Code) van al navegador. Lo de
      // xterm por defecto es confirm() y window.open() dentro del WebView: abria una ventana
      // emergente y dejaba la ventana con el IPC de respaldo (ver handle).
      linkHandler: {
        activate: (_e, uri) => {
          if (/^https?:\/\//i.test(uri)) api.openLink(uri).catch((err) => toast(errText(err), true));
        },
        allowNonHttpProtocols: false,
      },
    });
    const f = new FitAddon();
    t.loadAddon(f);
    t.open(el);
    // WebGL dibuja los bloques y las lineas de caja a mano, pixel exacto, en vez de usar
    // los de la fuente (que casi nunca llenan la celda y dejan costuras). Si la placa de
    // video no lo deja, queda el renderer comun.
    let glOk = false;
    if (sw.webgl) try {
      const gl = new WebglAddon();
      // El WebView tiene un tope de contextos WebGL por ventana (unos 16): con muchas
      // terminales, al pasarse descarta el mas viejo. Esa terminal sigue con el renderer
      // comun, pero hay que redibujarla; si no, queda en negro hasta que llegue salida.
      gl.onContextLoss(() => {
        dbgGlLost(t);
        gl.dispose();
        t.refresh(0, t.rows - 1);
      });
      t.loadAddon(gl);
      // Texto atenuado o en cursiva sin fondo negro (ver lib/xtermfix.ts).
      fixDefaultBackgrounds(gl);
      glOk = true;
    } catch (e) {
      dbg("webgl no disponible", String(e));
    }
    dbgTerm(t, key, glOk);
    // Imagenes en la terminal: Sixel (lo que usa Windows Terminal) y el protocolo de iTerm2
    // (WezTerm y otros). Con esto se ven los splash, chafa -f sixel, img2sixel, etc.
    t.loadAddon(new ImageAddon({ sixelSupport: true, iipSupport: true, storageLimit: 64 }));
    const live: Live = { term: t, fit: f, id: 0, replaying: false, handle, me, keys: () => true, selection: () => {}, parked: false };
    watchCwd(t, () => live.me());
    t.parser.registerOscHandler(52, onOsc52);
    // Sugerencias en gris mientras se escribe (ver lib/suggest.ts). Despues de watchCwd: los
    // dos escuchan el aviso del prompt y el de la carpeta.
    live.sug = new Suggester(t, {
      profile: () => live.me().local ?? live.me().host,
      shell: () => live.me().local,
      cwd: () => (live.me().local ? live.me().cwd : undefined),
      send: (d) => void (live.id && api.termWrite(live.id, d)),
      custom: () => {
        goToTab(0);
        app.view = "settings";
      },
    });
    // Lo que la terminal genera va a la sesion, la muestre el panel que la muestre.
    t.attachCustomKeyEventHandler((e) => live.keys(e));
    t.onSelectionChange(() => live.selection());
    t.onData((d) => {
      if (!live.id || live.replaying) return;
      live.sug?.input(d);
      api.termWrite(live.id, d);
    });
    // A la shell le llega el tamaño final, no cada paso de una animacion o de un arrastre:
    // con cada cambio redibuja el prompt, y muchos seguidos se pisan.
    let rt: ReturnType<typeof setTimeout> | undefined;
    t.onResize(() => {
      dbg("filas/columnas", { key: live.me().key, cols: t.cols, rows: t.rows });
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
    L.hl ??= new Highlighter(term, hlColors());
    L.hl.set(untrack(() => hlProfiles));
    dbgTerm(term, key, true);
    dbg(got ? "terminal adoptada" : "terminal nueva", { key, from }, true);
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
    window.addEventListener("ethereon:break", onBreak);
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

  // Al volver a verse se redibuja: oculta, el WebView puede haber descartado lo que tenia
  // pintado (sobre todo si se mostro en medio de un arrastre) y quedaba en negro.
  // Ademas, con WebGL las terminales comparten el atlas de letras: si otra (por ejemplo otra
  // pestaña del mismo puerto serie, que recibe la misma salida) lo cambio mientras esta
  // estaba oculta, al volver se veian letras cruzadas hasta el proximo cuadro (~1 s). Al
  // mostrarse se rehace el atlas, asi aparece bien de entrada.
  let wasShown = false;
  $effect(() => {
    if (term) dbg("panel", { key, shown, active });
    const now = shown && !!term;
    if (now && !wasShown)
      queueMicrotask(() => {
        term.clearTextureAtlas();
        term.refresh(0, term.rows - 1);
      });
    else if (now) queueMicrotask(() => term.refresh(0, term.rows - 1));
    wasShown = now;
  });
  onDestroy(() => {
    window.removeEventListener("themechange", onTheme);
    document.fonts.removeEventListener("loadingdone", refont);
    window.removeEventListener("ethereon:refresh", onRefresh);
    window.removeEventListener("ethereon:break", onBreak);
    gone = true;
    unlistenDrop?.();
    if (!L) return;
    dbg("panel desmontado", { key, parked: L.parked });
    // Estacionada: otro panel de esta ventana la toma, sigue viva.
    if (L.parked) return;
    dbgGone(L.term);
    drop(key, L);
    // Lo que llegue despues de la sesion que se cierra (su "termino") ya no es de nadie: si
    // no, marcaria como terminada a la pestaña, que al volver a montarse abre otra sesion.
    L.handle = () => {};
    // Si se esta pasando a otra ventana, la sesion sigue: la toma la otra.
    if (L.id && !me().moving && !leaving.has(L.id)) api.termClose(L.id);
    L.hl?.dispose();
    L.sug?.dispose();
    L.term.dispose();
  });
</script>

<div class="pane" class:bare style={hostLook}>
  <!-- El relleno va en .term y la terminal en .host, sin relleno: FitAddon mide la altura
       del contenedor directo y, con border-box, contaria el relleno como espacio libre
       (sobraba una fila y lo de abajo quedaba cortado). -->
  <div class="term card" class:drop-over={dropOver}><div class="host" bind:this={el}></div></div>
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
    position: relative;
  }
  .host {
    height: 100%;
    overflow: hidden;
  }
  /* Encima de los canvas de xterm: una sombra interna quedaria debajo. */
  .term.drop-over::after {
    content: "";
    position: absolute;
    inset: 0;
    z-index: 20;
    border-radius: inherit;
    box-shadow: 0 0 0 2px var(--accent) inset;
    pointer-events: none;
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
  /* La sugerencia en gris (lib/suggest.ts), encima de la celda del cursor. */
  .term :global(.eth-suggest) {
    position: absolute;
    z-index: 6;
    display: flex;
    align-items: center;
    gap: 8px;
    white-space: pre;
    pointer-events: none;
  }
  .term :global(.eth-suggest > span) {
    opacity: 0.42;
  }
  .term :global(.eth-suggest-key) {
    pointer-events: auto;
    font-family: var(--font);
    font-size: 10.5px;
    line-height: 1;
    padding: 2px 5px;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-3);
    background: transparent;
    cursor: pointer;
  }
  .term :global(.eth-suggest-key:hover) {
    color: var(--text);
    background: var(--hover);
  }
  /* Se abre hacia arriba, pegado a la etiqueta: el cursor suele estar en la ultima fila. */
  .term :global(.eth-suggest-menu) {
    position: absolute;
    bottom: calc(100% + 6px);
    right: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 168px;
    padding: 5px;
    border: 1px solid var(--line, var(--border));
    border-radius: var(--radius-sm, 8px);
    background: var(--float, var(--sidebar));
    box-shadow: var(--pop-shadow);
    pointer-events: auto;
    font-family: var(--font);
    font-size: 12.5px;
    white-space: nowrap;
    animation: eth-suggest-up 0.12s ease-out;
  }
  @keyframes eth-suggest-up {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }
  .term :global(.eth-suggest-title) {
    padding: 3px 8px 5px;
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .term :global(.eth-suggest-menu button) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    text-align: left;
    padding: 5px 8px;
    border-radius: 5px;
    color: var(--text);
    background: transparent;
    cursor: pointer;
  }
  .term :global(.eth-suggest-menu button:hover) {
    background: var(--hover);
  }
  .term :global(.eth-suggest-menu button.off) {
    color: var(--text-3);
    font-size: 12px;
  }
  .term :global(.eth-suggest-menu button.custom) {
    margin-top: 3px;
    padding-top: 7px;
    border-top: 1px solid var(--line, var(--border));
    border-radius: 0 0 5px 5px;
    color: var(--text-3);
  }
  .term :global(.eth-suggest-menu kbd) {
    font-family: var(--font);
    font-size: 11.5px;
    min-width: 22px;
    text-align: center;
    padding: 1px 6px;
    border: 1px solid var(--line, var(--border));
    border-bottom-width: 2px;
    border-radius: 4px;
  }
  .term :global(.eth-suggest-menu button.on kbd) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .term :global(.eth-suggest-menu .check) {
    color: var(--accent);
    font-size: 12px;
  }
</style>
