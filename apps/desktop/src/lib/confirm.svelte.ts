// Confirmaciones con el modal de la app (en vez del confirm() nativo de Windows).
export interface Ask {
  title: string;
  body?: string;
  confirm?: string;
  danger?: boolean;
}

export interface Prompt extends Ask {
  placeholder?: string;
  secret?: boolean;
}

export const confirmState = $state({
  open: false,
  title: "",
  body: "",
  confirm: "OK",
  danger: false,
  /** Con input, el modal pide un texto (por ejemplo una contraseña). */
  input: null as null | { placeholder: string; secret: boolean },
  value: "",
  resolve: null as ((ok: boolean) => void) | null,
});

export function ask(o: Ask): Promise<boolean> {
  return new Promise((resolve) => {
    Object.assign(confirmState, {
      title: o.title,
      body: o.body ?? "",
      confirm: o.confirm ?? "OK",
      danger: o.danger ?? false,
      input: null,
      value: "",
      open: true,
      resolve: (ok: boolean) => {
        confirmState.open = false;
        resolve(ok);
      },
    });
  });
}

/** Como ask, pero pide un texto. Devuelve null si se cancela. */
export function prompt(o: Prompt): Promise<string | null> {
  return new Promise((resolve) => {
    Object.assign(confirmState, {
      title: o.title,
      body: o.body ?? "",
      confirm: o.confirm ?? "OK",
      danger: o.danger ?? false,
      input: { placeholder: o.placeholder ?? "", secret: o.secret ?? false },
      value: "",
      open: true,
      resolve: (ok: boolean) => {
        confirmState.open = false;
        resolve(ok ? confirmState.value : null);
      },
    });
  });
}
