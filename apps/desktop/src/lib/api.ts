// Tipos y llamadas al backend. Los nombres siguen a los structs de Rust (snake_case).
import { invoke, Channel } from "@tauri-apps/api/core";

export type Env = "none" | "prod" | "test" | "dev";
export type Mode = "read_only" | "approval" | "guarded" | "unrestricted" | "off";
export type CredKind = "password" | "private_key" | "certificate" | "sudo_password" | "api_token";

export interface Policy {
  mode: Mode;
  allow: string[];
  deny: string[];
  timeout_secs: number;
  max_output_kb: number;
  max_per_minute: number;
  allow_sudo: boolean;
}

export interface Host {
  name: string;
  hostname: string;
  port: number;
  user: string;
  credential: string;
  sudo_credential: string | null;
  group: string | null;
  tags: string[];
  env: Env;
  jump: string[];
  description: string;
  policy: Policy | null;
}

export interface Group {
  name: string;
  policy: Policy;
}

export interface CredView {
  id: string;
  kind: CredKind;
  username: string;
  note: string;
  has_secret: boolean;
  has_passphrase: boolean;
  has_certificate: boolean;
  used_by: string[];
}

export interface KnownHost {
  addr: string;
  fingerprint: string;
  algorithm: string;
}

export interface Device {
  id: string;
  name: string;
  created: string;
  expires: string | null;
  hosts: string[];
  revoked: boolean;
}

export interface Settings {
  agent_enabled: boolean;
  autolock_minutes: number;
  approval_timeout_secs: number;
  audit_output: boolean;
  keep_alive_minutes: number;
}

export interface Snapshot {
  hosts: Host[];
  groups: Group[];
  credentials: CredView[];
  known_hosts: KnownHost[];
  devices: Device[];
  denylist: string[];
  settings: Settings;
  default_allow: string[];
  default_deny: string[];
}

export interface Status {
  exists: boolean;
  unlocked: boolean;
  agent_enabled: boolean;
  mcp_url: string;
  mcp_error: string | null;
  data_dir: string;
}

export interface Approval {
  id: number;
  host: string;
  env: Env;
  command: string;
  sudo: boolean;
  reason: string;
  device: string;
  warnings: string[];
  created: string;
}

export type SshError =
  | { kind: "host_key_unknown"; host: string; addr: string; fingerprint: string; algorithm: string }
  | { kind: "host_key_changed"; host: string; addr: string; expected: string; got: string }
  | { kind: "auth"; host: string; user: string; detail: string }
  | { kind: "connect"; host: string; detail: string };

export interface Alert {
  id: number;
  ts: string;
  error: SshError;
}

export interface AuditEntry {
  ts: string;
  device: string;
  client: string;
  host: string;
  command: string;
  sudo: boolean;
  reason: string;
  decision: string;
  detail: string;
  approved_by: string;
  exit_code: number | null;
  duration_ms: number | null;
  output_sha256: string;
  output: string | null;
  masked: number;
}

export type Decision =
  | { action: "approve" }
  | { action: "reject"; note?: string }
  | { action: "approve_pattern"; pattern: string }
  | { action: "approve_edited"; command: string };

export interface ImportSummary {
  hosts: number;
  credentials: number;
  skipped: number;
  notes: string[];
}

export type ExportFormat = "bitwarden" | "open_ssh" | "csv" | "json";

export interface CredInput {
  id: string;
  original: string | null;
  kind: CredKind;
  username: string;
  secret: string | null;
  passphrase: string | null;
  certificate: string | null;
  note: string;
}

export interface UpdateInfo {
  version: string;
  status: "downloading" | "ready";
}

export interface LocalShell {
  id: string;
  label: string;
}

export interface FileEntry {
  name: string;
  dir: boolean;
  link: boolean;
  size: number;
  mtime: number | null;
  mode: number | null;
}

export type XferEvent =
  | { kind: "progress"; done: number; total: number; file: string }
  | { kind: "done" }
  | { kind: "error"; message: string };

export type TermEvent = { kind: "data"; b64: string } | { kind: "exit"; code: number | null };

export const api = {
  status: () => invoke<Status>("status"),
  create: (password: string) => invoke<void>("vault_create", { password }),
  unlock: (password: string) => invoke<void>("vault_unlock", { password }),
  lock: () => invoke<void>("vault_lock"),
  changePassword: (old: string, nw: string) => invoke<void>("vault_change_password", { old, new: nw }),
  touch: () => invoke<void>("touch"),
  snapshot: () => invoke<Snapshot>("snapshot"),
  hostSave: (host: Host, original: string | null) => invoke<void>("host_save", { host, original }),
  hostDelete: (name: string) => invoke<void>("host_delete", { name }),
  hostMove: (name: string, group: string | null, before: string | null) => invoke<void>("host_move", { name, group, before }),
  hostTest: (name: string) => invoke<SshError | null>("host_test", { name }),
  hostTrust: (addr: string, fingerprint: string, algorithm: string) =>
    invoke<void>("host_trust", { addr, fingerprint, algorithm }),
  credSave: (cred: CredInput) => invoke<void>("cred_save", { cred }),
  credDelete: (id: string) => invoke<void>("cred_delete", { id }),
  groupSave: (group: Group, original: string | null) => invoke<void>("group_save", { group, original }),
  groupDelete: (name: string) => invoke<void>("group_delete", { name }),
  denylistSave: (rules: string[]) => invoke<void>("denylist_save", { rules }),
  settingsSave: (settings: Settings) => invoke<void>("settings_save", { settings }),
  agentSet: (on: boolean) => invoke<void>("agent_set", { on }),
  deviceCreate: (name: string, expiresDays: number | null, hosts: string[]) =>
    invoke<{ id: string; token: string; command: string }>("device_create", { name, expiresDays, hosts }),
  deviceRevoke: (id: string) => invoke<void>("device_revoke", { id }),
  deviceDelete: (id: string) => invoke<void>("device_delete", { id }),
  approvals: () => invoke<Approval[]>("approvals_list"),
  resolve: (id: number, decision: Decision) => invoke<void>("approval_resolve", { id, decision }),
  alerts: () => invoke<Alert[]>("alerts_list"),
  alertDismiss: (id: number) => invoke<void>("alert_dismiss", { id }),
  policyTest: (host: string, command: string) =>
    invoke<{ verdict: "allow" | "ask" | "deny"; detail: string }>("policy_test", { host, command }),
  auditSearch: (query: string, limit?: number) => invoke<AuditEntry[]>("audit_search", { query, limit }),
  auditVerify: () => invoke<string>("audit_verify"),
  auditExport: (path: string) => invoke<number>("audit_export", { path }),
  importSshConfig: (path: string | null) => invoke<ImportSummary>("import_ssh_config", { path }),
  importKeyFile: (path: string) => invoke<ImportSummary>("import_key_file", { path }),
  importBitwarden: (path: string) => invoke<ImportSummary>("import_bitwarden", { path }),
  exportData: (format: ExportFormat, path: string, password: string | null) =>
    invoke<string>("export_data", { format, path, password }),
  backup: (path: string) => invoke<void>("backup_vault", { path }),
  version: () => invoke<string>("app_version"),
  updateStatus: () => invoke<UpdateInfo | null>("update_status"),
  updateCheck: () => invoke<string | null>("update_check"),
  updateNow: () => invoke<void>("update_now"),
  termOpen: (host: string, cols: number, rows: number, onEvent: Channel<TermEvent>) =>
    invoke<number>("term_open", { host, cols, rows, onEvent }),
  termWrite: (id: number, data: string) => invoke<void>("term_write", { id, data }),
  termResize: (id: number, cols: number, rows: number) => invoke<void>("term_resize", { id, cols, rows }),
  termClose: (id: number) => invoke<void>("term_close", { id }),
  /** Suelta la sesion para que otra ventana la tome. */
  termDetach: (id: number) => invoke<void>("term_detach", { id }),
  /** Toma una sesion que solto otra ventana (recibe primero lo ultimo que se vio). */
  termAttach: (id: number, onEvent: Channel<TermEvent>) => invoke<void>("term_attach", { id, onEvent }),
  sftpOpen: (host: string) => invoke<{ id: number; home: string }>("sftp_open", { host }),
  sftpClose: (id: number) => invoke<void>("sftp_close", { id }),
  sftpList: (id: number, path: string) => invoke<FileEntry[]>("sftp_list", { id, path }),
  sftpRealpath: (id: number, path: string) => invoke<string>("sftp_realpath", { id, path }),
  sftpMkdir: (id: number, path: string) => invoke<void>("sftp_mkdir", { id, path }),
  sftpRename: (id: number, from: string, to: string) => invoke<void>("sftp_rename", { id, from, to }),
  sftpRemove: (id: number, paths: string[]) => invoke<void>("sftp_remove", { id, paths }),
  /** Cada lado es una sesion SFTP (su id) o esta PC (null). */
  filesTransfer: (from: number | null, to: number | null, paths: string[], into: string, onEvent: Channel<XferEvent>) =>
    invoke<number>("files_transfer", { from, to, paths, into, onEvent }),
  sftpCancel: (transfer: number) => invoke<void>("sftp_cancel", { transfer }),
  localHome: () => invoke<string>("local_home"),
  localList: (path: string) => invoke<FileEntry[]>("local_list", { path }),
  localMkdir: (path: string) => invoke<void>("local_mkdir", { path }),
  localRename: (from: string, to: string) => invoke<void>("local_rename", { from, to }),
  localRemove: (paths: string[]) => invoke<void>("local_remove", { paths }),
  importDetect: (path: string) =>
    invoke<{ kind: string; label: string; needs_password: boolean }>("import_detect", { path }),
  importAuto: (path: string, password: string | null) => invoke<ImportSummary>("import_auto", { path, password }),
  localShells: () => invoke<LocalShell[]>("local_shells"),
  termOpenLocal: (shell: string, cols: number, rows: number, cwd: string | null, onEvent: Channel<TermEvent>) =>
    invoke<number>("term_open_local", { shell, cols, rows, cwd, onEvent }),
  takeOpenHere: () => invoke<{ cwd: string; reuse: boolean } | null>("take_open_here"),
  hotkeyGet: () => invoke<string | null>("hotkey_get"),
  hotkeySet: (combo: string | null) => invoke<void>("hotkey_set", { combo }),
  openInGet: () => invoke<{ mode: "wt" | "tab"; wt_installed: boolean }>("open_in_get"),
  openInSet: (mode: "wt" | "tab") => invoke<void>("open_in_set", { mode }),
};

export function defaultPolicy(): Policy {
  return { mode: "approval", allow: [], deny: [], timeout_secs: 60, max_output_kb: 256, max_per_minute: 30, allow_sudo: true };
}

export function newHost(): Host {
  return {
    name: "",
    hostname: "",
    port: 22,
    user: "",
    credential: "",
    sudo_credential: null,
    group: null,
    tags: [],
    env: "none",
    jump: [],
    description: "",
    policy: null,
  };
}

export const MODE_LABEL: Record<Mode, string> = {
  read_only: "Read-only",
  approval: "Approval",
  guarded: "Guarded",
  unrestricted: "Free",
  off: "No agent",
};

export const KIND_LABEL: Record<CredKind, string> = {
  password: "Password",
  private_key: "Private key",
  certificate: "Key + certificate",
  sudo_password: "Sudo password",
  api_token: "API token",
};

export function errText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

/** El usuario con el que se entra: el del host, o si esta vacio el de su credencial. */
export function userOf(h: Host, creds: CredView[] | undefined): string {
  return h.user || creds?.find((c) => c.id === h.credential)?.username || "";
}
