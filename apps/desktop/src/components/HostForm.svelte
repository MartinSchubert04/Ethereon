<script lang="ts">
  import { untrack } from "svelte";
  // El formulario trabaja sobre una copia: se monta de nuevo cada vez que se abre.
  import Drawer from "./Drawer.svelte";
  import PolicyEditor from "./PolicyEditor.svelte";
  import Select from "./Select.svelte";
  import Seg from "./Seg.svelte";
  import Switch from "./Switch.svelte";
  import Logo from "./Logo.svelte";
  import { ICONS, hostIcon, iconOptions, osIcon } from "../lib/icons";
  import { X } from "@lucide/svelte";
  import { flip } from "svelte/animate";
  import { flipMs, pop } from "../lib/motion";
  import { api, defaultPolicy, newHost, type Env, type Host } from "../lib/api";
  import { app, attempt } from "../lib/state.svelte";
  import { ask } from "../lib/confirm.svelte";

  let { host, onclose }: { host: Host | null; onclose: () => void } = $props();
  const init = untrack(() => host);

  // Copia editable; `original` es el nombre con el que estaba guardado.
  const original = init?.name ?? null;
  let h = $state<Host>(init ? structuredClone($state.snapshot(init)) : newHost());
  let tags = $state(h.tags.join(", "));
  let jump = $state<string[]>([...h.jump]);
  let ownRules = $state(h.policy !== null);
  let policy = $state(h.policy ?? defaultPolicy());
  h.icon ??= null;
  // "Automatic" dice que se detecto; si todavia no se conecto nunca, que se va a detectar.
  const detected = osIcon(h.os);
  const iconOpts = [
    { value: null as string | null, label: "Automatic", hint: detected ? ICONS[detected].label : "detected when you connect" },
    ...iconOptions().map((o) => ({ ...o, value: o.value as string | null })),
  ];


  const loginCreds = $derived(
    (app.snap?.credentials ?? []).filter((c) => ["password", "private_key", "certificate"].includes(c.kind)),
  );
  const sudoCreds = $derived((app.snap?.credentials ?? []).filter((c) => c.kind === "sudo_password" || c.kind === "password"));
  const others = $derived((app.snap?.hosts ?? []).filter((x) => x.name !== original && !jump.includes(x.name)));
  const envs: { value: Env; label: string }[] = [
    { value: "none", label: "None" },
    { value: "prod", label: "prod" },
    { value: "test", label: "test" },
    { value: "dev", label: "dev" },
  ];
  const groupOpts = $derived([
    { value: null as string | null, label: "No group" },
    ...(app.snap?.groups ?? []).map((g) => ({ value: g.name as string | null, label: g.name, hint: g.policy.mode.replace("_", " ") })),
  ]);
  const credOpts = $derived(
    loginCreds.map((c) => ({ value: c.id, label: c.id, hint: [c.username, c.kind.replace("_", " ")].filter(Boolean).join(" · ") })),
  );
  const sudoOpts = $derived([
    { value: null as string | null, label: "None" },
    ...sudoCreds.map((c) => ({ value: c.id as string | null, label: c.id, hint: c.kind.replace("_", " ") })),
  ]);
  let addJump = $state<string | null>(null);

  // Login: escribir una contraseña aca mismo o usar una credencial del keychain. La
  // contraseña escrita se guarda igual en el vault, como "<host>-password".
  let login = $state<"password" | "saved">(init?.credential ? "saved" : "password");
  let password = $state("");
  // Si la credencial elegida trae usuario, el del host puede quedar vacio.
  const credUser = $derived(login === "saved" ? (app.snap?.credentials.find((c) => c.id === h.credential)?.username ?? "") : "");

  function newCredId(): string {
    const ids = new Set((app.snap?.credentials ?? []).map((c) => c.id));
    const base = `${h.name.trim() || "host"}-password`;
    let id = base;
    for (let n = 2; ids.has(id); n++) id = `${base}-${n}`;
    return id;
  }

  const split = (s: string) =>
    s
      .split(",")
      .map((x) => x.trim())
      .filter(Boolean);

  async function save() {
    let created: string | null = null;
    if (login === "password") {
      if (!password) return attempt(() => Promise.reject("Type the password, or pick a saved credential"));
      created = newCredId();
      const id = created;
      const ok = await attempt(() =>
        api.credSave({ id, original: null, kind: "password", username: h.user.trim(), secret: password, passphrase: null, certificate: null, note: `Login for ${h.name}` }),
      );
      if (!ok) return;
      h.credential = id;
    }
    const out: Host = {
      ...h,
      port: Number(h.port) || 22,
      tags: split(tags),
      jump: [...jump],
      group: h.group || null,
      sudo_credential: h.sudo_credential || null,
      policy: ownRules ? policy : null,
    };
    if (await attempt(() => api.hostSave(out, original), "Host saved")) onclose();
    else if (created) {
      // El host no se guardo: no dejar una credencial suelta.
      await api.credDelete(created).catch(() => {});
      h.credential = init?.credential ?? "";
    }
  }

  async function remove() {
    if (original && (await ask({ title: `Delete ${original}?`, body: "Its credentials stay in the keychain.", confirm: "Delete host", danger: true }))) {
      if (await attempt(() => api.hostDelete(original), "Host deleted")) onclose();
    }
  }
</script>

<Drawer title={original ? `Edit ${original}` : "New host"} {onclose} onsave={save}>
  <div class="grid2">
    <label class="field">Name<input bind:value={h.name} placeholder="web-1" /><span class="hint">What the agent calls it</span></label>
    <div class="field">Group<Select bind:value={h.group} options={groupOpts} /></div>
  </div>
  <div class="addr">
    <label class="field">Hostname or IP<input bind:value={h.hostname} placeholder="10.0.0.5" /></label>
    <label class="field">Port<input type="number" bind:value={h.port} /></label>
  </div>
  <div class="grid2">
    <label class="field">User<input bind:value={h.user} placeholder={credUser ? `${credUser} (from the credential)` : "deploy"} /></label>
    <div class="field">Login<Seg bind:value={login} options={[{ value: "password", label: "Password" }, { value: "saved", label: "Saved credential" }]} /></div>
  </div>
  {#if login === "password"}
    <label class="field">Password
      <input type="password" bind:value={password} autocomplete="new-password" placeholder={init?.credential ? "New password for this host" : ""} />
      <span class="hint">Saved encrypted in the vault as {newCredId()}. Keys and certificates go in Keychain.</span>
    </label>
  {:else}
    <div class="field">Credential<Select bind:value={h.credential} options={credOpts} placeholder="Pick one" /></div>
  {/if}
  <div class="grid2">
    <div class="field">Sudo password<Select bind:value={h.sudo_credential} options={sudoOpts} /></div>
    <div class="field">Environment<Seg bind:value={h.env} options={envs} /><span class="hint">Only a label: it colors the host and tells the agent it is production</span></div>
  </div>
  <div class="field">Icon
    <Select bind:value={h.icon} options={iconOpts}>
      {#snippet lead()}<Logo id={hostIcon(h)} size={14} />{/snippet}
    </Select>
  </div>
  <div class="field">
    Jump through
    <div class="jumps">
      {#each jump as j, i (j)}
        <span class="jchip" animate:flip={{ duration: flipMs() }} transition:pop>
          <span class="n">{i + 1}</span>{j}
          <button type="button" aria-label="Remove {j}" onclick={() => (jump = jump.filter((x) => x !== j))}><X size={12} /></button>
        </span>
      {/each}
      {#if others.length}
        <div class="add">
          <Select
            bind:value={addJump}
            options={others.map((o) => ({ value: o.name as string | null, label: o.name, hint: o.hostname }))}
            placeholder={jump.length ? "Then through..." : "Connect directly"}
            onchange={(v) => {
              if (v) jump = [...jump, v];
              addJump = null;
            }}
          />
        </div>
      {/if}
    </div>
    <span class="hint">Bastions in order. The first one is reached from this PC.</span>
  </div>
  <label class="field">Tags<input bind:value={tags} placeholder="nginx, docker" /><span class="hint">Comma separated</span></label>
  <label class="field">Description
    <textarea bind:value={h.description} rows="3" placeholder="Public web server. Nginx in front of the app in Docker."></textarea>
    <span class="hint">The agent reads this: what the host is for, what runs there</span>
  </label>

  <div class="section-title">Agent rules</div>
  <Switch bind:checked={ownRules}>
    Own rules for this host {#if !ownRules}<span class="faint">(uses the group's, or Approval)</span>{/if}
  </Switch>
  {#if ownRules}<div transition:pop={{ start: 0.98 }}><PolicyEditor bind:policy /></div>{/if}

  {#snippet footer()}
    {#if original}<button class="btn danger" onclick={remove}>Delete</button>{/if}
    <span class="grow"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" onclick={save}>Save</button>
  {/snippet}
</Drawer>

<style>
  .jumps {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .jchip {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 6px 0 6px;
    border-radius: var(--radius-sm);
    background: var(--panel);
    border: var(--border) solid var(--line-strong);
    color: var(--text);
    font-weight: 550;
  }
  .jchip .n {
    display: grid;
    place-items: center;
    min-width: 20px;
    height: 20px;
    border-radius: 5px;
    font-size: 11px;
    background: var(--accent);
    color: var(--accent-ink);
  }
  .jchip button {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 5px;
    color: var(--text-3);
  }
  .jchip button:hover {
    background: var(--hover);
    color: var(--danger);
  }
  .add {
    flex: 1;
    min-width: 180px;
  }
  .addr {
    display: grid;
    grid-template-columns: 1fr 90px;
    gap: 12px;
  }
</style>
