"""Genera src/lib/termschemes.ts desde scripts/termschemes.json.

Los esquemas de colores de la terminal (Settings → Terminal → Colors), como los de Termius.
Cada uno trae fondo, texto, cursor, seleccion y los 16 colores ANSI, copiados de su fuente
(el campo "source" de cada uno: casi todos de mbadolato/iTerm2-Color-Schemes o del repo del
tema). Para sumar uno: agregarlo al .json y correr: python scripts/termschemes.py
"""
import json
import pathlib
import re
import unicodedata

here = pathlib.Path(__file__).resolve().parent
schemes = json.loads((here / "termschemes.json").read_text(encoding="utf-8"))


def slug(name: str) -> str:
    plain = unicodedata.normalize("NFKD", name).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "-", plain.lower()).strip("-")


hexre = re.compile(r"^#[0-9a-f]{6}$")
# La seleccion puede ser translucida (#rrggbbaa), como en Termius: asi se sigue viendo el texto.
selre = re.compile(r"^#[0-9a-f]{6}([0-9a-f]{2})?$")
out, seen = [], set()
for s in schemes:
    sid = slug(s["name"])
    assert sid not in seen, f"repetido: {sid}"
    seen.add(sid)
    colors = [s["background"], s["foreground"], s["cursor"], *s["ansi"]]
    if "selectionForeground" in s:
        colors.append(s["selectionForeground"])
    assert len(s["ansi"]) == 16 and all(hexre.match(c) for c in colors), f"colores mal en {s['name']}"
    assert selre.match(s["selection"]), f"seleccion mal en {s['name']}"
    out.append({
        "id": sid,
        "name": s["name"],
        "light": bool(s["light"]),
        "bg": s["background"],
        "fg": s["foreground"],
        "cursor": s["cursor"],
        "selection": s["selection"],
        **({"selectionFg": s["selectionForeground"]} if "selectionForeground" in s else {}),
        "ansi": s["ansi"],
    })

ts = (
    "// Generado por scripts/termschemes.py: editar termschemes.json, no aca.\n"
    "export interface Scheme {\n"
    "  id: string;\n"
    "  name: string;\n"
    "  light: boolean;\n"
    "  bg: string;\n"
    "  fg: string;\n"
    "  cursor: string;\n"
    "  selection: string;\n"
    "  /** El texto seleccionado, si el esquema lo cambia (cuando la seleccion es clara). */\n"
    "  selectionFg?: string;\n"
    "  /** Negro rojo verde amarillo azul magenta cian blanco, normales y despues brillantes. */\n"
    "  ansi: string[];\n"
    "}\n\n"
    "export const SCHEMES: Scheme[] = "
    + json.dumps(out, indent=2, ensure_ascii=False)
    + ";\n"
)
(here.parent / "src" / "lib" / "termschemes.ts").write_text(ts, encoding="utf-8")
print("ok", len(out), "esquemas")
