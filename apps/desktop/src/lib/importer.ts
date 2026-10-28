// Importar cualquier cosa: Ethereon (respaldo cifrado, JSON, CSV, carpeta OpenSSH),
// SSH config, claves y Bitwarden. El formato lo reconoce el broker mirando el contenido.
// Siempre suma a lo que hay; nunca reemplaza el vault.
import { open } from "@tauri-apps/plugin-dialog";
import { api, errText, type ImportSummary } from "./api";
import { prompt } from "./confirm.svelte";
import { toast } from "./state.svelte";

function report(s: ImportSummary, label: string) {
  const what = [s.hosts && `${s.hosts} hosts`, s.credentials && `${s.credentials} credentials`].filter(Boolean).join(" and ");
  toast(`${label}: ${what || "nothing new"}${s.skipped ? `, ${s.skipped} already there` : ""}`);
  for (const n of s.notes.slice(0, 4)) toast(n);
}

/** Importa un archivo o carpeta ya elegido (o soltado sobre la ventana). */
export async function importPath(path: string): Promise<boolean> {
  try {
    const d = await api.importDetect(path);
    let password: string | null = null;
    if (d.needs_password) {
      password = await prompt({
        title: "Import an Ethereon backup",
        body: "Type the master password of that backup. Its hosts, credentials, groups and host keys are added to this vault; nothing here is replaced.",
        placeholder: "Master password of the backup",
        confirm: "Import",
        secret: true,
      });
      if (password === null) return false;
    }
    report(await api.importAuto(path, password), `Imported ${d.label}`);
    return true;
  } catch (e) {
    toast(errText(e), true);
    return false;
  }
}

/** Elegir un archivo (o una carpeta) y importarlo. */
export async function importPick(folder = false) {
  const path = (await open({ multiple: false, directory: folder })) as string | null;
  if (path) await importPath(path);
}

/** El ~/.ssh/config de esta PC. */
export async function importLocalSsh() {
  try {
    report(await api.importSshConfig(null), "Imported ~/.ssh/config");
  } catch (e) {
    toast(errText(e), true);
  }
}
