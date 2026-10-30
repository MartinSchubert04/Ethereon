// Los iconos que se pueden elegir para un host o para una shell local, y el que le toca a
// cada host segun el sistema detectado (Broker::detect_os). Los logos son de simple-icons
// (CC0), en un solo color: toman el del texto, como los de lucide. Microsoft saco los suyos
// de simple-icons, asi que para PowerShell y cmd quedan los genericos.
import type { Component } from "svelte";
import { Box, Cloud, Code, Database, Laptop, Monitor, Server, SquareTerminal, Terminal } from "@lucide/svelte";
import {
  siAlmalinux,
  siAlpinelinux,
  siApple,
  siArchlinux,
  siCentos,
  siClaude,
  siDebian,
  siDeepin,
  siDocker,
  siElementary,
  siEndeavouros,
  siFedora,
  siFreebsd,
  siGentoo,
  siGit,
  siGnubash,
  siKalilinux,
  siLinux,
  siLinuxmint,
  siManjaro,
  siMxlinux,
  siNeovim,
  siNixos,
  siNodedotjs,
  siNushell,
  siOpenbsd,
  siOpensuse,
  siPopos,
  siProxmox,
  siPython,
  siRaspberrypi,
  siRedhat,
  siRockylinux,
  siSlackware,
  siSuse,
  siTmux,
  siUbuntu,
  siVoidlinux,
  siZorin,
  siZsh,
  type SimpleIcon,
} from "simple-icons";

export type IconDef = { label: string; si?: SimpleIcon; lucide?: Component<{ size?: number }> };

const L = (label: string, lucide: Component<{ size?: number }>): IconDef => ({ label, lucide });
const S = (si: SimpleIcon, label = si.title): IconDef => ({ label, si });

/** En el orden en que se ofrecen: genericos, sistemas, herramientas. */
export const ICONS: Record<string, IconDef> = {
  terminal: L("Terminal", SquareTerminal),
  prompt: L("Prompt", Terminal),
  server: L("Server", Server),
  laptop: L("Laptop", Laptop),
  monitor: L("Monitor", Monitor),
  code: L("Code", Code),
  cloud: L("Cloud", Cloud),
  database: L("Database", Database),
  box: L("Box", Box),
  linux: S(siLinux),
  debian: S(siDebian),
  ubuntu: S(siUbuntu),
  raspberrypi: S(siRaspberrypi),
  arch: S(siArchlinux),
  redhat: S(siRedhat),
  fedora: S(siFedora),
  centos: S(siCentos),
  rocky: S(siRockylinux),
  alma: S(siAlmalinux),
  alpine: S(siAlpinelinux),
  mint: S(siLinuxmint),
  opensuse: S(siOpensuse),
  suse: S(siSuse),
  kali: S(siKalilinux),
  nixos: S(siNixos),
  gentoo: S(siGentoo),
  manjaro: S(siManjaro),
  endeavouros: S(siEndeavouros),
  pop: S(siPopos),
  elementary: S(siElementary),
  zorin: S(siZorin),
  deepin: S(siDeepin),
  mx: S(siMxlinux),
  void: S(siVoidlinux),
  slackware: S(siSlackware),
  proxmox: S(siProxmox),
  freebsd: S(siFreebsd),
  openbsd: S(siOpenbsd),
  macos: S(siApple, "macOS"),
  bash: S(siGnubash),
  zsh: S(siZsh),
  nushell: S(siNushell),
  git: S(siGit),
  claude: S(siClaude),
  docker: S(siDocker),
  python: S(siPython),
  node: S(siNodedotjs),
  neovim: S(siNeovim),
  tmux: S(siTmux),
};

/** `ID` de /etc/os-release (o `uname -s` en minusculas) -> icono. */
const BY_OS: Record<string, string> = {
  almalinux: "alma",
  alpine: "alpine",
  darwin: "macos",
  arch: "arch",
  archarm: "arch",
  centos: "centos",
  debian: "debian",
  deepin: "deepin",
  elementary: "elementary",
  endeavouros: "endeavouros",
  fedora: "fedora",
  freebsd: "freebsd",
  gentoo: "gentoo",
  kali: "kali",
  linuxmint: "mint",
  manjaro: "manjaro",
  mx: "mx",
  nixos: "nixos",
  openbsd: "openbsd",
  opensuse: "opensuse",
  "opensuse-leap": "opensuse",
  "opensuse-tumbleweed": "opensuse",
  pop: "pop",
  proxmox: "proxmox",
  raspbian: "raspberrypi",
  rhel: "redhat",
  rocky: "rocky",
  slackware: "slackware",
  sles: "suse",
  suse: "suse",
  ubuntu: "ubuntu",
  void: "void",
  zorin: "zorin",
};

/** El icono del sistema detectado ("pop ubuntu debian" -> el primero que se conoce). Si no
 *  se conoce ninguno, el pingüino; si no se detecto nada, null. */
export function osIcon(os: string | null | undefined): string | null {
  if (!os) return null;
  for (const w of os.split(" ")) if (BY_OS[w]) return BY_OS[w];
  return "linux";
}

/** El icono de un host: el elegido a mano, el de su sistema, o el de servidor. */
export const hostIcon = (h: { icon?: string | null; os?: string | null } | undefined) =>
  (h?.icon && ICONS[h.icon] ? h.icon : null) ?? osIcon(h?.os) ?? "server";

/** Para un Select: todos los iconos. */
export const iconOptions = () => Object.entries(ICONS).map(([value, d]) => ({ value, label: d.label }));
