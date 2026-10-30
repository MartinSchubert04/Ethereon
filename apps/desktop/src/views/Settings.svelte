<script lang="ts">
  import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { api, type ExportFormat, type Settings } from "../lib/api";
  import { app, attempt, toast } from "../lib/state.svelte";
  import KeyBinds from "../components/KeyBinds.svelte";
  import { comboOf, matches, prefs, pretty } from "../lib/prefs.svelte";
  import Select from "../components/Select.svelte";
  import Switch from "../components/Switch.svelte";
  import Section from "../components/Section.svelte";
  import Seg from "../components/Seg.svelte";
  import Logo from "../components/Logo.svelte";
  import { iconOptions } from "../lib/icons";
  import type { LocalShell } from "../lib/api";
  import { ArrowDownUp, FileInput, FolderInput, FolderOpen, Keyboard, Palette, Play, Power, Shield, ShieldCheck, SquareTerminal, Volume2, VolumeX } from "@lucide/svelte";
  import { importPick } from "../lib/importer";
  import { INCLUDED, ptToPx, systemFonts, wtFont, type WtFont } from "../lib/termfonts";
  import { THEMES, applyTheme, currentPalette, currentTheme, palettesOf, type ThemeId } from "../lib/themes";

  let theme = $state<ThemeId>(currentTheme());
  let palette = $state(currentPalette(currentTheme()));
  // La muestra de cada tema usa la paleta que tiene elegida.
  const swatchOf = (t: ThemeId) => palettesOf(t).find((p) => p.id === currentPalette(t))?.swatch ?? ["#000", "#000", "#000"];
  const themeName = $derived(THEMES.find((t) => t.id === theme)?.name ?? "");
  const paletteName = $derived(palettesOf(theme).find((p) => p.id === palette)?.name ?? "");
  let s = $state<Settings>(structuredClone($state.snapshot(app.snap!.settings)));
  let autostart = $state(false);
  // Las shells de esta PC, para elegir el icono de cada una.
  let shells = $state<LocalShell[]>([]);
  api.localShells().then((l) => (shells = l)).catch(() => {});
  const shellIconOpts = iconOptions();
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

  // Por defecto el formato propio: lo que se exporta aca se vuelve a importar entero.
  let format = $state<ExportFormat>("json");
  let exportPw = $state("");

  const formats: { id: ExportFormat; label: string; hint: string; secrets: boolean }[] = [
    { id: "json", label: "Ethereon (JSON)", hint: "Everything in plain text: hosts, credentials, groups and host keys. Import it back here or on another PC.", secrets: true },
    { id: "open_ssh", label: "OpenSSH (Termius, VS Code)", hint: "A folder with config and key files. Termius: Import → SSH config.", secrets: true },
    { id: "bitwarden", label: "Bitwarden JSON", hint: "Unencrypted JSON. Bitwarden: Tools → Import data → Bitwarden (json).", secrets: true },
    { id: "csv", label: "CSV (hosts only)", hint: "No secrets. For a spreadsheet.", secrets: false },
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

  // El volumen se guarda al soltar la barra, sin el boton de Security, y suena para que se
  // escuche como va a quedar. Va con los ajustes guardados, no con lo que se este editando ahi.
  function saveVolume() {
    const v = s.sound_volume;
    api.soundPlay("approval", v).catch(() => {});
    api.settingsSave({ ...app.snap!.settings, sound_volume: v }).catch((e) => toast(String(e), true));
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

  // ---- fuente de la terminal
  const WT = "__wt__";
  let wt = $state<WtFont | null>(null);
  let installed = $state<string[]>([]);
  wtFont().then((f) => (wt = f));
  systemFonts().then((l) => (installed = l));
  const fontOpts = $derived.by(() => {
    const included = new Set(INCLUDED.map((f) => f.family.replace(/ Variable$/, "")));
    const nerd = (f: string) => /nerd/i.test(f);
    const others = installed.filter((f) => !included.has(f));
    // La elegida puede ser una lista (la de Windows Terminal): se muestra tal cual.
    const cur = prefs.term.fontFamily;
    const listed = !cur || cur === WT || INCLUDED.some((f) => f.family === cur) || installed.includes(cur);
    return [
      { value: "", label: "Theme default" },
      ...(listed ? [] : [{ value: cur, label: cur, hint: "chosen" }]),
      ...(wt ? [{ value: WT, label: "Same as Windows Terminal", hint: `${wt.face} · ${ptToPx(wt.size)} px` }] : []),
      ...INCLUDED.map((f) => ({ value: f.family, label: f.family.replace(/ Variable$/, ""), hint: f.note })),
      ...others.filter(nerd).map((f) => ({ value: f, label: f, hint: "Nerd Font · installed" })),
      ...others.filter((f) => !nerd(f)).map((f) => ({ value: f, label: f, hint: "installed" })),
    ];
  });
  // "Same as Windows Terminal" copia fuente y tamaño; no queda guardado como opcion aparte.
  function pickFont(v: string) {
    if (v === WT && wt) {
      prefs.term.fontFamily = wt.face;
      prefs.term.fontSize = ptToPx(wt.size);
      toast(`Using ${wt.face}, ${ptToPx(wt.size)} px`);
    }
  }

  // ---- atajo global para traer la ventana
  let hotkey = $state<string | null>(null);
  api.hotkeyGet().then((h) => (hotkey = h));
  let recordingHk = $state(false);

  async function setHotkey(combo: string | null) {
    if (await attempt(() => api.hotkeySet(combo), combo ? `Shortcut set: ${pretty(combo)}` : "Shortcut turned off")) hotkey = combo;
  }

  // Donde se abre la carpeta del Explorador: Windows Terminal (con el perfil y el tema de
  // la persona) o una pestaña de Ethereon.
  let openIn = $state<"wt" | "tab">("wt");
  let wtInstalled = $state(true);
  api.openInGet().then((o) => ((openIn = o.mode), (wtInstalled = o.wt_installed)));
  async function setOpenIn(m: "wt" | "tab") {
    if (m === "wt" && !wtInstalled) return toast("Windows Terminal is not installed", true);
    if (await attempt(() => api.openInSet(m))) openIn = m;
  }

  // Graba la combinacion: hace falta Ctrl, Alt o Win, y una letra, numero o F1-F12.
  function onHkKey(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") return stopRecordHk();
    const c = comboOf(e);
    if (!c) return;
    const key = c.split("+").pop()!;
    if (!(e.ctrlKey || e.altKey || e.metaKey) || !/^([A-Z0-9]|F([1-9]|1[0-2]))$/.test(key)) {
      toast("Use Ctrl, Alt or Win plus a letter, a number or F1 to F12", true);
      return;
    }
    stopRecordHk();
    setHotkey(c.replace("Win", "Super"));
  }
  function startRecordHk() {
    recordingHk = true;
    prefs.recording = true;
    window.addEventListener("keydown", onHkKey, true);
  }
  function stopRecordHk() {
    recordingHk = false;
    prefs.recording = false;
    window.removeEventListener("keydown", onHkKey, true);
  }

  async function pickStart() {
    const dir = (await open({ directory: true, multiple: false })) as string | null;
    if (dir) {
      prefs.files.customPath = dir;
      prefs.files.localStart = "custom";
    } else if (!prefs.files.customPath) prefs.files.localStart = "desktop";
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

    <Section id="sounds" title="Sounds" icon={Volume2} summary={s.sound_volume ? `Volume ${s.sound_volume}%` : "Muted"}>
      <div class="field">Volume
        <span class="hint">Plays when a command waits for your approval, even with the window in the tray. 0 = muted.</span>
        <div class="vol">
          {#if s.sound_volume}<Volume2 size={15} />{:else}<VolumeX size={15} />{/if}
          <input type="range" min="0" max="100" step="5" bind:value={s.sound_volume} onchange={saveVolume} aria-label="Volume" />
          <span class="pct">{s.sound_volume}%</span>
          <button class="btn" disabled={!s.sound_volume} onclick={() => api.soundPlay("approval", s.sound_volume)}><Play size={13} />Play</button>
        </div>
      </div>
    </Section>

    <Section id="terminal" title="Terminal" icon={SquareTerminal} summary="Copy, paste, font, tab names and icons">
      <div class="term-opts">
        <Switch bind:checked={prefs.term.ctrlCCopy}>Ctrl C copies when there is a selection (otherwise it stops the command)</Switch>
        <Switch bind:checked={prefs.term.ctrlVPaste}>Ctrl V pastes</Switch>
        <Switch bind:checked={prefs.term.rightClick}>Right click copies the selection, or pastes when nothing is selected</Switch>
        <Switch bind:checked={prefs.term.copyOnSelect}>Copy as soon as text is selected</Switch>
        <Switch bind:checked={prefs.term.sixelLikeWt}>Show Sixel images at the size Windows Terminal uses (10×20 pixels per cell)</Switch>
        <Switch bind:checked={prefs.term.ctrlWToShell}>Inside a terminal, Ctrl W goes to the shell (close the tab with Ctrl Shift W)</Switch>
      </div>
      <div class="grid2">
        <div class="field">Font
          <Select bind:value={prefs.term.fontFamily} options={fontOpts} onchange={pickFont} />
          <span class="hint">Nerd Font icons work with any of them</span>
        </div>
        <div class="field">Font size
          <Select
            bind:value={prefs.term.fontSize}
            options={[
              { value: 0, label: "Theme default" },
              ...[12, 13, 14, 15, 16, 18, 20].map((n) => ({ value: n, label: `${n} px` })),
            ]}
          />
        </div>
        <label class="field">Tab name follows the folder after (seconds)
          <span class="hint">Local terminals only. 0 = right away, empty = never.</span>
          <input
            type="number"
            min="0"
            placeholder="Never"
            value={prefs.term.folderTitleDelay < 0 ? "" : prefs.term.folderTitleDelay}
            oninput={(e) => {
              const v = e.currentTarget.value.trim();
              prefs.term.folderTitleDelay = v === "" ? -1 : Math.max(0, Math.round(Number(v)) || 0);
            }}
          />
        </label>
        <label class="field">Name change animation (milliseconds)
          <span class="hint">How long the letters spin. 0 = no animation.</span>
          <input
            type="number"
            min="0"
            step="100"
            value={prefs.term.scrambleMs}
            oninput={(e) => (prefs.term.scrambleMs = Math.max(0, Math.round(Number(e.currentTarget.value)) || 0))}
          />
        </label>
      </div>
      {#if shells.length}
        <div class="grid2">
          {#each shells as sh (sh.id)}
            <div class="field">{sh.label} icon
              <Select
                value={prefs.term.shellIcons[sh.id] ?? "terminal"}
                options={shellIconOpts}
                onchange={(v) => (prefs.term.shellIcons = { ...prefs.term.shellIcons, [sh.id]: v })}
              >
                {#snippet icon(v)}<Logo id={v} size={14} />{/snippet}
              </Select>
            </div>
          {/each}
        </div>
      {/if}
    </Section>

    <Section
      id="files"
      title="Files (SFTP)"
      icon={FolderOpen}
      summary="This PC starts in {prefs.files.localStart === 'custom' && prefs.files.customPath ? prefs.files.customPath : prefs.files.localStart === 'home' ? 'your user folder' : 'Desktop'}"
    >
      <div class="grid2">
        <div class="field">This PC starts in
          <Select
            bind:value={prefs.files.localStart}
            options={[
              { value: "desktop", label: "Desktop" },
              { value: "home", label: "User folder" },
              { value: "custom", label: "Another folder", hint: prefs.files.customPath },
            ]}
            onchange={(v) => v === "custom" && !prefs.files.customPath && pickStart()}
          />
        </div>
        {#if prefs.files.localStart === "custom"}
          <div class="field">Folder
            <button class="btn path-btn" onclick={pickStart}>
              <FolderOpen size={14} /><span class="ellipsis">{prefs.files.customPath || "Pick a folder..."}</span>
            </button>
          </div>
        {/if}
      </div>
      <div class="faint small">The left panel of every new SFTP tab opens here. The house button goes back to it.</div>
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
      <div class="hk">
        <div class="grow">
          <div>Show Ethereon from anywhere</div>
          <div class="faint small">
            Works with the window closed. If a File Explorer folder (or the Desktop) is in front, it opens a terminal in that
            folder.
          </div>
        </div>
        {#if recordingHk}
          <span class="kbd rec">Press the keys...</span>
          <button class="btn" onclick={stopRecordHk}>Cancel</button>
        {:else}
          <span class="kbd" class:off={!hotkey}>{hotkey ? pretty(hotkey) : "Off"}</span>
          <button class="btn" onclick={startRecordHk}>Change</button>
          {#if hotkey}
            <button class="btn" onclick={() => setHotkey(null)}>Turn off</button>
          {:else}
            <button class="btn" onclick={() => setHotkey("Ctrl+Shift+Alt+T")}>Use Ctrl Shift Alt T</button>
          {/if}
        {/if}
      </div>
      {#if hotkey}
        <div class="grid2">
          <div class="field">Open the folder in
            <Seg
              value={openIn}
              options={[
                { value: "wt", label: "Windows Terminal" },
                { value: "tab", label: "Ethereon" },
              ]}
              onchange={setOpenIn}
            />
            <span class="hint">
              {openIn === "wt"
                ? "With your Windows Terminal profile, theme and font. Ethereon stays in the tray."
                : "A PowerShell 7 tab in Ethereon."}
            </span>
          </div>
        </div>
      {/if}
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

    <Section id="export" title="Import and export" icon={ArrowDownUp} summary="Ethereon, OpenSSH, Bitwarden, CSV or an encrypted backup">
      <div class="sub">Import</div>
      <div class="row">
        <button class="btn primary" onclick={() => importPick()}><FileInput size={14} /> Import a file...</button>
        <button class="btn" onclick={() => importPick(true)}><FolderInput size={14} /> Import a folder...</button>
      </div>
      <div class="faint small">
        The format is detected by itself: Ethereon backups and exports, SSH config (Termius exports too), private
        keys and Bitwarden JSON. It adds to this vault and never replaces what is already here. An encrypted backup
        asks for its own master password.
      </div>
      <div class="sub">Export</div>
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
        <button class="btn" onclick={backup}><ShieldCheck size={14} /> Encrypted backup</button>
      </div>
    </Section>
  </div>

  <div class="faint small foot">Data folder: <code>{app.status?.data_dir}</code></div>
</div>

<style>
  .vol {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 460px;
    color: var(--text-2);
  }
  /* La barra de volumen: sin la caja de los demas inputs, con los colores del tema. */
  .vol input[type="range"] {
    flex: 1;
    height: 18px;
    padding: 0;
    border: 0;
    background: transparent;
    appearance: none;
    cursor: pointer;
  }
  .vol input[type="range"]::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: var(--line-strong);
  }
  .vol input[type="range"]::-webkit-slider-thumb {
    appearance: none;
    width: 14px;
    height: 14px;
    margin-top: -5px;
    border-radius: 50%;
    background: var(--accent);
    border: 2px solid var(--panel);
    transition: transform 0.12s;
  }
  .vol input[type="range"]:hover::-webkit-slider-thumb {
    transform: scale(1.15);
  }
  .vol input[type="range"]:focus-visible::-webkit-slider-thumb {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .pct {
    width: 36px;
    font: 11.5px var(--mono);
    color: var(--text-3);
    text-align: right;
  }
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
  .hk {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding-top: 4px;
  }
  .kbd {
    font-family: var(--mono);
    font-size: 12px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    border: var(--border) solid var(--line);
    background: var(--panel-2);
    white-space: nowrap;
  }
  .kbd.off {
    color: var(--text-3);
  }
  .kbd.rec {
    border-color: var(--accent);
    color: var(--accent);
  }
  .path-btn {
    justify-content: flex-start;
    min-width: 0;
    height: 36px;
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
