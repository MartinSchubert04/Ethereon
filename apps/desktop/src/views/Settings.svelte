<script lang="ts">
  import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { save } from "@tauri-apps/plugin-dialog";
  import { api, type ExportFormat, type Settings } from "../lib/api";
  import { app, attempt } from "../lib/state.svelte";
  import KeyBinds from "../components/KeyBinds.svelte";
  import { matches, prefs } from "../lib/prefs.svelte";
  import Select from "../components/Select.svelte";
  import Switch from "../components/Switch.svelte";
  import Section from "../components/Section.svelte";
  import { Download, Keyboard, Palette, Power, Shield, SquareTerminal } from "@lucide/svelte";
  import { THEMES, applyTheme, currentPalette, currentTheme, palettesOf, type ThemeId } from "../lib/themes";

  let theme = $state<ThemeId>(currentTheme());
  let palette = $state(currentPalette(currentTheme()));
  // La muestra de cada tema usa la paleta que tiene elegida.
  const swatchOf = (t: ThemeId) => palettesOf(t).find((p) => p.id === currentPalette(t))?.swatch ?? ["#000", "#000", "#000"];
  const themeName = $derived(THEMES.find((t) => t.id === theme)?.name ?? "");
  const paletteName = $derived(palettesOf(theme).find((p) => p.id === palette)?.name ?? "");
  let s = $state<Settings>(structuredClone($state.snapshot(app.snap!.settings)));
  let autostart = $state(false);
  isEnabled().then((v) => (autostart = v)).catch(() => {});

  let version = $state("");
  api.version().then((v) => (version = v));
  let checking = $state(false);
  let upToDate = $state(false);
  async function checkUpdate() {
    checking = true;
    upToDate = false;
    try {
      upToDate = !(await api.updateCheck());
    } catch (e) {
      attempt(() => Promise.reject(e));
    } finally {
      checking = false;
    }
  }

  let oldPw = $state("");
  let newPw = $state("");

  let format = $state<ExportFormat>("open_ssh");
  let exportPw = $state("");

  const formats: { id: ExportFormat; label: string; hint: string; secrets: boolean }[] = [
    { id: "open_ssh", label: "OpenSSH (Termius, VS Code)", hint: "A folder with config and key files. Termius: Import → SSH config.", secrets: true },
    { id: "bitwarden", label: "Bitwarden JSON", hint: "Unencrypted JSON. Bitwarden: Tools → Import data → Bitwarden (json).", secrets: true },
    { id: "csv", label: "CSV (hosts only)", hint: "No secrets. For a spreadsheet.", secrets: false },
    { id: "json", label: "Ethereon JSON", hint: "Everything in plain text, to migrate or inspect.", secrets: true },
  ];
  const fmt = $derived(formats.find((f) => f.id === format)!);

  function pickTheme(t: ThemeId) {
    theme = t;
    palette = currentPalette(t);
    applyTheme(t, palette);
  }

  function pickPalette(p: string) {
    palette = p;
    applyTheme(theme, p);
  }

  async function saveSettings() {
    await attempt(() => api.settingsSave({ ...s, agent_enabled: app.snap!.settings.agent_enabled }), "Settings saved");
  }

  async function toggleAutostart() {
    await attempt(async () => {
      if (autostart) await disable();
      else await enable();
      autostart = await isEnabled();
    });
  }

  async function changePw() {
    if (newPw.length < 8) return attempt(() => Promise.reject("At least 8 characters"));
    if (await attempt(() => api.changePassword(oldPw, newPw), "Master password changed")) oldPw = newPw = "";
  }

  async function doExport() {
    const ext = format === "csv" ? "csv" : format === "open_ssh" ? "" : "json";
    const path = await save({ defaultPath: format === "open_ssh" ? "ethereon-ssh" : `ethereon-export.${ext}` });
    if (!path) return;
    if (await attempt(() => api.exportData(format, path, fmt.secrets ? exportPw : null).then((m) => m), "Exported")) {
      exportPw = "";
    }
  }

  async function backup() {
    const path = await save({ defaultPath: "ethereon-vault-backup.json" });
    if (path) attempt(() => api.backup(path), "Encrypted backup saved");
  }
</script>

<svelte:window
  onkeydown={(e) => {
    // Ctrl S guarda los ajustes de seguridad (lo demas se guarda solo).
    if (matches(e, "save") && !document.querySelector(".drawer, [role=alertdialog]")) {
      e.preventDefault();
      saveSettings();
    }
  }}
/>

<div class="page">
  <div class="page-head"><h1>Settings</h1></div>

  <div class="sections">
    <Section id="appearance" title="Appearance" icon={Palette} summary="{themeName} · {paletteName}">
      <div class="sub">Theme</div>
      <div class="themes">
        {#each THEMES as t}
          {@const sw = theme === t.id ? (palettesOf(t.id).find((p) => p.id === palette)?.swatch ?? swatchOf(t.id)) : swatchOf(t.id)}
          <button class="theme card" class:on={theme === t.id} onclick={() => pickTheme(t.id)}>
            <div class="sw" style="background: linear-gradient(135deg, {sw[0]} 0 45%, {sw[1]} 45% 75%, {sw[2]} 75%)"></div>
            <span>{t.name}</span>
          </button>
        {/each}
      </div>
      <div class="sub">Palette for {themeName}</div>
      <div class="palettes">
        {#each palettesOf(theme) as p (p.id)}
          <button class="pal" class:on={palette === p.id} onclick={() => pickPalette(p.id)}>
            <span class="dots">
              {#each p.swatch as c}<span style="background: {c}"></span>{/each}
            </span>
            {p.name}
          </button>
        {/each}
      </div>
    </Section>

    <Section
      id="security"
      title="Security"
      icon={Shield}
      summary={s.autolock_minutes ? `Locks after ${s.autolock_minutes} min idle` : "Never locks by itself"}
    >
      <div class="grid2">
        <label class="field">Lock after (minutes without using Ethereon) <span class="hint">0 = never. Agent requests do not keep it open.</span>
          <input type="number" min="0" bind:value={s.autolock_minutes} />
        </label>
        <label class="field">Approval wait (seconds) <span class="hint">After that the command is rejected</span>
          <input type="number" min="10" bind:value={s.approval_timeout_secs} />
        </label>
        <label class="field">Keep idle SSH connections (minutes)
          <input type="number" min="1" bind:value={s.keep_alive_minutes} />
        </label>
        <div class="chk"><Switch bind:checked={s.audit_output}>Save the (masked) output in the audit log, not only its hash</Switch></div>
      </div>
      <div><button class="btn primary" onclick={saveSettings}>Save</button></div>
      <div class="sub">Master password</div>
      <div class="grid2">
        <input type="password" placeholder="Current password" bind:value={oldPw} />
        <input type="password" placeholder="New password (8+ characters)" bind:value={newPw} />
      </div>
      <div><button class="btn" disabled={!oldPw || !newPw} onclick={changePw}>Change</button></div>
    </Section>

    <Section id="terminal" title="Terminal" icon={SquareTerminal} summary="Copy, paste and font size">
      <div class="term-opts">
        <Switch bind:checked={prefs.term.ctrlCCopy}>Ctrl C copies when there is a selection (otherwise it stops the command)</Switch>
        <Switch bind:checked={prefs.term.ctrlVPaste}>Ctrl V pastes</Switch>
        <Switch bind:checked={prefs.term.rightClick}>Right click copies the selection, or pastes when nothing is selected</Switch>
        <Switch bind:checked={prefs.term.copyOnSelect}>Copy as soon as text is selected</Switch>
        <Switch bind:checked={prefs.term.ctrlWToShell}>Inside a terminal, Ctrl W goes to the shell (close the tab with Ctrl Shift W)</Switch>
      </div>
      <div class="grid2">
        <div class="field">Font size
          <Select
            bind:value={prefs.term.fontSize}
            options={[
              { value: 0, label: "Theme default" },
              ...[12, 13, 14, 15, 16, 18, 20].map((n) => ({ value: n, label: `${n} px` })),
            ]}
          />
        </div>
      </div>
    </Section>

    <Section id="keys" title="Keyboard shortcuts" icon={Keyboard} summary="Change any shortcut">
      <KeyBinds />
    </Section>

    <Section
      id="app"
      title="Startup and updates"
      icon={Power}
      summary="Ethereon {version}{autostart ? ' · starts with Windows' : ''}"
    >
      <Switch checked={autostart} onchange={toggleAutostart}>Start with Windows, hidden in the tray</Switch>
      <div class="faint small">Closing the window keeps Ethereon in the tray. Quit from the tray menu.</div>
      <div class="row">
        <span class="grow">Ethereon {version}</span>
        {#if app.update?.status === "ready"}
          <button class="btn primary" onclick={() => api.updateNow()}>Install {app.update.version} now</button>
        {:else}
          <button class="btn" onclick={checkUpdate} disabled={checking || !!app.update}>
            {checking ? "Checking..." : app.update ? `Downloading ${app.update.version}...` : "Check for updates"}
          </button>
        {/if}
      </div>
      {#if upToDate}<div class="muted">You have the latest version.</div>{/if}
      <div class="faint small">
        Updates download in the background and install by themselves when the window is closed, the vault is locked and
        no agent command is running. Installing restarts Ethereon, so the vault ends up locked.
      </div>
    </Section>

    <Section id="export" title="Export and backup" icon={Download} summary="OpenSSH, Bitwarden, CSV, JSON or an encrypted copy">
      <div class="grid2">
        <div class="field">Format<Select bind:value={format} options={formats.map((f) => ({ value: f.id, label: f.label }))} /></div>
        {#if fmt.secrets}
          <label class="field">Master password
            <input type="password" bind:value={exportPw} />
            <span class="hint">The export has your secrets in plain text</span>
          </label>
        {/if}
      </div>
      <div class="faint small">{fmt.hint}</div>
      <div class="row">
        <button class="btn primary" disabled={fmt.secrets && !exportPw} onclick={doExport}>Export</button>
        <button class="btn" onclick={backup}>Encrypted backup of the vault</button>
      </div>
    </Section>
  </div>

  <div class="faint small foot">Data folder: <code>{app.status?.data_dir}</code></div>
</div>

<style>
  .themes {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 12px;
  }
  .theme {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    text-align: left;
    font-weight: 600;
  }
  .theme.on {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .palettes {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .pal {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    height: 34px;
    padding: 0 13px 0 8px;
    border-radius: 999px;
    background: var(--panel);
    border: var(--border) solid var(--line);
    font-weight: 550;
    transition: border-color 0.15s;
  }
  .pal:hover {
    border-color: var(--line-strong);
  }
  .pal.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .dots {
    display: inline-flex;
  }
  .dots span {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 1.5px solid var(--panel);
    margin-left: -5px;
  }
  .dots span:first-child {
    margin-left: 0;
  }
  .sw {
    height: 56px;
    border-radius: var(--radius-sm);
    border: 1px solid rgba(127, 127, 127, 0.25);
  }
  .sections {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .sub {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin-bottom: -6px;
  }
  .term-opts {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .chk {
    align-self: end;
    padding-bottom: 8px;
  }
  .small {
    font-size: 12px;
  }
  .foot {
    margin-top: 24px;
  }
</style>
