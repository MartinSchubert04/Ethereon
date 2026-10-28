// Confirmaciones con el modal de la app (en vez del confirm() nativo de Windows).
export interface Ask {
  title: string;
  body?: string;
  confirm?: string;
  danger?: boolean;
}

export const confirmState = $state({
  open: false,
  title: "",
  body: "",
  confirm: "OK",
  danger: false,
  resolve: null as ((ok: boolean) => void) | null,
});

export function ask(o: Ask): Promise<boolean> {
  return new Promise((resolve) => {
    Object.assign(confirmState, {
      title: o.title,
      body: o.body ?? "",
      confirm: o.confirm ?? "OK",
      danger: o.danger ?? false,
      open: true,
      resolve: (ok: boolean) => {
        confirmState.open = false;
        resolve(ok);
      },
    });
  });
}
