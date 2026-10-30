<script lang="ts" module>
  // El logo del sistema de un host (lo detecta la terminal al conectar, ver Broker::detect_os).
  // Los logos son de simple-icons (CC0), en un solo color: toman el del texto, como los de lucide.
  import {
    siAlmalinux,
    siAlpinelinux,
    siApple,
    siArchlinux,
    siCentos,
    siDebian,
    siDeepin,
    siElementary,
    siEndeavouros,
    siFedora,
    siFreebsd,
    siGentoo,
    siKalilinux,
    siLinux,
    siLinuxmint,
    siManjaro,
    siMxlinux,
    siNixos,
    siOpenbsd,
    siOpensuse,
    siPopos,
    siProxmox,
    siRaspberrypi,
    siRedhat,
    siRockylinux,
    siSlackware,
    siSuse,
    siUbuntu,
    siVoidlinux,
    siZorin,
    type SimpleIcon,
  } from "simple-icons";

  /** Por `ID` de /etc/os-release (o `uname -s` en minusculas). */
  const BY_ID: Record<string, SimpleIcon> = {
    almalinux: siAlmalinux,
    alpine: siAlpinelinux,
    darwin: siApple,
    arch: siArchlinux,
    archarm: siArchlinux,
    centos: siCentos,
    debian: siDebian,
    deepin: siDeepin,
    elementary: siElementary,
    endeavouros: siEndeavouros,
    fedora: siFedora,
    freebsd: siFreebsd,
    gentoo: siGentoo,
    kali: siKalilinux,
    linuxmint: siLinuxmint,
    manjaro: siManjaro,
    mx: siMxlinux,
    nixos: siNixos,
    openbsd: siOpenbsd,
    opensuse: siOpensuse,
    "opensuse-leap": siOpensuse,
    "opensuse-tumbleweed": siOpensuse,
    pop: siPopos,
    proxmox: siProxmox,
    raspbian: siRaspberrypi,
    rhel: siRedhat,
    rocky: siRockylinux,
    slackware: siSlackware,
    sles: siSuse,
    suse: siSuse,
    ubuntu: siUbuntu,
    void: siVoidlinux,
    zorin: siZorin,
  };

  /** El primero que se conoce de "id id_like...". Si no se conoce ninguno, el pingüino. */
  export function osLogo(os: string | null | undefined): SimpleIcon | null {
    if (!os) return null;
    for (const w of os.split(" ")) if (BY_ID[w]) return BY_ID[w];
    return siLinux;
  }
</script>

<script lang="ts">
  import type { Component } from "svelte";

  let { os, size = 14, fallback: Fallback }: { os: string | null | undefined; size?: number; fallback: Component<{ size?: number }> } =
    $props();
  const logo = $derived(osLogo(os));
</script>

{#if logo}
  <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" role="img" aria-label={logo.title}>
    <title>{logo.title}</title>
    <path d={logo.path} />
  </svg>
{:else}
  <Fallback {size} />
{/if}
