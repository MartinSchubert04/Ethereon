# Original spec

As requested. "I" is the agent (Claude). What changed while building it is in
[PLAN.md](PLAN.md), "Decisions" section.

## Core idea

The app is the only one that has the credentials and the one that opens the SSH connections. The agent never
receives a password or a key: it asks the app "run this command on host X" and receives only
the output. The person manages everything from a graphical interface similar to Termius, and also uses it
as an SSH client.

## 1. Components

- Background service (broker): stores the vault, opens the SSH connections, applies the rules for what can
  be run and logs everything. It can run on the PC or on a server, to use it from multiple PCs.
- Web or desktop interface (Termius-style): hosts, groups, credentials, rules, approvals and
  audit. Interactive terminal.
- Connection for agents (MCP): local on the PC, or over HTTPS with a per-PC token if the broker is on
  a server.

## 2. Vault

- Secret types: passwords, private keys (with or without passphrase), SSH certificates,
  sudo password, API tokens.
- Encryption of the whole vault (AES-256-GCM) with a key derived from a master password
  (Argon2id), or protected by the operating system (DPAPI on Windows, Keychain on Mac).
- Only a person can unlock it, from the interface. It locks itself after a period of no
  use. The agent has no way to unlock it.
- Hosts reference credentials by ID (`cred: bastion-key`). The agent only knows that ID.
- Import from `~/.ssh/config`, from Termius and from key files.
- Easy export: Bitwarden, Termius and other common formats (added later).

## 3. Hosts

- Name, IP or hostname, port, user, credential, group, tags and environment (prod, test, dev).
- Bastion jumps (chained ProxyJump).
- Host key verification: the first time it shows the fingerprint and a person has to
  confirm it. If it changes later, the connection is blocked and a warning is shown.
- A host description the agent can read for context.
- (Added later) A host can also be a device on a serial console cable (switch,
  AP, router): port, baud rate, 8N1, prompt pattern and, if needed, username, password and
  `enable` password from the vault. The agent uses it with the same `run(host, command)` and the same
  rules; it never sees the port name.

## 4. What the agent can do

- List hosts and groups: name, environment, tags and description; never IPs, credentials or secrets.
- `run(host, command)`: output, exit code and duration.
- Optional per host: persistent sessions, reading files, uploading and downloading files, tunnels.
- sudo without exposing the password: the app injects it through standard input. Never on the command
  line, in environment variables, or in the output.

## 5. Rules per host or per group

- Modes: read-only (allowlist), with approval, free.
- Always-on forbidden list: `rm -rf`, `shutdown`, `mkfs`, viewing the shell history, reading
  `~/.ssh` and `/etc/shadow`.
- Commands are analyzed in full, including pipes, `;`, `$( )` and redirections.
- Limits on time, output size and commands per minute.

## 6. Approvals

- Queue with the command, the host and the reason the agent gives.
- Approve, reject, approve this pattern for the session, or edit the command before approving it.
- Notification on the desktop or on the phone.

## 7. Output protection

- All vault secrets are searched for in the output and replaced with `***`.
- Also known patterns: private keys, tokens, JWT, `password=`, Authorization headers and
  connection strings with a password.

## 8. Audit

- Each command: who, from which PC, host, command, output (or its hash), result and who approved it.
- Optional session recording; search and export from the interface.

## 9. Multiple PCs

- Server mode: PCs connect with a device token with expiry, allowed hosts and
  immediate revocation.
- Master kill switch that cuts all agent access.

## 10. Out of scope

The agent unlocking the vault, exporting or reading secrets, or changing the rules or the credentials.

## 11. Acceptance tests

1. The agent asks for a `cat` of a file that contains a vault password and receives `***`.
2. sudo works without the password appearing in the output, the audit or the process list.
3. A forbidden command hidden in a pipe or in a `$( )` is rejected.
4. With the vault locked, every request fails.
5. If a host's key changes, the connection is blocked.

## Requested stages

1. Vault with hosts, the agent connection with run, masking and audit.
2. Rules and approvals.
3. Server mode for multiple PCs.
4. Full interface with interactive terminal.

Style: modern, cheap to modify, lightweight and running in the background without fuss. Extra themes
inspired by Warp, Poolsuite FM and Haze.
