// Probar un host y, si hace falta, pedir que se confirme la huella (una por salto).
import { api, type SshError } from "./api";
import { attempt, toast } from "./state.svelte";

type KeyError = Extract<SshError, { kind: "host_key_unknown" | "host_key_changed" }>;

export const prompt = $state({ error: null as KeyError | null, resolve: null as ((ok: boolean) => void) | null });

function ask(error: KeyError): Promise<boolean> {
  return new Promise((resolve) => {
    prompt.error = error;
    prompt.resolve = (ok) => {
      prompt.error = null;
      resolve(ok);
    };
  });
}

/** Confirma la huella de un error de host (desde una alerta). */
export async function confirmKey(error: KeyError): Promise<boolean> {
  if (!(await ask(error))) return false;
  const fp = error.kind === "host_key_unknown" ? error.fingerprint : error.got;
  const alg = error.kind === "host_key_unknown" ? error.algorithm : "";
  return attempt(() => api.hostTrust(error.addr, fp, alg));
}

/** Prueba la conexion hasta que anda o la persona cancela. Devuelve si quedo lista. */
export async function testHost(name: string, quiet = false): Promise<boolean> {
  for (let i = 0; i < 4; i++) {
    let res: SshError | null;
    try {
      res = await api.hostTest(name);
    } catch (e) {
      toast(String(e), true);
      return false;
    }
    if (!res) {
      if (!quiet) toast(`${name}: connection OK`);
      return true;
    }
    if (res.kind !== "host_key_unknown" && res.kind !== "host_key_changed") return false;
    if (!(await confirmKey(res))) return false;
  }
  return false;
}
