import { mount } from "svelte";
import "@fontsource-variable/inter";
import "@fontsource-variable/geist-mono";
import "@xterm/xterm/css/xterm.css";
import "./styles/themes.css";
import "./styles/palettes.css";
import "./styles/base.css";
import App from "./App.svelte";
import { applyTheme, currentTheme } from "./lib/themes";

applyTheme(currentTheme());

const app = mount(App, { target: document.getElementById("app")! });

// Sin menu contextual del navegador fuera de los campos de texto y la terminal.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest("input, textarea, .xterm")) e.preventDefault();
});

export default app;
