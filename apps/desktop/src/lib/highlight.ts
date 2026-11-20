// Resaltado de sintaxis de la terminal, como el de MobaXterm o SecureCRT: IPs, MACs,
// interfaces, estados (up / down), errores y avisos, prompts y lineas de configuracion.
//
// No toca los datos: cuando una linea termina de llegar se lee de la pantalla de xterm y se
// le pinta encima el color con decoraciones (`registerDecoration` con `foregroundColor`).
// Asi funciona aunque la salida llegue de a pedacitos (una consola serie parte una IP en
// dos), lo que se copia sigue siendo texto plano, y lo que ya trae colores propios (ls
// --color, vim, htop) no se toca. La pantalla alternativa (vim, htop, less) se saltea.
//
// Los colores son los ANSI del esquema de la terminal (los brillantes si el fondo es
// oscuro), asi combinan con cualquier tema.
import type { IBufferCell, IDecoration, IDisposable, IMarker, Terminal } from "@xterm/xterm";

export type Profile = "network" | "linux" | "windows";
type Tone = "red" | "green" | "yellow" | "blue" | "magenta" | "cyan" | "dim";

interface Rule {
  re: RegExp;
  tone: Tone;
  /** Pintar solo este grupo de la coincidencia (con el flag `d`). */
  group?: number;
}

const r = (src: string, tone: Tone, flags = "gi", group?: number): Rule => ({
  re: new RegExp(src, flags + (group ? "d" : "")),
  tone,
  group,
});

// El orden importa: la primera regla que pinta una celda gana.
const COMMON: Rule[] = [
  r(String.raw`https?://[^\s"'<>]+`, "blue"),
  // MAC: aa:bb:cc:dd:ee:ff, aa-bb-..., y la de Cisco aabb.ccdd.eeff
  r(String.raw`\b[0-9a-f]{2}(?:[:-][0-9a-f]{2}){5}\b|\b[0-9a-f]{4}\.[0-9a-f]{4}\.[0-9a-f]{4}\b`, "magenta"),
  // IPv4 con mascara opcional
  r(String.raw`\b(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}(?:/\d{1,2})?\b`, "cyan"),
  // IPv6 completa o abreviada con ::
  r(String.raw`\b(?:[0-9a-f]{1,4}:){7}[0-9a-f]{1,4}\b|(?<![\w:])(?:[0-9a-f]{1,4}:){1,6}:(?:[0-9a-f]{1,4}(?::[0-9a-f]{1,4})*)?(?:/\d{1,3})?(?![\w:])`, "cyan"),
  r(String.raw`\b(?:error|errors|fail(?:ed|ure|ing)?|denied|refused|invalid|fatal|critical|unreachable|timed? ?out|not found|no such|unknown host|bad|corrupt(?:ed)?|abort(?:ed)?)\b`, "red"),
  r(String.raw`\b(?:warn(?:ing)?|deprecated|retry(?:ing)?|degraded|disabled|expir(?:ed|es|ing))\b`, "yellow"),
  r(String.raw`\b(?:ok|success(?:ful(?:ly)?)?|succeeded|connected|established|enabled|healthy|passed|done|complete(?:d)?|reachable)\b`, "green"),
  // Hora: 10:42:07 o 10:42:07.123
  r(String.raw`\b\d{1,2}:\d{2}:\d{2}(?:\.\d+)?\b`, "dim"),
];

const NETWORK: Rule[] = [
  // Prompt de equipo: Switch#, Router(config-if)#, AP>, <Huawei>, [MikroTik] >
  r(String.raw`^\s*(?:[\w.\-/@:]+(?:\([\w\-./]+\))?[#>]|<[\w.\-]+>|\[[\w.\-@ ]+\]\s?>)`, "green", "g"),
  // Mensajes de syslog de IOS: %LINK-3-UPDOWN — severidad 0-3 rojo, 4 amarillo, 5-7 azul
  r(String.raw`%[A-Z0-9_]+-[0-3]-[A-Z0-9_]+`, "red", "g"),
  r(String.raw`%[A-Z0-9_]+-4-[A-Z0-9_]+`, "yellow", "g"),
  r(String.raw`%[A-Z0-9_]+-[5-7]-[A-Z0-9_]+`, "blue", "g"),
  // Errores de la CLI: "% Invalid input detected", "% Incomplete command"
  r(String.raw`^\s*%\s.*$`, "red", "g"),
  // Separadores y comentarios de configuracion
  r(String.raw`^\s*!.*$`, "dim", "g"),
  // Contadores en cero: atenuados (los distintos de cero van en rojo, mas abajo)
  r(String.raw`\b0\s+(?:input |output )?(?:errors|crc|runts|giants|throttles|collisions|interface resets|drops?|discards?|overruns?|ignored|late collision|frame|abort|underruns)\b`, "dim"),
  r(String.raw`administratively down`, "yellow"),
  r(String.raw`\b(?:down|notconnect|not connected|err-disabled|errdisable|blocking|inactive|suspended|lost|flapping|mismatch|collisions?|crc)\b`, "red"),
  r(String.raw`\b(?:up|forwarding|full|full-duplex|a-full|learning|designated)\b`, "green"),
  r(String.raw`\b(?:half|half-duplex|a-half|listening|init|exstart|exchange|loading|active|idle|connect)\b`, "yellow"),
  // Contadores de errores distintos de cero
  r(String.raw`\b[1-9]\d*\s+(?:input errors|output errors|crc|runts|giants|throttles|collisions|interface resets|drops?|discards?|overruns?|ignored|late collision)\b`, "red"),
  // Interfaces: Gi1/0/24, GigabitEthernet0/1, Te1/1/1, Po10, Vlan20, Lo0, Tu1, ge-0/0/0, ether1
  r(
    String.raw`\b(?:(?:Gi(?:gabitEthernet)?|Fa(?:stEthernet)?|Te(?:nGigabitEthernet)?|Twe(?:ntyFiveGigE)?|Fo(?:rtyGigabitEthernet)?|Hu(?:ndredGigE)?|Eth(?:ernet)?|Po(?:rt-channel)?|Vl(?:an)?|Lo(?:opback)?|Tu(?:nnel)?|Se(?:rial)?|BDI|BVI|Dot11Radio|mgmt|Null|Wlan-GigabitEthernet)\s?\d+(?:[/:.]\d+)*|(?:ge|xe|et|ae)-\d+(?:/\d+)*(?:\.\d+)?|(?:ether|sfp-sfpplus|bridge|wlan)\d+)\b`,
    "magenta",
  ),
  // Lineas de configuracion: lo que va al principio
  r(String.raw`^\s*(no\s+shutdown)\b`, "green", "g", 1),
  r(String.raw`^\s*(shutdown)\s*$`, "red", "g", 1),
  r(
    String.raw`^\s*((?:no\s+)?(?:interface|router|vlan|ip|ipv6|hostname|line|username|snmp-server|logging|ntp|spanning-tree|switchport|channel-group|access-list|route-map|prefix-list|aaa|crypto|banner|service|enable|network|neighbor|address-family|vrf|class-map|policy-map|object-group|exit-address-family|end|exit))\b`,
    // Sin "i": la configuracion va en minusculas; asi "Interface" de un encabezado no cuenta.
    "blue",
    "g",
    1,
  ),
  r(String.raw`^\s*description\s+(.+)$`, "yellow", "gi", 1),
];

const LINUX: Rule[] = [
  // user@host:~/dir$ (# si es root: rojo)
  r(String.raw`^[\w.\-]+@[\w.\-]+:[^\s$#]*#`, "red", "g"),
  r(String.raw`^[\w.\-]+@[\w.\-]+:[^\s$#]*\$`, "green", "g"),
  r(String.raw`\b(?:ERROR|ERR|CRIT(?:ICAL)?|FATAL|EMERG|ALERT|PANIC)\b`, "red", "g"),
  r(String.raw`\bWARN(?:ING)?\b`, "yellow", "g"),
  r(String.raw`\bINFO\b|\bNOTICE\b`, "green", "g"),
  r(String.raw`\bDEBUG\b|\bTRACE\b`, "dim", "g"),
  // systemctl status
  r(String.raw`^\s*(?:Loaded|Active|Main PID|Tasks|Memory|CPU|CGroup|Docs):`, "dim", "g"),
  r(String.raw`active \(running\)|active \(exited\)`, "green", "g"),
  r(String.raw`inactive \(dead\)`, "yellow", "g"),
  // Permisos de ls -l
  r(String.raw`^[-dlcbps][rwxsStT-]{9}[.+@]?`, "dim", "g"),
  // Interfaces de Linux
  r(String.raw`\b(?:eth|ens|enp\w*|eno|wlan|wlp\w*|docker|br|veth\w*|bond|tun|tap|wg|tailscale)\d+\b|\blo\b`, "magenta"),
  // Rutas absolutas
  r(String.raw`(?<=^|[\s="'(])(/(?:[\w.\-@+]+/?)+)`, "blue", "g"),
];

const WINDOWS: Rule[] = [
  r(String.raw`^PS [A-Za-z]:\\[^>]*>|^[A-Za-z]:\\[^>]*>`, "green", "g"),
  r(String.raw`\b\w*(?:Exception|Error)\b|Access is denied|Acceso denegado|CategoryInfo|FullyQualifiedErrorId|is not recognized|Cannot find|does not exist|No se encuentra|no existe`, "red"),
  r(String.raw`^\s*\+\s*~+.*$|^\s*At line:\d+.*$`, "dim", "g"),
  r(String.raw`Request timed out|Destination host unreachable|General failure|Media disconnected|Tiempo de espera agotado|Host de destino inaccesible|Medios desconectados`, "red"),
  r(String.raw`Reply from|Respuesta desde`, "green"),
  r(String.raw`\bTTL=\d+|\btime[=<]\d+ms|\btiempo[=<]\d+ms`, "cyan"),
  r(String.raw`\b[A-Za-z]:\\[^\s"'<>|*?:)]*`, "blue", "g"),
];

const PROFILES: Record<Profile, Rule[]> = {
  network: [...NETWORK, ...COMMON],
  linux: [...LINUX, ...COMMON],
  windows: [...WINDOWS, ...COMMON],
};

export interface Span {
  start: number;
  end: number;
  tone: Tone;
}

/** Lo que hay que pintar en un texto: rangos que no se pisan (gana la primera regla).
 *  `blocked(s, e)` dice si un rango ya tiene color propio. */
export function findSpans(text: string, profiles: Profile[], blocked: (s: number, e: number) => boolean = () => false, max = PER_LINE): Span[] {
  const out: Span[] = [];
  const taken = new Uint8Array(text.length);
  for (const rule of profiles.flatMap((p) => PROFILES[p])) {
    rule.re.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = rule.re.exec(text)) && out.length < max) {
      if (m[0].length === 0) {
        rule.re.lastIndex++;
        continue;
      }
      const [s, e] = rule.group && m.indices?.[rule.group] ? m.indices[rule.group]! : [m.index, m.index + m[0].length];
      if (e <= s || blocked(s, e)) continue;
      let free = true;
      for (let i = s; i < e; i++) if (taken[i]) free = false;
      if (!free) continue;
      taken.fill(1, s, e);
      out.push({ start: s, end: e, tone: rule.tone });
    }
  }
  return out;
}

/** Que perfil le toca a una terminal. */
export function profileFor(
  tab: { local?: string },
  host: { transport?: string; icon?: string | null; os?: string | null; tags?: string[] } | undefined,
): Profile[] {
  if (tab.local) return /powershell|pwsh|cmd/i.test(tab.local) ? ["windows"] : ["linux"];
  if (!host) return ["linux", "network"];
  if (host.transport === "serial") return ["network"];
  const tags = (host.tags ?? []).join(" ").toLowerCase();
  if (["switch", "router", "ap", "firewall", "network", "antenna", "cable"].includes(host.icon ?? "")) return ["network"];
  if (/cisco|switch|router|\bap\b|firewall|fortinet|fortigate|mikrotik|huawei|juniper|aruba|ubiquiti|unifi|hp ?e|procurve|nexus|ios/.test(tags)) return ["network"];
  if (/windows/i.test(host.os ?? "") || /windows/.test(tags)) return ["windows"];
  // Un host SSH sin pistas: Linux, y lo de red por si es un equipo.
  return ["linux", "network"];
}

/** xterm solo acepta #RRGGBB en las decoraciones. */
function hex(color: string): string | null {
  const c = document.createElement("canvas").getContext("2d");
  if (!c) return null;
  c.fillStyle = "#000000";
  c.fillStyle = color.trim();
  const v = c.fillStyle as string;
  if (v.startsWith("#")) return v;
  const m = v.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
  return m ? "#" + [m[1], m[2], m[3]].map((n) => (+n).toString(16).padStart(2, "0")).join("") : null;
}

/** Los colores ANSI de xterm, para cuando el tema no da los suyos. */
const XTERM_ANSI = ["#2e3436", "#cc0000", "#4e9a06", "#c4a000", "#3465a4", "#75507b", "#06989a", "#d3d7cf", "#555753", "#ef2929", "#8ae234", "#fce94f", "#729fcf", "#ad7fa8", "#34e2e2", "#eeeeec"];

/** Tono -> #RRGGBB con la paleta de la terminal (brillantes sobre fondo oscuro). */
export function tones(ansi: string[], background: string): Record<Tone, string> {
  const list = (ansi.length === 16 ? ansi : XTERM_ANSI).map((c, i) => hex(c) ?? XTERM_ANSI[i]);
  const bg = hex(background) ?? "#000000";
  const lum = (parseInt(bg.slice(1, 3), 16) * 299 + parseInt(bg.slice(3, 5), 16) * 587 + parseInt(bg.slice(5, 7), 16) * 114) / 255000;
  const o = lum < 0.5 ? 8 : 0;
  return { red: list[1 + o], green: list[2 + o], yellow: list[3 + o], blue: list[4 + o], magenta: list[5 + o], cyan: list[6 + o], dim: list[8] };
}

/** Cuantas pantallas arriba y abajo de lo que se ve se pintan de antemano (al desplazarse ya
 *  esta pintado). */
const MARGIN = 1;
/** Lo pintado a mas de estas pantallas de lo que se ve se despinta: cada decoracion viva le
 *  cuesta a xterm en cada cuadro y en cada linea que sale del historial. Con el historial
 *  lleno de lineas pintadas, 3,5 MB de salida tardaban 6 s en vez de 0,15 s. */
const KEEP = 3;
/** Lo que se esta escribiendo (el prompt, la linea sin Enter) se pinta despues de esta pausa. */
const IDLE_MS = 120;
/** Tope de decoraciones por linea. */
const PER_LINE = 24;

interface Row {
  marker: IMarker;
  decos: IDecoration[];
}

/** Despinta una fila (las decoraciones caen con su marcador). */
function drop(row: Row) {
  row.decos.forEach((d) => d.dispose());
  row.marker.dispose();
}

// Solo se pinta lo que se ve, mas un margen: una salida larga no deja miles de decoraciones
// vivas, y lo del historial se pinta recien cuando se sube a verlo.
export class Highlighter {
  private rows: Row[] = [];
  /** Desde esta linea todo es nuevo: lo pintado ahi pudo cambiar y se rehace. */
  private next: IMarker | null = null;
  private idle: ReturnType<typeof setTimeout> | undefined;
  private frame = 0;
  private subs: IDisposable[] = [];
  private cell: IBufferCell;
  private colors: Record<Tone, string>;
  private profiles: Profile[] = [];
  private on = false;

  private term: Terminal;

  constructor(term: Terminal, palette: Record<Tone, string>) {
    this.term = term;
    this.cell = term.buffer.active.getNullCell();
    this.colors = palette;
    this.subs.push(
      term.onWriteParsed(() => this.onWrite()),
      // Desplazarse por el historial: pintar lo que aparece.
      term.onScroll(() => this.schedule()),
      // Al cambiar el ancho las lineas se reacomodan: las columnas viejas ya no sirven.
      term.onResize(() => this.on && this.repaint()),
    );
  }

  /** Prende, apaga o cambia de perfil. */
  set(profiles: Profile[] | null) {
    const list = profiles ?? [];
    if (this.on === !!profiles && list.join() === this.profiles.join()) return;
    this.on = !!profiles;
    this.profiles = list;
    this.repaint();
  }

  /** Colores nuevos (cambio de tema o de esquema): se repinta. */
  setPalette(p: Record<Tone, string>) {
    const same = (Object.keys(p) as Tone[]).every((k) => p[k] === this.colors[k]);
    this.colors = p;
    if (!same && this.on) this.repaint();
  }

  dispose() {
    clearTimeout(this.idle);
    cancelAnimationFrame(this.frame);
    this.clear();
    this.subs.forEach((s) => s.dispose());
  }

  private clear() {
    const rows = this.rows;
    this.rows = [];
    rows.forEach(drop);
    this.next?.dispose();
    this.next = null;
  }

  /** Borra todo y pinta de nuevo lo que se ve. */
  private repaint() {
    this.clear();
    if (!this.on || this.term.buffer.active.type !== "normal") return;
    this.next = this.term.registerMarker(0) ?? null;
    this.update();
    this.later();
  }

  private onWrite() {
    if (!this.on) return;
    // vim, htop, less: pantalla alternativa, no se toca.
    if (this.term.buffer.active.type !== "normal") return;
    // Lo que se escribio desde la ultima vez (de `next` al cursor) se vuelve a pintar.
    const from = this.next && !this.next.isDisposed ? this.next.line : 0;
    this.forget((y) => y >= from);
    this.next?.dispose();
    this.next = this.term.registerMarker(0) ?? null;
    this.update();
    this.later();
  }

  /** La linea del cursor (un prompt, lo que se esta escribiendo), cuando se queda quieta. */
  private later() {
    clearTimeout(this.idle);
    this.idle = setTimeout(() => {
      const b = this.term.buffer.active;
      if (this.on && b.type === "normal") this.paint(b.baseY + b.cursorY, b.baseY + b.cursorY);
    }, IDLE_MS);
  }

  private schedule() {
    if (!this.on || this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.update();
    });
  }

  /** Despinta las filas cuya linea cumple `gone`. Primero se sacan de la lista: al soltar el
   *  marcador, su `onDispose` tambien la busca ahi. */
  private forget(gone: (y: number) => boolean) {
    const out = this.rows.filter((row) => row.marker.isDisposed || gone(row.marker.line));
    if (!out.length) return;
    this.rows = this.rows.filter((row) => !out.includes(row));
    out.forEach(drop);
  }

  /** Pinta lo que se ve (y un margen) que falte, y despinta lo que quedo lejos. */
  private update() {
    if (!this.on) return;
    const buf = this.term.buffer.active;
    if (buf.type !== "normal") return;
    const cursor = buf.baseY + buf.cursorY;
    const top = buf.viewportY;
    const n = this.term.rows;
    this.forget((y) => y < top - KEEP * n || y > top + n + KEEP * n);
    const done = new Set(this.rows.map((row) => row.marker.line));
    // La linea del cursor la pinta `later`, cuando se queda quieta.
    const to = Math.min(cursor - 1, top + n - 1 + MARGIN * n);
    for (let y = Math.max(0, top - MARGIN * n); y <= to && this.on; y++) if (!done.has(y)) this.paint(y, cursor);
  }

  /** Pinta la fila `y` (absoluta). `cursor` es la fila del cursor, para ubicar el marcador. */
  private paint(y: number, cursor: number) {
    const buf = this.term.buffer.active;
    const line = buf.getLine(y);
    // Si ya estaba pintada (la linea del cursor que cambio), se rehace.
    const old = this.rows.findIndex((row) => !row.marker.isDisposed && row.marker.line === y);
    if (old >= 0) drop(this.rows.splice(old, 1)[0]);
    if (!line) return;
    // Se anota aunque no haya nada que pintar: asi no se vuelve a leer en cada cuadro.
    const marker = this.term.registerMarker(y - cursor);
    if (!marker) return;
    const decos: IDecoration[] = [];
    this.rows.push({ marker, decos });
    // Cuando la linea sale del historial se olvida.
    marker.onDispose(() => {
      const i = this.rows.findIndex((row) => row.marker === marker);
      if (i >= 0) this.rows.splice(i, 1);
    });
    // El texto de la fila, con la columna de cada caracter (las letras anchas ocupan dos).
    let text = "";
    const col: number[] = [];
    const width: number[] = [];
    const busy = new Uint8Array(this.term.cols + 1);
    for (let x = 0; x < this.term.cols; x++) {
      const c = line.getCell(x, this.cell);
      if (!c) break;
      const w = c.getWidth();
      if (w === 0) continue;
      const s = c.getChars() || " ";
      // Lo que ya trae color propio no se toca.
      if (!c.isFgDefault() || c.isInverse()) busy[x] = 1;
      for (let k = 0; k < s.length; k++) {
        col.push(x);
        width.push(w);
      }
      text += s;
    }
    if (!text.trim()) return;
    const x0 = (s: number) => col[s];
    const x1 = (e: number) => col[e - 1] + width[e - 1];
    const blocked = (s: number, e: number) => {
      for (let x = x0(s); x < x1(e); x++) if (busy[x]) return true;
      return false;
    };
    try {
      for (const sp of findSpans(text, this.profiles, blocked)) {
        const d = this.term.registerDecoration({ marker, x: x0(sp.start), width: x1(sp.end) - x0(sp.start), foregroundColor: this.colors[sp.tone], layer: "bottom" });
        if (d) decos.push(d);
      }
    } catch (e) {
      // Si xterm no deja decorar (por ejemplo, sin allowProposedApi), se apaga en vez de
      // tirar el mismo error en cada linea.
      console.warn("highlight off:", e);
      this.on = false;
      this.clear();
    }
  }
}
