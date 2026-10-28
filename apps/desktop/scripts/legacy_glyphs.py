"""Genera src/lib/legacy-glyphs.gen.json: sextantes (2x3) y octantes (2x4) de "Symbols for
Legacy Computing" como rectangulos en la grilla de 8x8 que usa xterm.js para dibujar bloques.

xterm.js dibuja por su cuenta los bloques y cuadrantes (por eso se ven sin costuras), pero no
estos: sin esto salen de otra fuente, con otras proporciones, y las imagenes que arman
herramientas como chafa (y los relojes grandes de los MOTD) se ven deformadas.

Correr con la tabla oficial de Unicode 16:
  curl -LO https://www.unicode.org/Public/16.0.0/ucd/UnicodeData.txt
  python scripts/legacy_glyphs.py UnicodeData.txt
"""
import json
import pathlib
import re
import sys

COLS = 2


def rects(cells: set[int], rows: int) -> list[dict]:
    """Celdas numeradas 1..(2*rows) de izquierda a derecha y de arriba a abajo. Junta las
    filas iguales que estan pegadas, para que no queden rayas entre rectangulos."""
    h = 8 / rows
    masks = [tuple(((r * COLS + c + 1) in cells) for c in range(COLS)) for r in range(rows)]
    out = []
    r = 0
    while r < rows:
        m = masks[r]
        end = r
        while end + 1 < rows and masks[end + 1] == m:
            end += 1
        y, height = r * h, (end - r + 1) * h
        if all(m):
            out.append({"x": 0, "y": y, "w": 8, "h": height})
        else:
            for c in range(COLS):
                if m[c]:
                    out.append({"x": c * 4, "y": y, "w": 4, "h": height})
        r = end + 1
    return out


def main(path: str):
    defs = {}
    for line in pathlib.Path(path).read_text(encoding="utf-8").splitlines():
        code, name = line.split(";")[:2]
        m = re.fullmatch(r"BLOCK (SEXTANT|OCTANT)-(\d+)", name)
        if not m:
            continue  # los SEPARATED (con huecos) quedan para la fuente
        rows = 3 if m.group(1) == "SEXTANT" else 4
        cells = {int(d) for d in m.group(2)}
        defs[chr(int(code, 16))] = rects(cells, rows)
    out = pathlib.Path(__file__).resolve().parent.parent / "src/lib/legacy-glyphs.gen.json"
    out.write_text(json.dumps(defs, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    sext = sum(1 for k in defs if 0x1FB00 <= ord(k) <= 0x1FB3B)
    print(f"ok {len(defs)} glyphs ({sext} sextants, {len(defs) - sext} octants)")


if __name__ == "__main__":
    main(sys.argv[1])
