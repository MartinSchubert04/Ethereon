import { mount } from "svelte";
import "@fontsource-variable/inter";
import "@fontsource-variable/geist-mono";
import "@fontsource/jacquard-24/latin-400.css";
// Fuentes que se pueden elegir para la terminal (solo latin, normal y negrita).
import "@fontsource/jetbrains-mono/latin-400.css";
import "@fontsource/jetbrains-mono/latin-700.css";
import "@fontsource/fira-code/latin-400.css";
import "@fontsource/fira-code/latin-700.css";
import "@fontsource/ibm-plex-mono/latin-400.css";
import "@fontsource/ibm-plex-mono/latin-700.css";
import "@fontsource/source-code-pro/latin-400.css";
import "@fontsource/source-code-pro/latin-700.css";
import "@fontsource/cascadia-mono/latin-400.css";
import "@fontsource/cascadia-mono/latin-700.css";
import "@xterm/xterm/css/xterm.css";
import "./styles/themes.css";
import "./styles/palettes.css";
import "./styles/base.css";
import "./styles/athanor.css";
import "./styles/term-fonts.css";
import App from "./App.svelte";
import TermWindow from "./TermWindow.svelte";
import { applyTheme, currentTheme } from "./lib/themes";
import { installTooltips } from "./lib/tooltip";
import { installDebug } from "./lib/debug.svelte";

applyTheme(currentTheme());

// Las ventanas de terminales sacadas de la principal cargan la misma interfaz con ?window=term.
const isTermWindow = new URLSearchParams(location.search).get("window") === "term";
const app = mount(isTermWindow ? TermWindow : App, { target: document.getElementById("app")! });

// Sin menu contextual del navegador fuera de los campos de texto y la terminal.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest("input, textarea, .xterm")) e.preventDefault();
});

// Tooltips con mas demora que los del navegador.
installTooltips();

// Panel de debug (solo con npm run dev, ver lib/debug.svelte.ts).
installDebug();

export default app;
