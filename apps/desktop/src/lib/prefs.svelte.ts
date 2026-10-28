// Preferencias de la interfaz: atajos y terminal. No afectan la seguridad, asi que viven
// en el almacenamiento del WebView (las de seguridad estan en el vault).

export type Action =
  | "newTab"
  | "newLocal"
  | "refresh"
  | "save"
  | "newItem"
  | "zoomIn"
  | "zoomOut"
  | "zoomReset"
  | "closeTab"
  | "closeTabAnywhere"
  | "nextTab"
  | "prevTab"
  | "sidebar"
  | "search"
  | "settings"
  | "lock"
  | "termCopy"
  | "termPaste";

export const ACTIONS: { id: Action; label: string; def: string }[] = [
  { id: "newTab", label: "New tab (pick a host or local shell)", def: "Ctrl+T" },
  { id: "newLocal", label: "New local terminal (first shell in the list)", def: "Ctrl+Shift+T" },
  { id: "refresh", label: "Refresh: folders in SFTP, reconnect a tab that lost its connection", def: "Ctrl+R" },
  { id: "closeTab", label: "Close tab", def: "Ctrl+W" },
  { id: "closeTabAnywhere", label: "Close tab, also from inside a terminal", def: "Ctrl+Shift+W" },
  { id: "nextTab", label: "Next tab", def: "Ctrl+Tab" },
  { id: "prevTab", label: "Previous tab", def: "Ctrl+Shift+Tab" },
  { id: "sidebar", label: "Show or hide the sidebar", def: "Ctrl+B" },
  { id: "search", label: "Search hosts", def: "Ctrl+F" },
  { id: "settings", label: "Settings", def: "Ctrl+," },
  { id: "lock", label: "Lock the vault", def: "Ctrl+Shift+L" },
  { id: "save", label: "Save the open form (host, group, credential)", def: "Ctrl+S" },
  { id: "newItem", label: "New host (in Keychain: new credential)", def: "Ctrl+N" },
  { id: "zoomIn", label: "Terminal: bigger text", def: "Ctrl+=" },
  { id: "zoomOut", label: "Terminal: smaller text", def: "Ctrl+-" },
  { id: "zoomReset", label: "Terminal: text size of the theme", def: "Ctrl+0" },
  { id: "termCopy", label: "Terminal: copy selection", def: "Ctrl+Shift+C" },
  { id: "termPaste", label: "Terminal: paste", def: "Ctrl+Shift+V" },
];

const defaults = () => ({
  binds: Object.fromEntries(ACTIONS.map((a) => [a.id, a.def])) as Record<Action, string>,
  term: {
    /** Copiar apenas se selecciona (como PuTTY). */
    copyOnSelect: false,
    /** Clic derecho: copia si hay seleccion, si no pega (como Windows Terminal). */
    rightClick: true,
    /** Ctrl C copia si hay texto seleccionado; si no, corta el proceso como siempre. */
    ctrlCCopy: true,
    /** Ctrl V pega. Apagado, la shell recibe Ctrl V. */
    ctrlVPaste: true,
    /** Ctrl W dentro de una terminal va a la shell (borra la palabra anterior). */
    ctrlWToShell: true,
    /** 0 = lo que diga el tema. */
    fontSize: 0,
    /** Vacio = la del tema. Si no, una o varias familias separadas por coma. */
    fontFamily: "",
    /** Imagenes Sixel con la escala de Windows Terminal (10x20 pixeles por celda). */
    sixelLikeWt: true,
  },
  files: {
    /** Donde arranca el panel de esta PC en SFTP: el Escritorio, la carpeta del usuario u otra. */
    localStart: "desktop" as "desktop" | "home" | "custom",
    customPath: "",
  },
  /** Mientras se graba un atajo nuevo, los atajos no corren. */
  recording: false,
});

const KEY = "ethereon.prefs";

function load() {
  const d = defaults();
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    return { ...d, binds: { ...d.binds, ...saved.binds }, term: { ...d.term, ...saved.term }, files: { ...d.files, ...saved.files }, recording: false };
  } catch {
    return d;
  }
}

export const prefs = $state(load());

$effect.root(() => {
  $effect(() => {
    const out = JSON.stringify({ binds: prefs.binds, term: prefs.term, files: prefs.files });
    try {
      localStorage.setItem(KEY, out);
    } catch {}
  });
});

export function resetBinds() {
  prefs.binds = defaults().binds;
}

/** Cambia un atajo. Si otra accion lo usaba, se lo saca. */
export function setBind(a: Action, combo: string) {
  for (const x of ACTIONS) if (x.id !== a && prefs.binds[x.id] === combo) prefs.binds[x.id] = "";
  prefs.binds[a] = combo;
}

const CODE_KEYS: Record<string, string> = {
  Comma: ",",
  Period: ".",
  Slash: "/",
  Semicolon: ";",
  Quote: "'",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Minus: "-",
  Equal: "=",
  Backquote: "`",
  Space: "Space",
};

/** "Ctrl+Shift+T" a partir de un evento. Vacio si solo se aprieta un modificador. */
export function comboOf(e: KeyboardEvent): string {
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return "";
  let key: string;
  if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else if (/^Digit[0-9]$/.test(e.code)) key = e.code.slice(5);
  else key = CODE_KEYS[e.code] ?? (e.key.length === 1 ? e.key.toUpperCase() : e.key);
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Win"].filter(Boolean);
  return [...mods, key].join("+");
}

export function matches(e: KeyboardEvent, a: Action): boolean {
  const b = prefs.binds[a];
  return !!b && comboOf(e) === b;
}

/** Para mostrar: "Ctrl Shift T". */
export function pretty(combo: string): string {
  return combo ? combo.split("+").join(" ") : "Not set";
}
