# Plan

Ethereon's build plan. Each stage has what it delivers, how it's verified and what state it's in.
The original spec is in [spec.md](spec.md).

## Goal

A Termius-style SSH manager that also serves as a gateway for agents. The app is the only one that has
the credentials and the one that opens the connections. The agent (Claude Code over MCP) asks "run this command
on host X" and receives only the output, already masked. The person uses the same app as an SSH client,
with its own terminal.

**Done means:** `cargo test` passes, the spec's acceptance tests pass against real SSH
servers (`tests/ssh-fixture`), `npm run check` gives no errors or warnings, and with the window closed
a single small process remains in the tray that keeps serving the agent.

## Stages

| # | Stage | Delivers | Verification | Status |
|---|---|---|---|---|
| 1 | Vault, hosts, `run`, masking and audit | `crates/ethereon-core`: `vault`, `model`, `ssh`, `mask`, `audit`, `mcp`, `broker`. Bastion jumps, TOFU fingerprints, sudo via stdin, connection pool, auto-lock, master switch, per-device tokens | 20 unit tests. `tests/acceptance.rs` against two containers (bastion + internal): the spec's 5 tests, plus MCP over real HTTP | Done |
| 2 | Rules and approvals | `shell` (parser for pipes, `;`, `$( )`, backticks, `sh -c`, `eval`, `find -exec`, `sudo`, `xargs`, braces), `policy` (read-only, approval, free, no agent; global list; time, output and commands-per-minute limits), approval queue with approve, reject, per-session pattern and edit | `shell` and `policy` tests. Edited approval tested from the interface with sudo and a jump | Done |
| 3 | Server mode for multiple PCs | Headless `ethereon-server` binary with the same `Broker`; HTTPS (rustls) with its own or a Let's Encrypt certificate; minimal web interface or the desktop one connected remotely to unlock and approve; push to the phone (ntfy) | See "Stage 3" below | Pending. Already there: per-device tokens with expiry, allowed hosts and immediate revocation; the MCP transport is HTTP |
| 4 | Full interface with terminal | Svelte 5: Termius-style hosts by group, keychain, rules with "test a command", approvals, agents, audit with verification and export, settings. xterm.js terminal over PTY. Four themes (Default, Warp, Poolsuite FM, Haze) with 25 palettes. Custom title bar with Termius-style tabs, keyboard shortcuts, and the window comes back with the same position, size and maximized state. Import `~/.ssh/config`, key files and Bitwarden; export OpenSSH (Termius), Bitwarden, CSV, JSON and encrypted backup | `svelte-check` clean. Screenshots of each view and of the four themes driving the WebView through DevTools. Terminal tested through the bastion | Done |
| 5 | Publishing | GitHub repo, CI (`desktop.yml`: tests, types and build), Release by tag with signed installer and automatic updates (`updater.rs`) | The first push runs the full CI | Done. The `TAURI_SIGNING_PRIVATE_KEY` secret is loaded and releases publish `latest.json` |
| 6 | Serial console | `serial.rs`: second transport (console cable for switches, APs, routers). `Host::transport` (SSH or Serial, old vaults are read as SSH) with port, baud rate, 8N1, prompt pattern, timeout and login and `enable` credentials. Prompt-based `exec` (no exit code: reads up to the prompt, discards the echo, answers `--More--`), one port per host shared between the terminal and the agent, closed on lock. Same rules, approvals, masking and audit; base read-only list for network gear (`show`, `display`, `get`...), replaceable by the host's or group's. Form with SSH / Serial selector and port list; raw terminal | `serial` tests on a fake in-memory port (prompt, pagers, login, `enable`, timeout, truncation) and an end-to-end one in `broker` (login + `enable` + masked command, read-only, no port in `list_hosts`). **Not tested with real hardware**: see "Pending" | Code done; still needs testing with a cable |

### Pending

Ordered by how much they contribute versus how much they cost.

| What | Stage | Notes |
|---|---|---|
| Test the serial console with a real device | 6 | It was written without hardware: plug a cable into a switch and check (a) the port list and the raw terminal, (b) the agent's `run` with login, `enable` and `--More--`, (c) that typing in the terminal while the agent runs a command doesn't break it (they share the port; the agent's command takes the turn but the person's keystrokes also get through), (d) keystroke latency on Windows (the port handles one operation at a time: a write waits up to 50 ms for the ongoing read, `serial::READ_WAIT`). Whatever doesn't fit a device (odd prompt, different pager) is fixed in `serial::PAGERS` or with the host's prompt pattern |
| Serial console: have the terminal warn if the automatic login failed | 6 | On connect, `test_host` attempts the login and `enable` (`serial::ready`) and the console opens anyway even if there's no prompt (device off, booting); today the reason only ends up in "Show logs" |
| Serial console: commands that ask for confirmation (`reload`, `write erase`) | 6 | They hit the timeout. If needed, a list of answers to prompts (`[confirm]` → Enter) configurable per host |

### Stage 3, how to approach it

1. `apps/server`: a `main.rs` that creates a `Broker` with a `Notifier` that logs and sends push, and serves
   `mcp::router` behind `axum-server` with rustls. Nothing new in `ethereon-core`.
2. Remote unlock: a `/ui` endpoint separate from the MCP one, with its own admin token and
   only from the internal network or a tunnel (Tailscale). Never from an agent's token.
3. The desktop app gains a "connect to a broker" mode: the same views, but the commands
   go over HTTPS instead of IPC. That's why the interface commands are thin and logic-free.
4. On the server, the vault can use the master password at startup (requested by the remote
   interface) instead of DPAPI.

## Decisions

| Decision | Discarded alternative | Reason |
|---|---|---|
| Tauri 2 + Svelte 5 | Tauri + React (the spec) | It's Haze's stack: same way of working, less JavaScript, and the window is destroyed on close just like there |
| Rust with `russh` | Go with `x/crypto/ssh` | A single language with Tauri: the broker is a crate the app links, no second process |
| `russh` with `ring` | `aws-lc-rs` (the default) | `aws-lc-rs` needs NASM to build on Windows |
| AES-256-GCM + Argon2id from RustCrypto | libsodium (the spec) | Pure Rust, no C libraries on Windows. Same algorithms the spec asks for |
| Hosts, rules, fingerprints and settings inside the encrypted vault | A plaintext JSON with only the secrets encrypted | The agent runs on the same PC: if the rules were a plain file it could edit them. AES-GCM also detects any change |
| MCP over local HTTP (127.0.0.1) with a token | MCP over stdio with a bridge process | Claude Code speaks HTTP directly, there's no second binary, and stage 3 is the same transport over HTTPS |
| Hand-written MCP protocol (`mcp.rs`, ~200 lines) | Official SDK (`rmcp`) | It's four JSON-RPC methods; the Rust SDK changes its API often and would add weight |
| With the vault locked, the MCP answers "locked" as a tool error | Responding 401 | A 401 makes Claude Code attempt an OAuth login that doesn't exist |
| sudo with `sudo -k -S -p ''` and the password via stdin | `SUDO_ASKPASS`, or the password in the command | It never shows up on the command line or in variables. `-k` forces it to always ask, so it never ends up as input to the command |
| Rules as lines of text (`rm -r\|-R -f`, `path:.ssh`) | Regular expressions or a JSON per rule | They're read and edited in a textarea; the syntax fits in six lines of help |
| What can't be analyzed (program in a variable, pipe to a shell, heredocs) is rejected in free and read-only mode, and flagged in approval | Letting it through | It's the way to hide a forbidden command |
| Audit in JSONL with chained HMAC | SQLite | It can be read with any editor, exported by copying the file, and the chain detects deleted or changed lines |
| Auto-lock counts only interface use | Also counting agent requests | Otherwise, an agent could keep the vault open forever |
| Terminal: output through a Tauri `Channel` | Global events | It goes only to the window that requested it and, if the window was closed, the send fails and the session ends on its own |
| Terminal output batched every 8 ms and as raw bytes (`InvokeResponseBody::Raw`) | One base64 message per read | ConPTY delivers one line at a time and each message is an `eval` in the WebView: a 32 MB `type` was 440,000 messages and 4 minutes (release). Batched: 8,000 messages and 66 s, the same as Windows Terminal at the same size. What arrives after a while with no output (a keystroke's echo) goes out immediately |
| Windows Terminal's ConPTY (1.25) instead of Windows' | The system one | It lets Sixel through. With medium outputs (a 90 KB `git log`) it's ~30% slower than the system one (measured in November 2026) |
| Exporting with secrets asks for the master password again | Exporting with the vault open | Someone who sat down at the unlocked PC doesn't walk away with everything |
| Frameless window with a custom bar (tabs in the title bar) | Windows frame and tabs below | It's what Termius does: a single bar at the top. Copied from Haze, including moving and resizing with `SetWindowPos` (with `set_size` the window grew every time it was opened) |
| In a terminal, Ctrl W goes to the shell and the tab closes with Ctrl Shift W; locking is Ctrl Shift L | Ctrl W and Ctrl L always for the app | In bash, Ctrl W deletes the previous word and Ctrl L clears the screen: accidentally closing the session costs more than a modifier |
| Palettes generated by a Python script | Writing them by hand in CSS | 25 palettes are hundreds of variables; in the script each one is a line |
| Updates like Haze: `latest.json` on GitHub Releases, signed, in the background | Notify and have the person download the installer | It's the flow that already works in Haze. Difference: it only installs itself with the window closed, the vault locked and no agent commands, because restarting locks the vault |
| When updating on demand with the vault open, the new version starts unlocked: the old one hands the key to a copy of itself in %TEMP%, which gives it over a named pipe only to the installed `ethereon.exe` (`handoff.rs`) | Asking for the password again; or storing the key in a DPAPI-protected file | Asking for it right after being inside makes no sense. A DPAPI file can be decrypted by any program of the same user (the agent too); the key in the copy's memory, for a few seconds, doesn't open anything the open app didn't already have |
| Optional switch "Ask for the master password when Ethereon starts" (on by default). Off, the vault key is kept in `unlock.key` encrypted with DPAPI for the user (`remember.rs`), and turning it off asks for the master password | Never storing the key; or a flag inside the vault | It is the person's choice to trade security for convenience, and the interface says so: any program of the same user (the agent too) can decrypt the file and open the vault. The file itself is the setting: the agent can only delete it (then it asks again), never create it without the key. Locking by hand or by idle time still asks |
| Custom Select, Switch and Seg | Native `<select>` and checkboxes | On Windows the native `<select>` can't be styled: it broke every theme |
| Clipboard with `tauri-plugin-clipboard-manager` | `navigator.clipboard` | In the WebView, reading the clipboard asks for permissions or fails depending on the origin; the plugin always works |
| Shortcuts and terminal options in the WebView's storage | Inside the vault | They don't affect security, and this way they can be changed with the vault locked |
| Argon2 and AES optimized in `dev` too | Everything unoptimized | Unoptimized, unlocking the vault with `npm run dev` took several seconds |
| Free = total freedom and Guarded = everything except what's forbidden; old vaults with "free" are read as Guarded | Changing the meaning of "free" in place | Otherwise, on update, hosts that were protected by the forbidden list would lose it without warning |
| Dragging hosts with custom mouse events | Browser drag and drop | In Tauri the WebView hands the drag to Windows (for dropping files) and HTML5 events don't arrive |
| Inter + Geist Mono typography in every theme except Athanor | A pixel font for Poolsuite | Each theme's personality goes in colors, borders and shadows; the type stays readable |
| Serial: `Host` gains `transport` and `serial` with `serde(default)`, instead of an enum `Host { Ssh(..), Serial(..) }` | A per-host tagged enum | The enum changed the vault JSON for every host (migration); with the new fields an old vault is read as-is as SSH, and the interface keeps treating all hosts the same (name, group, rules, icon) |
| Serial: one port per host shared between the terminal and the agent (`serial::Conn` with `tap`) | Each one opening the port | A serial port is exclusive: if the terminal has it, the agent couldn't open it. Shared, the person sees live what the agent does, like a shared `screen` |
| Serial: the prompt is recognized on the last line after a `\n`, and it waits 200 ms after seeing it | Cutting at the first `#` or `>` | An output line can end in `#` (`show run` shows comments); the command echo can end in `$`. With the wait, whatever keeps arriving discards the false prompt |
| Serial: in `list_hosts` the agent sees `transport: serial` | Having the host look identical to an SSH one | The agent needs to know there's no shell (no `sh` pipes, no exit code) in order to send the device's commands; the transport isn't a secret, the port is and doesn't leak |
| Serial: `sudo` doesn't exist; Ethereon does `enable` before the command if the prompt ends in `>` | Having the agent send `enable` as a command | The agent never sees the `enable` password, same as sudo's: Ethereon types it when the device asks for it (`serial::ready`) |
| Serial: port errors to the agent without detail (`details in the connection log in Ethereon`) | Passing through the system error | The text of an open error can name the port (`COM3`), which for the agent is like the IP; the detail goes to the connection log that only the person sees |
| Agent files: `download` / `upload` over SFTP, turned on with `Policy::files` (off by default, `serde(default)`) and with its `max_file_kb` limit; inherited from the group like the rest of the rules | `features` on the `Host` (the original plan) or a dedicated per-host switch | It's one more rule about what the agent can do: as part of the policy it's inherited from the group, edited in the same place (switch in the rules editor, for host and group) and an old vault is read as off without a migration |
| Same rules as `run`: host mode, global and host `path:` rules (on the requested path, the one resolved by the server and the `~/` form), approval, rate shared with `run` and audit with size and hash. Read-only: download only; Free: no forbidden list, like in `run` | Separate rules for files | A file is another way to read or write: if `cat ~/.ssh/x` is forbidden, downloading it is too, and there's no second list to maintain |
| Download returns the masked text in the response; it doesn't write to the PC's disk, and binaries are rejected | Downloading to a local file (masked or not) | If it's written to disk, the agent reads it raw and `mask.rs` is bypassed; masking on write doesn't work for a binary, and writing opens the door to overwriting files on the PC. In the response it goes through the same masking as `run` |
| Upload: text in the request or a local file; the local one is resolved (`canonicalize`) and can't be in Ethereon's folder, in credential folders (`~/.ssh`, `~/.aws`, DPAPI, browser profiles...), have a key name (`id_rsa`, `*.pem`, `.env`) or be a network path. Nothing containing a vault secret or a private key goes out, in any mode | Reading any local file, or only the request text | The agent could use Ethereon to read what its own sandbox doesn't let it (confused deputy); a `\\server\...` path would make Windows send the session. The content check stops the case that matters even if the folder list doesn't cover it |
| Upload writes a temp file in the already resolved folder and renames it; it doesn't follow a link at the destination or overwrite without `overwrite`, and it preserves permissions | Opening the destination with `create` | Writing over the file would follow a hard or symbolic link somewhere else (`authorized_keys`); with the rename the directory entry is replaced and nothing more |
| Responses without the resolved path and generic SFTP and connection errors | Passing through the `russh-sftp` or `SshError` error | The resolved path carries the username (`/home/tester`) and connection errors the address; they're not needed for the agent to fix its request |
| Multiple addresses per host (`alt_addrs`, LAN and Tailscale): SSH is opened on all at once and the first one to present the confirmed fingerprint wins | Measuring ping (ICMP) and choosing the lowest latency | Many servers don't answer ICMP, on Windows it needs another API, and ping doesn't tell whether the SSH port works. The race picks the fastest of the working ones, a dead one doesn't cause a wait, and another machine with the same LAN IP on another network loses because its fingerprint doesn't match |
| The fingerprint is confirmed once, on the main address, and applies to all; an address can have its own if a change is confirmed from it | One fingerprint per address | It's the same machine: confirming the same fingerprint twice doesn't protect any more. The per-address one covers the case where a route presents a different key (Tailscale SSH without access to the system keys) |
| Alternative addresses share the host's jumps | Different jumps per address | With Tailscale no bastion is needed from outside; if that case comes up it can be added later without changing the format |
