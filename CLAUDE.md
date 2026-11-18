# CLAUDE.md

Ethereon: a Termius-style SSH manager with a vault that gives Claude Code access over MCP without ever
handing it a credential. Tauri 2 + Svelte 5, with the broker in a separate Rust crate.

## Where each decision lives

| Question | File |
|---|---|
| What the user asked for | `docs/spec.md` |
| Stages, what's missing, discarded alternatives | `docs/PLAN.md` |
| How the interface should look | `DESIGN.md` |
| What each broker module does | header comment of each file in `crates/ethereon-core/src/` |
| How to debug the interface (debug mode, test copy) | `docs/DEBUG.md` |

## Commands

```powershell
npm run dev     # the app with live reload
npm run check   # svelte-check
cargo test      # core tests
```

Before calling a change done: `cargo test` and `npm run check` with no errors or warnings. If the
change touches `ssh`, `broker`, `policy` or `mcp`, also the acceptance tests (see README).

## Versions: commit, tag and push

Every finished fix or feature (tests and check green) gets published without waiting to be asked:
bump the version, commit, tag, and push `main` and the tag.

- **Which number goes up** (semver, `MAJOR.MINOR.PATCH`):
  - `x.y.Z` (patch): fixes and small tweaks to something that already existed (a text, a width,
    a bug).
  - `x.Y.0` (minor): something new that can be used (an option, an icon, a screen), without
    breaking anything.
  - `X.0.0` (major): breaks something that already exists (the vault format without a migration, the
    MCP API, removing a feature). Ask before bumping it.
- **Where the version lives** (all five the same): `apps/desktop/package.json`,
  `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/Cargo.toml`,
  `crates/ethereon-core/Cargo.toml`, and the `ethereon` / `ethereon-core` entries in
  `Cargo.lock` and `@ethereon/desktop` in `package-lock.json` (careful with blind replacing:
  other dependencies may have the same number).
- **Commit**: `vX.Y.Z: <summary>` and in the body one line per change, in Spanish, ending
  with the version (see `git log`). Tag `vX.Y.Z` on that commit.
- Changes that are neither a fix nor a feature (docs, this file) are committed and pushed without a tag.

## Structure

```
crates/ethereon-core/src/   broker without a UI
  vault.rs                encrypted file (AES-256-GCM + Argon2id)
  model.rs                what's inside the vault
  shell.rs, policy.rs     command parser and rules
  mask.rs                 output masking
  audit.rs                log with chained HMAC
  ssh.rs                  connections, jumps, fingerprints, exec, PTY
  serial.rs               serial console (console cable): shared port, prompt-based exec,
                          pagers, login; `Link` trait to test it without hardware
  broker.rs               ties everything together and dispatches by transport (SSH or serial); the only thing the UI and the MCP use
  mcp.rs                  MCP server (HTTP, hand-written JSON-RPC)
  import.rs, export.rs    ssh config, keys, Bitwarden; OpenSSH, Bitwarden, CSV, JSON
  files.rs                SFTP and local disk for the person (no rules)
  transfer.rs             agent files over SFTP (`download` / `upload`): paths, content, local files
crates/ethereon-core/tests/ acceptance tests and the test SSH containers
apps/desktop/src-tauri/   tray, window, UI commands (commands.rs), terminal (SSH, serial and local), files.rs (SFTP),
                          hotkey.rs (global hotkey), conpty/ (Windows Terminal's ConPTY, see its README)
apps/desktop/src/         UI: views/ (one per section), components/, lib/, styles/
```

## Project rules

- **The agent never reaches a secret.** Nothing returned by `mcp.rs` or `Broker::agent_*` may
  include IPs, serial port names (`COM3`), usernames, credentials or unmasked output. Whatever
  unlocks, edits or exports exists only as a Tauri command.
- **The broker doesn't know how a command travels.** Rules, approvals, masking and auditing
  are the same for SSH and serial; only `Broker::exec` dispatches on `Host::transport`.
- **Everything that affects security goes inside the vault** (rules, fingerprints, devices, settings),
  never in a plain file: the agent runs on the same PC and could edit it.
- **Tauri commands are thin.** The logic goes in `ethereon-core`, so stage 3 can replace them
  with HTTP without rewriting anything.
- **Themes only through variables.** Colors live in `src/styles/themes.css`; components have no
  hardcoded colors. A new theme = one block there + one line in `src/lib/themes.ts`.
- Docs in English; code comments in Spanish; interface text in English, literal (see `DESIGN.md`).
- **Rust disk space.** In November 2026 `target/` reached ~110 GB (plus ~12 GB in the
  worktrees) and filled C:. So it doesn't happen again:
  - `--release` only to publish or to test the installer.
  - In a worktree (`.claude/worktrees/`) build with `CARGO_TARGET_DIR` pointing to the `target/`
    of the main repo (`$env:CARGO_TARGET_DIR="<repo>\target"`), don't create a new one. When the
    task is done, `git worktree remove` the worktree.
  - When done testing a task that compiled (before the push that publishes it), `cargo clean`
    in `target/`: it goes down to 0. Rebuilding takes a few minutes, but the disk doesn't fill up.
    First, close the test instance running from there (Windows won't let you delete its .exe).
