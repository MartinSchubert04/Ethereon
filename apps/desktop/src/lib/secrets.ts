// Ver y copiar secretos guardados, como en Bitwarden. Pide la contraseña maestra (el broker
// la recuerda unos minutos, ver `Broker::reveal`) y el portapapeles se limpia solo.
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { api, type Host, type Revealed } from "./api";
import { prompt } from "./confirm.svelte";
import { toast } from "./state.svelte";

/** A los cuantos ms se borra del portapapeles un secreto copiado. */
const CLEAR_MS = 30_000;

/** La credencial en claro, o null si se cancela o la contraseña no es la correcta. */
export async function reveal(id: string): Promise<Revealed | null> {
  try {
    if (await api.revealUnlocked()) return await api.revealSecret(id, null);
  } catch {
    // Se vencio justo: se pide la contraseña.
  }
  const pw = await prompt({
    title: "Master password",
    body: "To view or copy a stored secret. It isn't asked again for 5 minutes.",
    placeholder: "Master password",
    secret: true,
    confirm: "Unlock",
  });
  if (pw === null) return null;
  try {
    return await api.revealSecret(id, pw);
  } catch (e) {
    toast(String(e), true);
    return null;
  }
}

let clearTimer: ReturnType<typeof setTimeout> | undefined;

/** Copia un secreto y lo borra del portapapeles a los 30 s, si sigue estando ahi. */
export async function copySecret(text: string, what: string) {
  await writeText(text);
  toast(`${what} copied. The clipboard is cleared in 30 s.`);
  clearTimeout(clearTimer);
  clearTimer = setTimeout(async () => {
    try {
      if ((await readText()) === text) await writeText("");
    } catch {}
  }, CLEAR_MS);
}

/** Copia algo que no es secreto (un usuario, una direccion). */
export async function copyPlain(text: string, what: string) {
  await writeText(text);
  toast(`${what} copied`);
}

/** Copia la contraseña (o la clave privada) de un host. */
export async function copyHostSecret(h: Host) {
  if (!h.credential) return toast(`${h.name} has no credential`, true);
  const r = await reveal(h.credential);
  if (r) await copySecret(r.secret, r.secret.includes("PRIVATE KEY") ? "Private key" : "Password");
}
