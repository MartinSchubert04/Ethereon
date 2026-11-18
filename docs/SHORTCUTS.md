# Shortcuts and conveniences

Everything you can do with the keyboard (and some mouse gestures) in Ethereon.
Shortcuts marked with ✎ can be changed in **Settings → Keyboard shortcuts**; the rest are fixed.

## From anywhere in Windows

| Shortcut | What it does |
|---|---|
| `Ctrl Shift Alt T` | If the active window is an Explorer folder (or the Desktop), opens a terminal in that folder, like "Abrir en Terminal" ("Open in Terminal"). Otherwise, brings Ethereon to the front (even if it's closed to the tray) with PowerShell in your Desktop; if a local terminal is already open, it goes to that one instead of opening another |

It can be changed or turned off in **Settings → Startup and updates**. If another app already uses the combination,
it warns and the previous one is kept. That's also where you choose where the folder opens:

- **Windows Terminal** (default): with your profile, your theme and your font. Ethereon stays in the
  tray.
- **Ethereon**: a PowerShell 7 tab. With the vault locked, it opens when you unlock it.

## Throughout the app

| Shortcut | What it does |
|---|---|
| `Ctrl T` ✎ | New tab: search for a host or a shell on this PC |
| `Ctrl Shift T` ✎ | New local terminal (the first shell in the list, usually PowerShell 7) |
| `Ctrl R` ✎ | Refresh the current tab: in SFTP it reloads the folders; in a tab that didn't connect or got cut off, it reconnects right there. In a connected terminal, `Ctrl R` belongs to the shell (history search) |
| `Ctrl W` ✎ | Close the tab (inside a terminal it goes to the shell if configured that way) |
| `Ctrl Shift W` ✎ | Close the tab, also from a terminal |
| `Ctrl Tab` / `Ctrl Shift Tab` ✎ | Next / previous tab |
| `Ctrl 1` … `Ctrl 9` | Go to tab N (`Ctrl 1` is the vault) |
| `Ctrl B` ✎ | Show or hide the sidebar (in a terminal, it floats on top) |
| `Ctrl F` ✎ | Search hosts |
| `Ctrl N` ✎ | New host. In Keychain: new credential |
| `Ctrl ,` ✎ | Settings |
| `Ctrl Shift L` ✎ | Lock the vault |

The app never reloads: `F5`, `Ctrl R` and `Ctrl Shift R` don't close the tabs like in a
browser.

## Tabs and windows

- **Reorder:** drag a tab over another. The tab goes under the mouse and the others
  shift to make room for it.
- **Move it to its own window:** drag it out of the bar (downward or outside the
  window) and drop it; the new window appears where it's dropped. Also with right-click →
  **Move to new window**.
- **Move it to another window or back to the main one:** right-click → **Move to …**. A terminal
  window can have several tabs, and it closes on its own when it has none left.
- The session isn't cut when moving: the terminal is still the same one, with what was on screen.
  (SFTP tabs are reopened in the other window.)

## Several terminals in one tab (tiling)

- **Combine:** drag a tab downward, over a terminal, and drop it on one of its
  edges (left, right, top or bottom): that space is split in two. To add it to another
  tab, hover over that tab for half a second while dragging: it's shown, and then you
  move down to its terminal. Also works across windows. A tab that already has several
  terminals is added whole.
- **Approvals next to a terminal:** drag **Approvals** from the sidebar (the vault's
  or the one that opens with ☰) to a tab in the bar, or directly onto an edge of the
  visible terminal. Same as with a pane: dropping it on the tab adds it on the right, and
  hovering shows the tab. Each tab can have its own: the ones in other tiled layouts stay
  where they are (they all show the same list). If that tab already has them, that pane is moved
  (there can't be two in the same one). A click on Approvals still opens its view.
- **Width and height:** drag the divider between two terminals.
- **Rearrange:** drag a terminal by its name (top left, on the edge) to
  another side of another one. **Make it a standalone tab again:** drop it on an empty spot of the tab
  bar.
- **Move it to another tab:** drag it by its name to that tab in the bar. Dropping it
  there adds it at the far right; hovering for half a second shows that tab, and then you
  move down and drop it on the edge you want. If the other one wasn't a tiled layout, it becomes one. The
  session isn't cut.
- **Rearrange with the keyboard:** with `Ctrl Shift` held, the arrows move a cursor through the
  tiled layout and a marker shows where the focused terminal would go. It only moves when you release
  `Ctrl Shift`; `Esc` cancels.
  - Toward another terminal: the cursor jumps to the nearest one on that side, remembering the row or
    column it was coming from. With two on the left (top and bottom) and one on the right, from the
    right `←` goes to the top one and from there `↓` to the bottom one. Solid marker = they swap;
    dashed over its own = it stays where it is.
  - With nothing else on that side: the marker moves to half of that terminal (it goes to that side,
    splitting its spot). The same arrow again: that whole side of the tiled layout.
  - The opposite arrow undoes the last step.

  Only in a tiled layout: otherwise, `Ctrl Shift ←/→` still belongs to the shell (select by
  word). It can be changed in Settings → Shortcuts (modifiers count).
- **Switch from one to another:** `Shift Tab` (in order, and from the last it goes back to the first). Also
  in group tabs. With a single terminal in the tab, `Shift Tab` still belongs to the
  shell (in PowerShell it completes backwards). It can be changed in Settings → Shortcuts.
- **Close one:** the `×` on its edge, or `Ctrl Shift W` in the focused one.
- If a connection drops, the others keep going. The drop shows the path (this PC, the jumps, the
  host) with the broken hop and a **Retry** button. `Ctrl R` retries all dropped ones in the
  tab and leaves the connected ones as they are; if none dropped, `Ctrl R` belongs to the shell.
- Moving terminals within the window doesn't reconnect anything: it's the same terminal, live, in another
  place. When it gets narrower, xterm reflows what was already written (like any terminal);
  an image that was in that history may end up split.

## Groups

- **Open all** (in the title of each group in Hosts) opens all its hosts in a single tab,
  like Termius: on the left the "Terminals" list with the state of each one (Connecting,
  Connected, Disconnected) and on the right the selected one.
- In that list: click or `↑` `↓` to switch, `×` or middle-click to close one.
  `Ctrl Shift W` closes only the selected terminal; with the last one the tab closes.
- **Reorder hosts:** drag the card; a line marks where it lands. Dropping it in another group
  moves it there (if the agent would then run without asking, it asks for confirmation).

## New tab search (`Ctrl T`)

| Shortcut | What it does |
|---|---|
| Type | Filters by name, address, group, tags or description |
| `↑` `↓` | Select |
| `Enter` | Open the host's terminal (or the local shell) |
| `Shift Enter` / `Shift` + click | Open the host's files (SFTP) |
| `Esc` | Close |

## Forms (host, group, credential)

| Shortcut | What it does |
|---|---|
| `Ctrl S` ✎ or `Ctrl Enter` | Save |
| `Esc` | Close without saving |

- When creating a host you can type the password right there (**Login → Password**). It's stored
  encrypted in the vault as `<host>-password`; there's no need to create it first in Keychain.
- If the credential has a username, the host's **User** field can be left empty and the
  credential's is used.
- **Environment** starts at **None**. It's just a label (prod, test, dev): it colors the host and
  tells the agent that a machine is production. It doesn't change any rule.
- In **Settings**, `Ctrl S` saves the Security section (everything else saves itself).
- `Esc` in a confirmation dialog closes only the dialog, not the form behind it.

## Terminal

| Shortcut | What it does |
|---|---|
| `Ctrl Shift C` / `Ctrl Shift V` ✎ | Copy / paste |
| `Ctrl C` | Copies if there's selected text; otherwise, interrupts the process (configurable) |
| `Ctrl V` | Pastes (configurable) |
| Right-click | Copies the selection or pastes if there's nothing selected (configurable) |
| `Ctrl =` / `Ctrl -` ✎ | Bigger / smaller font |
| `Ctrl 0` ✎ | Back to the theme's size |

- The font is chosen in **Settings → Terminal → Font**: the theme's, the same as Windows Terminal
  (copies font and size), one of the bundled ones (JetBrains Mono, Fira Code, IBM Plex Mono, Source
  Code Pro, Cascadia Mono, and the themes' fonts) or any installed one. Nerd Font icons show up
  with any of them. The font is set by the terminal, not the shell: the PowerShell theme doesn't change it.
- Local terminals don't inherit variables from the terminal Ethereon was opened from
  (`WT_SESSION`, `NO_COLOR`…) and identify themselves as `TERM_PROGRAM=Ethereon`, so the
  PowerShell profile knows where it is.
- They show Sixel images (`chafa -f sixel`, `img2sixel`…). For that Ethereon uses Windows
  Terminal's ConPTY (`apps/desktop/src-tauri/conpty`): Windows' own discards images. By
  default it scales them like Windows Terminal (10×20 pixels per cell), so what was designed for Windows
  Terminal looks the same; it can be changed in **Settings → Terminal**.
  The Sixel decoder is WebAssembly: the CSP in `tauri.conf.json` needs `'wasm-unsafe-eval'`
  (without it, images don't show in the installed app; in `npm run dev` there's no CSP and they do show).
- With pixel fonts (PxPlus and similar) bold is drawn like in Windows Terminal: the
  glyph repeated one pixel to the right, crisp. The browser would thicken it with antialiasing and it would
  look blurry (`pixelBold` plugin in `vite.config.ts`).
- Local terminals use the real Windows shell (PowerShell 7, Windows PowerShell, CMD, WSL,
  Git Bash, whichever are installed), with your profile, oh-my-posh included.
- Block and box-drawing characters (`█ ▀ ▄ ─ │`), and also the sextants and octants used by
  `chafa` and MOTDs with images, are drawn pixel-exact, with no stripes between rows or glyphs from another
  font. The sextant and octant table comes from `apps/desktop/scripts/legacy_glyphs.py`.

## Files (SFTP)

It's opened with each host's **SFTP** button or with `Shift Enter` in the search. It starts with this PC on
the left and the server on the right, but **each side can be changed from its title**: this PC or
any host. That way you upload, download, copy from one server to another (directly, without going through this
PC's disk) or within this PC. If one side doesn't connect, the other keeps working and from the same title
you pick another.

| Shortcut | What it does |
|---|---|
| Type letters | Jump to the first item that starts that way; the same letter again goes to the next one |
| `↑` `↓` `Home` `End` | Move (with `Shift`, select several) |
| Click / `Ctrl` click / `Shift` click | Select one / add or remove / select a range |
| `Ctrl A` | Select all |
| `Enter` / double-click | Open the folder; on a file, send it to the other side |
| `Space` or `Ctrl Enter` | Copy the selection to the other side |
| `Backspace` / `Alt ←` | Go up one folder |
| `F2` | Rename |
| `Delete` | Delete (asks first; folders are deleted with everything in them) |
| `Ctrl Shift N` | New folder (ready to be named) |
| `F5` or `Ctrl R` | Reload |
| `Ctrl F` | Filter the folder |
| `Ctrl H` | Show or hide hidden files (the ones starting with a dot) |
| `Esc` | Clear the selection |
| `Tab` | Switch to the other pane |

- **Dragging** files from one pane to the other transfers them. Dropping them on a folder puts them
  inside it.
- You can also **drag from Windows Explorer** to either of the two panes.
- Right-click opens a menu with all the actions.
- This PC's pane starts on the Desktop. It can be changed in **Settings → Files (SFTP)**: Desktop,
  user folder or another folder. The house button goes back there.
- Click the path at the top to type one by hand (`Enter` to go). The breadcrumbs lead to
  each folder; on this PC, the disk icon shows the drives.
- Before overwriting something that already exists, it asks.
- Transfers run in the background with progress, speed and a button to cancel. A
  file cancelled halfway is deleted.
- Every action on the server is recorded in the audit (`[sftp] upload …`, `delete …`, etc.).
- With the vault locked nothing on the server is touched. On unlock, the tab reconnects on its own.

## Import

**Hosts → Import → A file...** (or **Settings → Import and export**), or drop the file on the
Hosts page. The format is detected automatically, by looking at the content:

- Ethereon encrypted backup (asks for that backup's master password).
- Ethereon export in JSON (the default export format) or CSV.
- Folder exported in OpenSSH format, or a `.ssh` folder (**A folder...**).
- SSH config (also what Termius exports), private keys and Bitwarden JSON.

It always adds to the current vault: whatever already exists with the same name is left as is, and already
confirmed server fingerprints aren't overwritten. Agent tokens aren't imported.

## Settings

Sections open and close with a click; Ethereon remembers which ones you left open.
