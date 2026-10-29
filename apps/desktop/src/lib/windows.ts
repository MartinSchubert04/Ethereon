// Pestañas entre ventanas: sacar una terminal a su propia ventana, pasarla a otra o volverla
// a la principal. Las sesiones no se cortan: la ventana que la tenia la suelta
// (term_detach) y la nueva la toma (term_attach) con lo ultimo que se vio.
//
// Cada ventana tiene una bandeja de pestañas en el backend: se dejan ahi (term_window_give o
// al crear la ventana) y la ventana las toma al abrirse o cuando le avisan ("take-tabs").
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { app, closeTerminal, newKey, toast, type TermTab } from "./state.svelte";

/** Lo que viaja de una ventana a otra. */
interface Moved {
  title: string;
  host: string;
  local?: string;
  cwd?: string;
  sftp?: boolean;
  attach?: number;
  members?: Moved[];
  active?: number;
}

const pack = (t: TermTab): Moved => ({
  title: t.title,
  host: t.host,
  local: t.local,
  cwd: t.cwd,
  sftp: t.sftp,
  attach: t.sftp ? undefined : t.sid,
  members: t.members?.map(pack),
  active: t.members ? Math.max(0, t.members.findIndex((m) => m.key === t.activeMember)) : undefined,
});

function unpack(m: Moved): TermTab {
  const members = m.members?.map(unpack);
  return {
    key: newKey(),
    host: m.host,
    local: m.local,
    cwd: m.cwd,
    sftp: m.sftp,
    title: m.title,
    alive: true,
    attach: m.attach,
    members,
    activeMember: members ? members[m.active ?? 0]?.key : undefined,
  };
}

/** Suelta las sesiones de la pestaña (y de sus miembros) para que otra ventana las tome. */
async function release(t: TermTab) {
  for (const m of t.members ?? [t]) {
    m.moving = true;
    if (!m.sftp && m.sid) await api.termDetach(m.sid).catch(() => {});
  }
  t.moving = true;
}

export const currentLabel = () => getCurrentWindow().label;

/** Las otras ventanas a las que se puede mandar una pestaña. */
export async function otherWindows(): Promise<{ label: string; name: string }[]> {
  const all = await invoke<string[]>("term_windows").catch(() => [] as string[]);
  return all
    .filter((l) => l !== currentLabel())
    .map((l) => ({ label: l, name: l === "main" ? "Main window" : `Window ${l.replace("term-", "")}` }));
}

/** Manda una pestaña a una ventana nueva (en `at`, coordenadas de pantalla) o a otra. */
export async function moveTab(key: number, target: "new" | string, at?: { x: number; y: number }) {
  const t = app.terms.find((x) => x.key === key);
  if (!t) return;
  const tabs = [pack(t)];
  await release(t);
  // Primero se entrega y despues se saca de aca: si era la ultima pestaña de una ventana de
  // terminales, la ventana se cierra al sacarla.
  try {
    if (target === "new") {
      await invoke("term_window_open", { tabs, x: at?.x ?? null, y: at?.y ?? null });
    } else {
      await invoke("term_window_give", { label: target, tabs });
      if (target === "main") await invoke("main_window_show");
      await emitTo(target, "take-tabs");
    }
  } catch (e) {
    toast(String(e), true);
  }
  closeTerminal(key);
}

async function take() {
  const tabs = await invoke<Moved[] | null>("term_window_take").catch(() => null);
  if (!tabs?.length) return;
  for (const m of tabs) {
    const t = unpack(m);
    app.terms.push(t);
    app.activeTerm = t.key;
  }
  app.view = "terminal";
  const w = getCurrentWindow();
  await w.show();
  await w.setFocus();
}

let started = false;
/** Toma las pestañas que esperan a esta ventana, ahora y cada vez que le avisen. */
export function initWindows() {
  if (started) return;
  started = true;
  listen("take-tabs", () => take());
  take();
}
