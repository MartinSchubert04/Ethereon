# DESIGN.md

Ethereon's design system. Format: [awesome-design-md](https://github.com/VoltAgent/awesome-design-md)
(nine sections). Read before touching the interface.

## 1. Visual theme and atmosphere

A desktop SSH client, calm and dense like Termius: sidebar on the left, grouped host cards, and the
terminal as the protagonist when it's open. Security shows in the states (environment, rules mode,
pending approvals, agent access), not in decoration.

- **Density:** medium. 250 px cards, tables for long lists (keychain, audit, devices).
- **Voice:** literal. "Approve", "Reject", "Agent access", "Lock vault". No metaphors.
- **Four themes, one structure.** Colors, borders, shadows and backgrounds change; never the layout or
  the type.

## 2. Colors and roles

All colors are variables in `apps/desktop/src/styles/themes.css`.

Each theme also has an opacity (Settings → Appearance, 30–100 %, each theme remembers its own).
Below 100 % the window is frosted glass: the desktop shows through behind it, blurred (Windows
Acrylic, like in the Haze app) and the theme's background and surfaces become translucent. It's
handled by `lib/themes.ts` on `<body>`; the themes don't know about it.

What floats over other content (the ≡ sidebar over a terminal, search, menus, dialogs, drawers,
toasts, tooltips) doesn't follow the opacity: it uses `--float`, the theme's `--sidebar` painted
over `--bg` at 97 %, so its text never mixes with what's behind. `--float` only exists below
100 %; floating components write `background: var(--float, <their usual background>)`.

| Variable | Role |
|---|---|
| `--bg`, `--bg-image` | Window background (Warp and Haze use gradients, Poolsuite a sunset) |
| `--sidebar`, `--panel`, `--panel-2` | Surfaces, from furthest back to furthest front |
| `--line`, `--line-strong`, `--border` | Borders and their width (2 px in Poolsuite) |
| `--text`, `--text-2`, `--text-3` | Primary, secondary and tertiary text |
| `--accent`, `--accent-ink`, `--primary-bg` | Accent, text on the accent, primary button (can be a gradient) |
| `--danger`, `--warn`, `--ok` | Rejected or blocked; pending; allowed or active |
| `--env-prod`, `--env-test`, `--env-dev` | Host environment (badge and chip) |
| `--term-*` | Terminal background, text, cursor and selection |

Terminal colors can be set separately from the theme (Settings → Terminal → Colors, like in
Termius): a scheme overrides `--term-bg`, `--term-fg`, `--term-cursor`, `--term-selection`,
`--term-selection-fg` and `--term-ansi` with a style on `<html>`. They're in
`apps/desktop/scripts/termschemes.json` (each with its source), which generates `lib/termschemes.ts`.
"Theme colors" (empty) keeps the theme's.

Each host can have its own (edit host → Terminal colors, `term_scheme` in the vault). They're
applied with the same variables as a style on the pane (`lib/hostcolors.svelte.ts`), not on `<html>`:
in a tiled layout each terminal has its host's colors, with its frame, and the rest inherit the global ones.

Syntax highlighting (off by default; per tab with right click, or for every terminal in Settings → Terminal) colors IPs, MACs, interfaces, up / down, errors, prompts and config lines with the scheme's ANSI colors (bright ones on a dark background), painted over the text with xterm decorations (`lib/highlight.ts`): the data and what gets copied stay plain. Profiles: network gear (serial hosts, and hosts with a switch / router / AP / firewall icon or tag), Linux and Windows; an SSH host without hints gets Linux + network.

Each theme has palettes (Settings → Palette). They're defined in `apps/desktop/scripts/palettes.py`, which
generates `styles/palettes.css` and `lib/palettes.gen.ts`; each theme remembers its own.

| Theme | Palettes |
|---|---|
| Default | Mint, Nord, Dracula, Tokyo Night, Catppuccin, Gruvbox, Paper (light) |
| Warp | Violet, Solar Flare, Pink City, Jellyfish, Leafy, Red Rock |
| Poolsuite FM | Sunset, Mint Pool, Tropicana, Lavender, Beach, Platinum (Mac OS 9), Monochrome (dark, like the new Poolsuite app: dithered near-black with cream text and borders), Vellum |
| Haze | Cream, Peach, Rose, Mauve, Sky, Sage, Amber (Catppuccin accents, with the haze tinted to match) |
| Athanor | Srcery, Umber, Cinnabar, Vellum (light), Orpiment (light): the ones from portfolio-athanor |

Each theme also has an "athanor-style" palette (Srcery, Ember, Vellum, Amber): warm black,
cream and a single amber accent.

| Theme | Background | Accent | Trait |
|---|---|---|---|
| Default | `#0d0f14` | mint `#6ee7b7` | Flat, sober |
| Warp | black with violet and cyan lights | gradient `#8b5cf6 → #22d3ee` | Translucent panels with blur |
| Poolsuite FM | pastel sunset (sky, pink, peach) | pink `#ff4f9a` | Light, 2 px black borders, hard shadows, striped window bar like the classic Mac |
| Haze | warm and cool haze over blue-gray | cream `#f9e2af` | Glass, just like the Haze app |
| Athanor | `#1C1B19` (Srcery) | amber `#FBB829` | Pixel clay: stair-stepped corners, hard bevels, checkerboard-dithered shadows; bitmap font |

## 3. Typography

Inter Variable for everything and Geist Mono Variable for commands, addresses, fingerprints and the terminal.
Both are bundled (no network).

**Exception: Athanor**, requested this way on purpose so it matches the portfolio. All text in
PxPlus IBM VGA 8x16 at exactly 16 px and without antialiasing (it's bitmap: at any other size it looks blurry), no
bold or italic; headings in Jacquard 24 at 48 px. See `styles/athanor.css` and `src/fonts/`.

| Use | Size | Weight |
|---|---|---|
| Page title | 21 px | 650 |
| Drawer title | 15 px | 650 |
| Text | 13.5 px | 400 to 550 |
| Section labels | 11.5 px uppercase | 650 |
| Mono | 12.5 px (13 px in the terminal) | 400 |

## 4. Components

- **Host card:** badge in the environment's color, name, `user@host` in mono, description on
  two lines, chips (environment, mode, jump, tags) and Connect button. The test and edit icons
  appear on hover.
- **Drawer:** forms on the right, over a scrim. Footer with Delete on the left and
  Cancel / Save on the right.
- **Chip:** 20 px pill. Variants `prod`, `test`, `dev`, `ok`, `warn`, `bad`.
- **Segmented control:** rules mode and environment.
- **Approval card:** host + environment + sudo, reason in italics, warnings, the editable command
  in mono, and the actions. A file request shows instead a box with the
  direction (Download / Upload), the size, the path in mono and, if it uploads something from this PC, from where;
  it can only be approved or rejected.
- **Fingerprint alert:** strip above the content; red if the key changed.
- **Agent access button:** always visible at the bottom of the sidebar; green on, red off.
- **Select** (`components/Select.svelte`): replaces the native `<select>`. Floating list with a
  check on the selected one, a gray hint per option, arrows / Enter / Esc / typing to jump, and
  a search box if there are more than 7 options.
- **Switch**: replaces single-setting checkboxes. **Seg**: segmented control with a sliding
  pill. The remaining checkboxes (lists) are custom, with an animated checkmark.
- **Confirm** (`components/Confirm.svelte`, `lib/confirm.svelte.ts`): confirmation modal. Replaces
  Windows' `confirm()`. Red when the action removes protections or deletes something.
- **Dragging hosts**: a copy of the card follows the cursor, the target group is highlighted with a
  dashed border and empty groups show "Drop here".
- **≡ menu in a terminal**: the sidebar appears floating on top; it closes by clicking outside,
  with ≡ or with Esc.
- **Fields**: help text goes below the field (`.field .hint`), so fields in a row stay
  aligned even if one has help and another doesn't.

## 5. Layout

Like Termius. The window has no Windows frame: at the top there's a 46 px bar that is both
title bar and tab strip (`components/Titlebar.svelte`):

`≡` (collapse the sidebar) · **Vault** tab · one tab per terminal · `+ New tab` · drag
area · pending approvals · minimize, maximize, close (to the tray).

Below, in the Vault tab, the sidebar (216 px, or 58 px collapsed with icons only) and the
content. In a terminal tab there's no sidebar: the terminal uses the full width. Terminals
stay mounted when switching tabs (the session isn't lost).

"New tab" (Ctrl T) opens a host search; Enter opens the terminal. Shortcuts are in
`lib/shortcuts.ts` and are listed in Settings.

## 5b. Motion

All animations go through `lib/motion.ts` and last 0 with Windows' "reduce animations".

| What | How |
|---|---|
| Switching views | moves up 6 px and fades in (220 ms) |
| Drawer | enters from the right with a bit of spring; the background blurs |
| Dialogs, search, dropdowns | grow from 95 % (160 ms) |
| Host cards | appear staggered (35 ms apart) and lift 2 px on hover |
| Buttons | rise 1 px on hover, sink when pressed; the primary has a shine that sweeps across it |
| Switch | the thumb bounces and stretches when pressed |
| Wrong password | the card shakes |
| Toasts | enter from the bottom and rearrange (flip) |

In Athanor motion is stepped (`steps()`), like pixel art.

## 6. Depth

Default, Warp and Haze: no shadows on cards; depth through surfaces and borders. Poolsuite:
hard offset shadows (`3px 3px 0`) on cards, primary buttons and the active item.

## 7. Do's and don'ts

- Do: states with color and text at once (a red chip says "denied").
- Do: secrets hidden by default; stored fields show "Stored. Type to replace." Viewing or copying
  one (Keychain, host card) asks for the master password (`lib/secrets.ts`, then 5 minutes of
  grace) and a copied secret is cleared from the clipboard after 30 s.
- Don't: hardcoded colors in components. If one is needed, it's a new variable in all four themes.
- Don't: long animations. Only the drawer entrance (180 ms) and hovers.

## 8. Responsive behavior

Minimum window 860 x 560. The host grid arranges itself (`auto-fill`, 250 px).

## 9. Agent guide

Before changing the interface: read this file and `styles/themes.css`. Test in all four themes
(Settings → Theme). `npm run check` with no warnings.
