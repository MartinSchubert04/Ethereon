// Los colores de la terminal de cada host (Hosts → editar → Terminal colors). Cada panel los
// pone como estilo en su elemento (ver `schemeStyle`): en un mosaico cada terminal tiene los
// de su host, y las que no tienen (locales, hosts sin esquema) heredan los de Settings.
import { app, type TermTab } from "./state.svelte";
import { schemeStyle } from "./themes";

// El fondo translucido depende de la opacidad: con cada cambio de tema se vuelve a armar.
let rev = $state(0);
window.addEventListener("themechange", () => rev++);

/** El esquema propio del host de un panel, o null si usa el general. */
export function tabScheme(m: TermTab): string | null {
  if (m.local || m.approvals || m.sftp) return null;
  return app.snap?.hosts.find((h) => h.name === m.host)?.term_scheme ?? null;
}

/** El estilo con los colores de ese esquema ("" = hereda los generales). */
export function tabStyle(m: TermTab): string {
  void rev;
  return schemeStyle(tabScheme(m));
}
