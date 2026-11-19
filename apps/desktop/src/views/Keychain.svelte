<script lang="ts">
  import { Copy, Eye, EyeOff, FileKey, KeyRound, Pencil, Plus, UserRound } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import Drawer from "../components/Drawer.svelte";
  import Select from "../components/Select.svelte";
  import { api, KIND_LABEL, type CredKind, type CredView, type Revealed } from "../lib/api";
  import { app, attempt, toast } from "../lib/state.svelte";
  import { ask } from "../lib/confirm.svelte";
  import { copyPlain, copySecret, reveal } from "../lib/secrets";

  let editing = $state<CredView | null | undefined>(undefined);
  let id = $state("");
  let kind = $state<CredKind>("password");
  let secret = $state("");
  let passphrase = $state("");
  let certificate = $state("");
  let note = $state("");
  let username = $state("");
  /** El secreto guardado, en claro, mientras se muestra en el cajon. */
  let shown = $state<Revealed | null>(null);
  // Solo las credenciales de login llevan usuario.
  const hasUser = $derived(kind === "password" || kind === "private_key" || kind === "certificate");

  const kinds: CredKind[] = ["password", "private_key", "certificate", "sudo_password", "api_token"];
  const isKeyKind = (k: CredKind) => k === "private_key" || k === "certificate";
  const isKey = $derived(isKeyKind(kind));
  const secretName = (k: CredKind) => (isKeyKind(k) ? "private key" : k === "api_token" ? "token" : "password");

  // Ctrl N en el Keychain abre una credencial nueva.
  $effect(() => {
    if (app.pendingNew === "credential") {
      app.pendingNew = null;
      edit(null);
    }
  });

  function edit(c: CredView | null) {
    editing = c;
    id = c?.id ?? "";
    kind = c?.kind ?? "password";
    note = c?.note ?? "";
    username = c?.username ?? "";
    secret = passphrase = certificate = "";
    shown = null;
  }

  function close() {
    editing = undefined;
    shown = null;
  }

  async function toggleShow(c: CredView) {
    shown = shown ? null : await reveal(c.id);
  }

  /** Copia el secreto de una credencial (desde la tabla o desde el cajon). */
  async function copyOf(c: CredView, field: "secret" | "passphrase" = "secret") {
    const r = shown ?? (await reveal(c.id));
    if (!r) return;
    const what = field === "passphrase" ? "Passphrase" : secretName(c.kind);
    await copySecret(r[field], what[0].toUpperCase() + what.slice(1));
  }

  async function save() {
    const c = editing;
    // Vacio al editar = se deja el que estaba.
    const keep = (v: string, had: boolean | undefined) => (v === "" && had ? null : v);
    const ok = await attempt(
      () =>
        api.credSave({
          id: id.trim(),
          original: c?.id ?? null,
          kind,
          username: hasUser ? username : "",
          secret: keep(secret, c?.has_secret),
          passphrase: isKey ? keep(passphrase, c?.has_passphrase) : null,
          certificate: kind === "certificate" ? keep(certificate, c?.has_certificate) : null,
          note,
        }),
      "Credential saved",
    );
    if (ok) close();
  }

  async function remove(c: CredView) {
    if (await ask({ title: `Delete ${c.id}?`, body: "The secret is erased from the vault. This cannot be undone.", confirm: "Delete", danger: true }))
      await attempt(() => api.credDelete(c.id), "Deleted");
  }

  async function importKey() {
    const path = (await open({ multiple: false })) as string | null;
    if (!path) return;
    try {
      const s = await api.importKeyFile(path);
      toast(s.credentials ? "Key imported" : "A credential with that name already exists");
    } catch (e) {
      toast(String(e), true);
    }
  }
</script>

<div class="page">
  <div class="page-head">
    <h1>Keychain</h1>
    <span class="sub">Agents only see these names. You can view or copy a secret with the master password.</span>
    <span class="grow"></span>
    <button class="btn" onclick={importKey}><FileKey size={14} /> Import key file</button>
    <button class="btn primary" onclick={() => edit(null)}><Plus size={15} /> New credential</button>
  </div>

  {#if !app.snap?.credentials.length}
    <div class="empty">
      <KeyRound size={30} />
      <h3>No credentials yet</h3>
      <div>Passwords, private keys, certificates, sudo passwords and API tokens.</div>
    </div>
  {:else}
    <div class="card">
      <table class="list">
        <thead><tr><th>Name</th><th>User</th><th>Type</th><th>Used by</th><th>Note</th><th></th></tr></thead>
        <tbody>
          {#each app.snap.credentials as c (c.id)}
            <tr class="click" onclick={() => edit(c)}>
              <td class="mono">{c.id}</td>
              <td class="mono muted">{c.username || "—"}</td>
              <td><span class="chip">{KIND_LABEL[c.kind]}</span>{#if c.has_passphrase} <span class="chip">passphrase</span>{/if}</td>
              <td class="muted">{c.used_by.join(", ") || "—"}</td>
              <td class="faint ellipsis note">{c.note}</td>
              <td class="right nowrap">
                {#if c.username}
                  <button class="icon-btn" title="Copy user" onclick={(e) => (e.stopPropagation(), copyPlain(c.username, "User"))}><UserRound size={14} /></button>
                {/if}
                {#if c.has_secret}
                  <button class="icon-btn" title="Copy {secretName(c.kind)}" onclick={(e) => (e.stopPropagation(), copyOf(c))}><Copy size={14} /></button>
                {/if}
                <button class="icon-btn" aria-label="Edit"><Pencil size={14} /></button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

{#if editing !== undefined}
  <Drawer title={editing ? `Edit ${editing.id}` : "New credential"} onclose={close} onsave={save}>
    <div class="grid2">
      <label class="field">Name<input bind:value={id} class="mono" placeholder="bastion-key" /></label>
      <div class="field">Type<Select bind:value={kind} options={kinds.map((k) => ({ value: k, label: KIND_LABEL[k] }))} /></div>
    </div>
    {#if hasUser}
      <div class="field">User
        <span class="with-btn">
          <input bind:value={username} class="mono" placeholder="deploy" autocomplete="off" spellcheck="false" aria-label="User" />
          {#if username}<button class="icon-btn" title="Copy user" onclick={() => copyPlain(username, "User")}><Copy size={14} /></button>{/if}
        </span>
        <span class="hint">Optional. Hosts that leave their user empty log in with this one.</span>
      </div>
    {/if}
    {#if editing?.has_secret}
      {@const c = editing}
      <div class="field">
        Stored {secretName(c.kind)}
        <span class="with-btn">
          {#if shown && isKeyKind(c.kind)}
            <textarea class="mono" rows="7" readonly value={shown.secret}></textarea>
          {:else}
            <input class="mono" readonly value={shown ? shown.secret : "••••••••••••"} aria-label="Stored {secretName(c.kind)}" />
          {/if}
          <button class="icon-btn" title={shown ? "Hide" : "Show"} onclick={() => toggleShow(c)}>
            {#if shown}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
          </button>
          <button class="icon-btn" title="Copy" onclick={() => copyOf(c)}><Copy size={14} /></button>
        </span>
        {#if shown?.passphrase}
          <span class="with-btn">
            <input class="mono" readonly value={shown.passphrase} aria-label="Passphrase" />
            <button class="icon-btn" title="Copy passphrase" onclick={() => copyOf(c, "passphrase")}><Copy size={14} /></button>
          </span>
        {/if}
        <span class="hint">Asks for the master password, then not again for 5 minutes. A copied secret is cleared from the clipboard after 30 s.</span>
      </div>
    {/if}
    {#if isKey}
      <label class="field">{editing?.has_secret ? "Replace private key" : "Private key"} <span class="hint">OpenSSH, PEM or PuTTY</span>
        <textarea class="mono" rows="7" bind:value={secret} placeholder={editing?.has_secret ? "Paste a new key to replace the stored one." : "-----BEGIN OPENSSH PRIVATE KEY-----"}></textarea>
      </label>
      <label class="field">Passphrase <span class="hint">If the key has one</span>
        <input type="password" bind:value={passphrase} placeholder={editing?.has_passphrase ? "Stored. Type to replace." : ""} />
      </label>
      {#if kind === "certificate"}
        <label class="field">Certificate <span class="hint">Contents of id_xxx-cert.pub</span>
          <textarea class="mono" rows="3" bind:value={certificate} placeholder={editing?.has_certificate ? "Stored. Paste to replace." : "ssh-ed25519-cert-v01@openssh.com ..."}></textarea>
        </label>
      {/if}
    {:else}
      <label class="field">{editing?.has_secret ? `Replace ${secretName(kind)}` : kind === "api_token" ? "Token" : "Password"}
        <input type="password" bind:value={secret} placeholder={editing?.has_secret ? "Type to replace the stored one." : ""} />
      </label>
    {/if}
    <label class="field">Note<input bind:value={note} /></label>
    {#snippet footer()}
      {#if editing}<button class="btn danger" onclick={() => editing && remove(editing).then(close)}>Delete</button>{/if}
      <span class="grow"></span>
      <button class="btn" onclick={close}>Cancel</button>
      <button class="btn primary" onclick={save}>Save</button>
    {/snippet}
  </Drawer>
{/if}

<style>
  .note {
    max-width: 260px;
  }
  .right {
    text-align: right;
  }
  .nowrap {
    white-space: nowrap;
  }
  .with-btn {
    display: flex;
    align-items: flex-start;
    gap: 6px;
  }
  .with-btn > :is(input, textarea) {
    flex: 1;
    min-width: 0;
  }
  .with-btn + .with-btn {
    margin-top: 6px;
  }
</style>
