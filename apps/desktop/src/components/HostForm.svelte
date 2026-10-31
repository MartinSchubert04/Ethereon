<script lang="ts">
  import { untrack } from "svelte";
  // El formulario trabaja sobre una copia: se monta de nuevo cada vez que se abre.
  import Drawer from "./Drawer.svelte";
  import PolicyEditor from "./PolicyEditor.svelte";
  import Select from "./Select.svelte";
  import Seg from "./Seg.svelte";
  import Switch from "./Switch.svelte";
  import Logo from "./Logo.svelte";
  import { ICONS, iconOptions, osIcon } from "../lib/icons";
  import { RefreshCw, X } from "@lucide/svelte";
  import { flip } from "svelte/animate";
  import { flipMs, pop } from "../lib/motion";
  import { api, DEFAULT_PROMPT, defaultPolicy, defaultSerial, newHost, type Env, type Host, type Parity, type SerialPortInfo, type Transport } from "../lib/api";
  import { app, attempt } from "../lib/state.svelte";
  import { ask } from "../lib/confirm.svelte";

  let { host, onclose }: { host: Host | null; onclose: () => void } = $props();
  const init = untrack(() => host);

  // Copia editable; `original` es el nombre con el que estaba guardado.
  const original = init?.name ?? null;
  let h = $state<Host>(init ? structuredClone($state.snapshot(init)) : newHost());
  // Hosts guardados antes de que existiera el transporte serial.
  h.transport ??= "ssh";
  h.serial ??= defaultSerial();
  let tags = $state(h.tags.join(", "));
  let jump = $state<string[]>([...h.jump]);
  let ownRules = $state(h.policy !== null);
  let policy = $state(h.policy ?? defaultPolicy());
  h.icon ??= null;
  const serial = $derived(h.transport === "serial");
  // "Automatic" dice que se detecto; si todavia no se conecto nunca, que se va a detectar.
  const detected = osIcon(h.os);
  const iconOpts = $derived([
    {
      value: null as string | null,
      label: "Automatic",
      hint: serial ? ICONS.cable.label : detected ? ICONS[detected].label : "detected when you connect",
    },
    ...iconOptions().map((o) => ({ ...o, value: o.value as string | null })),
  ]);
  const autoIcon = $derived(serial ? "cable" : (detected ?? "server"));

  const transports: { value: Transport; label: string }[] = [
    { value: "ssh", label: "SSH" },
    { value: "serial", label: "Serial console" },
  ];

  // ---- consola serial: los puertos de esta PC, mas el guardado si ya no esta enchufado.
  let ports = $state<SerialPortInfo[]>([]);
  let scanning = $state(false);
  async function scanPorts() {
    scanning = true;
    ports = await api.serialPorts().catch(() => []);
    scanning = false;
  }
  // Al pasar a Serial (o al abrir un host serial) se buscan los puertos una vez.
  $effect(() => {
    if (serial && !ports.length) scanPorts();
  });
  const portOpts = $derived.by(() => {
    const list = ports.map((p) => ({ value: p.name as string | null, label: p.name, hint: p.label || undefined }));
    if (h.serial.port && !ports.some((p) => p.name === h.serial.port)) list.unshift({ value: h.serial.port, label: h.serial.port, hint: "not plugged in" });
    return list;
  });
  const baudOpts = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200].map((b) => ({ value: b, label: String(b) }));
  const dataBitsOpts = [8, 7].map((b) => ({ value: b, label: `${b} data bits` }));
  const parityOpts: { value: Parity; label: string }[] = [
    { value: "none", label: "No parity" },
    { value: "even", label: "Even parity" },
    { value: "odd", label: "Odd parity" },
  ];
  const stopBitsOpts = [1, 2].map((b) => ({ value: b, label: `${b} stop bit${b > 1 ? "s" : ""}` }));

  const loginCreds = $derived(
    (app.snap?.credentials ?? []).filter((c) => (serial ? c.kind === "password" : ["password", "private_key", "certificate"].includes(c.kind))),
  );
  const sudoCreds = $derived((app.snap?.credentials ?? []).filter((c) => c.kind === "sudo_password" || c.kind === "password"));
  const others = $derived((app.snap?.hosts ?? []).filter((x) => x.name !== original && !jump.includes(x.name) && x.transport !== "serial"));
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
  // contraseña escrita se guarda igual en el vault, como "<host>-password". En una consola
  // serial el login es opcional: muchos equipos no piden nada.
  let login = $state<"none" | "password" | "saved">(init?.credential ? "saved" : init?.transport === "serial" ? "none" : "password");
  let password = $state("");
  const loginOpts = $derived([
    ...(serial ? [{ value: "none" as const, label: "No login" }] : []),
    { value: "password" as const, label: "Password" },
    { value: "saved" as const, label: "Saved credential" },
  ]);
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
    if (login === "none" && !serial) login = "password";
    if (login === "password") {
      if (!password) return attempt(() => Promise.reject("Type the password, or pick a saved credential"));
      created = newCredId();
      const id = created;
      const ok = await attempt(() =>
        api.credSave({ id, original: null, kind: "password", username: h.user.trim(), secret: password, passphrase: null, certificate: null, note: `Login for ${h.name}` }),
      );
      if (!ok) return;
      h.credential = id;
    } else if (login === "none") {
      h.credential = "";
    }
    const out: Host = {
      ...h,
      port: Number(h.port) || 22,
      serial: { ...h.serial, baud: Number(h.serial.baud) || 9600, timeout_secs: Number(h.serial.timeout_secs) || 20, prompt: h.serial.prompt.trim() || DEFAULT_PROMPT },
      tags: split(tags),
      jump: serial ? [] : [...jump],
      group: h.group || null,
      sudo_credential: serial ? null : h.sudo_credential || null,
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
    <label class="field">Name<input bind:value={h.name} placeholder={serial ? "sw-core" : "web-1"} /><span class="hint">What the agent calls it</span></label>
    <div class="field">Group<Select bind:value={h.group} options={groupOpts} /></div>
  </div>
  <div class="field">Connection
    <Seg bind:value={h.transport} options={transports} />
    <span class="hint">{serial ? "A console cable (switch, router, access point). The agent sends the device's own commands." : "A server over the network, with optional jumps through bastions."}</span>
  </div>

  {#if serial}
    <div class="addr" transition:pop={{ start: 0.98 }}>
      <div class="field">Serial port
        <div class="port-row">
          <Select bind:value={h.serial.port} options={portOpts} placeholder={ports.length ? "Pick one" : "No ports found"} />
          <button type="button" class="icon-btn" title="Scan ports again" aria-label="Scan ports again" class:spin={scanning} onclick={scanPorts}><RefreshCw size={14} /></button>
        </div>
        <span class="hint">Plug the cable in and scan if it is missing</span>
      </div>
      <div class="field">Baud rate<Select bind:value={h.serial.baud} options={baudOpts} /></div>
    </div>
    <div class="grid3">
      <div class="field"><Select bind:value={h.serial.data_bits} options={dataBitsOpts} /></div>
      <div class="field"><Select bind:value={h.serial.parity} options={parityOpts} /></div>
      <div class="field"><Select bind:value={h.serial.stop_bits} options={stopBitsOpts} /></div>
    </div>
    <div class="prompt">
      <label class="field">Prompt pattern<input class="mono" bind:value={h.serial.prompt} placeholder={DEFAULT_PROMPT} />
        <span class="hint">Regex for the end of the output: a command is done when the device shows its prompt again</span>
      </label>
      <label class="field">Prompt timeout (s)<input type="number" min="1" bind:value={h.serial.timeout_secs} /><span class="hint">How long to wait for the prompt</span></label>
    </div>
  {:else}
    <div class="addr" transition:pop={{ start: 0.98 }}>
      <label class="field">Hostname or IP<input bind:value={h.hostname} placeholder="10.0.0.5" /></label>
      <label class="field">Port<input type="number" bind:value={h.port} /></label>
    </div>
  {/if}

  <div class="grid2">
    <label class="field">User<input bind:value={h.user} placeholder={credUser ? `${credUser} (from the credential)` : serial ? "admin (if the console asks)" : "deploy"} /></label>
    <div class="field">Login<Seg bind:value={login} options={loginOpts} /></div>
  </div>
  {#if login === "password"}
    <label class="field">Password
      <input type="password" bind:value={password} autocomplete="new-password" placeholder={init?.credential ? "New password for this host" : ""} />
      <span class="hint">Saved encrypted in the vault as {newCredId()}. {serial ? "Typed when the console asks for it." : "Keys and certificates go in Keychain."}</span>
    </label>
  {:else if login === "saved"}
    <div class="field">Credential<Select bind:value={h.credential} options={credOpts} placeholder="Pick one" /></div>
  {:else}
    <div class="field"><span class="hint">The console does not ask for a user or password. Commands from the agent still follow the host's rules.</span></div>
  {/if}
  <div class="grid2">
    {#if serial}
      <div class="field">Enable password<Select bind:value={h.serial.enable_credential} options={sudoOpts} /><span class="hint">Typed after `enable` if the prompt ends in `&gt;`</span></div>
    {:else}
      <div class="field">Sudo password<Select bind:value={h.sudo_credential} options={sudoOpts} /></div>
    {/if}
    <div class="field">Environment<Seg bind:value={h.env} options={envs} /><span class="hint">Only a label: it colors the host and tells the agent it is production</span></div>
  </div>
  <div class="field">Icon
    <Select bind:value={h.icon} options={iconOpts}>
      {#snippet icon(v)}<Logo id={v ?? autoIcon} size={14} />{/snippet}
    </Select>
  </div>
  {#if !serial}
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
  {/if}
  <label class="field">Tags<input bind:value={tags} placeholder={serial ? "cisco, switch" : "nginx, docker"} /><span class="hint">Comma separated</span></label>
  <label class="field">Description
    <textarea bind:value={h.description} rows="3" placeholder={serial ? "Core switch, Cisco IOS. Console cable at the rack." : "Public web server. Nginx in front of the app in Docker."}></textarea>
    <span class="hint">The agent reads this: what the host is for, what runs there</span>
  </label>

  <div class="section-title">Agent rules</div>
  <Switch bind:checked={ownRules}>
    Own rules for this host {#if !ownRules}<span class="faint">(uses the group's, or Approval)</span>{/if}
  </Switch>
  {#if ownRules}<div transition:pop={{ start: 0.98 }}><PolicyEditor bind:policy {serial} /></div>{/if}

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
    grid-template-columns: 1fr 120px;
    gap: 12px;
  }
  .grid3 {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
  }
  .prompt {
    display: grid;
    grid-template-columns: 1fr 150px;
    gap: 12px;
  }
  .port-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .port-row > :first-child {
    flex: 1;
    min-width: 0;
  }
  .spin :global(svg) {
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
