# Interface debug mode

A panel to see what's going on with tabs and terminals when something looks wrong and can't be
reproduced in a regular browser (WebView2 issues). It lives in `apps/desktop/src/lib/debug.svelte.ts`
and only shows up with `npm run dev` (or in the installed app with `localStorage["ethereon.debug"] = "1"`).
In a release it does nothing.

## Running it alongside the installed app

With `ETHEREON_DATA_DIR`, the test copy uses a different vault and doesn't hand off to the already open instance
(single-instance is skipped, see `src-tauri/src/lib.rs`). With `ETHEREON_PORT`, the MCP listens on
another port. From the repo root:

```powershell
$env:ETHEREON_DATA_DIR = "$env:TEMP\ethereon-dev"; $env:ETHEREON_PORT = 47399; npm run dev
```

It asks you to create a password (new, empty vault). To test terminals, "This PC" is enough (`Ctrl T`).

## The panel

Bottom right:

- **Mark (F9):** logs "se ve mal" ("looks wrong") with the full state of every terminal.
- **Redraw:** redraws every terminal. If this fixes it, it's a painting problem.
- **Copy / Clear:** copies or clears the log.
- **Switches** `capture`, `focus`, `tips`, `webgl`: each one turns something off to isolate a cause
  (each button's tooltip says what it does). They're saved in `localStorage["ethereon.dbgsw"]`.
- At the top, live, each visible terminal: size, rows x columns, WebGL, how many times it drew and
  how long ago, its canvases (with the middle color of the 2D ones) and which element sits on top of the center.

It logs on its own: pressing, dragging, dropping and clicking on tabs; which tab is visible; when a
terminal gains or loses focus; row and column changes; window focus, size and visibility;
WebGL context loss; and JavaScript errors (`error`, `unhandledrejection`,
`console.error`). To log something else: `dbg("qué pasó", datos, true)` (with `true` it takes snapshots of the
state at 80 ms and at 700 ms).

## The log in a file

Everything the panel logs also goes to `target/ui-debug.log`, through vite's live reload
channel (the CSP doesn't allow `fetch`; see the `debugLog` plugin in `vite.config.ts`). That way it can be read without
copying and pasting. Playwright tests against the same vite also write there.

## Solved cases

- **Black terminal on losing focus** (click on the current tab, dragging a tab):
  it only happened with a Sixel image on screen (the athanor splash). The image addon
  drew on a `desynchronized` canvas and WebView2 left the terminal black. Fixed in
  `vite.config.ts` (`imageCanvasSynchronized`). What revealed it: the `focus` switch, and that the
  draw counter didn't go up on losing focus only when there was an image.
