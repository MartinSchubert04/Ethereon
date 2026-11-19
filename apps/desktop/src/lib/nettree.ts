// Arma el arbol de red a partir de traceroutes: la raiz es esta PC, cada salto un nodo, y
// los caminos que pasan por el mismo router se juntan en la misma rama. Despues lo ubica
// en columnas (una por salto) para dibujarlo en SVG (components/NetTree.svelte).
import type { Trace, TraceHop } from "./api";

export type NodeKind = "root" | "router" | "host" | "lost" | "jump" | "unreached";

export interface NetNode {
  key: string;
  kind: NodeKind;
  /** Lo que se lee primero: el host del vault, el nombre DNS o la IP. */
  label: string;
  /** La segunda linea: la IP (si el label no lo es) o el estado. */
  sub: string;
  /** Promedio de las pruebas de este salto, en ms. */
  rtt: number | null;
  /** Las pruebas de este salto (para el tooltip). */
  rtts: (number | null)[];
  ttl: number;
  children: NetNode[];
  /** Hosts del vault que pasan por aca. */
  hosts: Set<string>;
  /** Hosts del vault que terminan aca (comparten la IP). */
  dest: string[];
  // Posicion, en px (la pone `layout`).
  x: number;
  y: number;
}

export interface Edge {
  from: NetNode;
  to: NetNode;
  /** SSH a traves de un bastion o sin respuesta: se dibuja punteado. */
  dashed: boolean;
}

function node(key: string, kind: NodeKind, label: string, sub: string, ttl: number, rtts: (number | null)[] = []): NetNode {
  const ok = rtts.filter((x): x is number => x !== null);
  const rtt = ok.length ? Math.round((ok.reduce((a, b) => a + b, 0) / ok.length) * 10) / 10 : null;
  return { key, kind, label, sub, rtt, rtts, ttl, children: [], hosts: new Set(), dest: [], x: 0, y: 0 };
}

function hopNode(h: TraceHop, parentKey: string): NetNode {
  if (!h.ip) return node(`${parentKey}/*${h.ttl}`, "lost", "* * *", "no reply", h.ttl, h.rtts);
  const label = h.host ?? h.dns ?? h.ip;
  return node(h.ip, h.host ? "host" : "router", label, label === h.ip ? (h.dns ?? "") : h.ip, h.ttl, h.rtts);
}

/** El hijo de `parent` con la misma clave, o lo agrega. */
function child(parent: NetNode, n: NetNode): NetNode {
  const found = parent.children.find((c) => c.key === n.key);
  if (found) return found;
  parent.children.push(n);
  return n;
}

/** Un arbol con todos los caminos. */
export function buildTree(traces: Trace[]): NetNode {
  const root = node("root", "root", "This PC", "", 0);
  for (const t of traces) {
    let at = root;
    root.hosts.add(t.host);
    for (const h of t.hops) {
      at = child(at, hopNode(h, at.key));
      at.hosts.add(t.host);
    }
    // El primer salto que se ve por ICMP es el host o su primer bastion. Despues, los
    // bastiones y el host van por SSH (punteado).
    const chain = t.via.length ? [...t.via, t.host] : [t.host];
    if (!t.reached || t.error) {
      const sub = t.error ?? "did not reply to ping";
      const n = node(`${at.key}/x:${chain[0]}`, "unreached", chain[0], sub, at.ttl + 1);
      at = child(at, n);
      at.hosts.add(t.host);
    } else if (at.kind !== "root") {
      // El destino: se llama como el/los host(s) del vault, con la IP abajo.
      at.kind = "host";
      if (!at.dest.includes(chain[0])) at.dest.push(chain[0]);
      at.label = at.dest.join(", ");
      at.sub = at.key;
    }
    for (const name of chain.slice(1)) {
      at = child(at, node(`${at.key}/ssh:${name}`, "jump", name, "via SSH", at.ttl + 1));
      at.hosts.add(t.host);
    }
  }
  return root;
}

export const COL = 210;
export const ROW = 58;
export const NODE_W = 168;
export const NODE_H = 40;

/** Columnas por profundidad, filas por hoja; cada padre queda a la altura media de sus hijos. */
export function layout(root: NetNode): { nodes: NetNode[]; edges: Edge[]; width: number; height: number } {
  const nodes: NetNode[] = [];
  const edges: Edge[] = [];
  let row = 0;
  let depthMax = 0;
  function walk(n: NetNode, depth: number) {
    nodes.push(n);
    depthMax = Math.max(depthMax, depth);
    n.x = depth * COL;
    if (!n.children.length) {
      n.y = row++ * ROW;
      return;
    }
    for (const c of n.children) {
      walk(c, depth + 1);
      edges.push({ from: n, to: c, dashed: c.kind === "jump" || c.kind === "unreached" || c.kind === "lost" });
    }
    n.y = (n.children[0].y + n.children[n.children.length - 1].y) / 2;
  }
  walk(root, 0);
  return { nodes, edges, width: depthMax * COL + NODE_W, height: Math.max(1, row) * ROW - (ROW - NODE_H) };
}

/** El estado de una latencia, para el color. */
export function rttLevel(ms: number | null): "ok" | "warn" | "bad" | "none" {
  if (ms === null) return "none";
  return ms < 30 ? "ok" : ms < 120 ? "warn" : "bad";
}
