"""Genera src/styles/palettes.css y src/lib/palettes.gen.ts.

Las paletas se editan aca: cada una pisa algunas variables de su tema (ver themes.css).
La primera de cada tema es la base y no pisa nada. Correr con: python scripts/palettes.py
"""
import json
import pathlib


def haze_bg(a, b, c):
    return (
        f"radial-gradient(ellipse 45% 55% at 32% 48%, {a}, transparent 70%), "
        f"radial-gradient(ellipse 50% 40% at 78% 18%, {b}, transparent 70%), "
        f"radial-gradient(ellipse 60% 45% at 65% 92%, {c}, transparent 70%), "
        "linear-gradient(160deg, #1b2126 0%, #12171b 55%, #0d1114 100%)"
    )


def warp(g1, g2, accent, ink, glow1, glow2, sel):
    return {
        "--accent": accent,
        "--accent-ink": ink,
        "--primary-bg": f"linear-gradient(90deg, {g1}, {g2})",
        "--bg-image": f"radial-gradient(55% 45% at 85% -5%, {glow1}, transparent 70%), "
        f"radial-gradient(45% 40% at 0% 100%, {glow2}, transparent 70%)",
        "--term-cursor": accent,
        "--term-selection": sel,
        "--env-dev": g2,
    }


def dark(bg, sidebar, panel, panel2, line, strong, text, t2, t3, accent, ink, danger, warn, ok, test, dev, sel, cursor=None):
    return {
        "--bg": bg,
        "--bg-image": "none",
        "--sidebar": sidebar,
        "--panel": panel,
        "--panel-2": panel2,
        "--line": line,
        "--line-strong": strong,
        "--text": text,
        "--text-2": t2,
        "--text-3": t3,
        "--accent": accent,
        "--accent-ink": ink,
        "--primary-bg": accent,
        "--danger": danger,
        "--warn": warn,
        "--ok": ok,
        "--env-prod": danger,
        "--env-test": test,
        "--env-dev": dev,
        "--term-bg": bg,
        "--term-fg": text,
        "--term-cursor": cursor or accent,
        "--term-selection": sel,
    }


def pool(grad, accent, sidebar, panel, panel2, ink="#ffffff"):
    return {
        "--bg-image": grad,
        "--accent": accent,
        "--accent-ink": ink,
        "--primary-bg": accent,
        "--sidebar": sidebar,
        "--panel": panel,
        "--panel-2": panel2,
        "--term-bg": panel,
        "--term-cursor": accent,
        "--hover": f"color-mix(in srgb, {accent} 10%, transparent)",
        "--term-selection": f"color-mix(in srgb, {accent} 25%, transparent)",
    }


def hz(accent, a, b, c):
    return {
        "--accent": accent,
        "--primary-bg": accent,
        "--term-cursor": accent,
        "--warn": accent,
        "--term-selection": f"color-mix(in srgb, {accent} 25%, transparent)",
        "--bg-image": haze_bg(a, b, c),
    }


def ath(ink, deep, clay, clayhi, rule, umber, dim, tan, cream, amber, ahi, alo, blood, light=False):
    """Una paleta de athanor (src/index.css del portfolio-athanor) en los tokens de Ethereon."""
    v = {
        "--bg": ink, "--sidebar": ink, "--panel": clay, "--panel-2": clayhi, "--hover": clay,
        "--line": rule, "--line-strong": umber, "--text": cream, "--text-2": tan, "--text-3": dim,
        "--accent": amber, "--accent-ink": ink, "--primary-bg": amber, "--danger": blood, "--warn": amber,
        "--ok": ahi, "--env-prod": blood, "--env-test": amber, "--env-dev": tan,
        "--term-bg": ink, "--term-fg": cream, "--term-cursor": amber,
        "--term-selection": f"color-mix(in srgb, {amber} 30%, transparent)",
        "--ink-deep": deep, "--clay-hi": clayhi, "--umber": umber,
        "--accent-hi": ahi, "--accent-lo": alo, "--pop-shadow": f"8px 8px 0 {umber}",
    }
    if light:
        v.update({"color-scheme": "light", "--clay-glow": "rgb(255 255 255 / 0.5)",
                  "--clay-shade": f"color-mix(in srgb, {cream} 12%, transparent)"})
    return v


# La misma paleta Srcery "a la athanor" para los otros temas, con sus formas.
SRCERY_DEFAULT = dark("#1c1b19", "#161513", "#22211e", "#2a2824", "#3c3a36", "#5c4c39", "#fce8c3", "#baa67f", "#918175",
                      "#fbb829", "#1c1b19", "#f75341", "#fbb829", "#fed06e", "#fbb829", "#baa67f", "rgba(251,184,41,.3)")

PAPER = {
    **dark("#f6f7f9", "#eef0f3", "#ffffff", "#f1f3f6", "#dfe3e8", "#c9cfd8", "#1b1f24", "#59636e", "#8a939e",
           "#0f9d6f", "#ffffff", "#d1343e", "#b7791f", "#0f9d6f", "#b7791f", "#2563eb", "rgba(15,157,111,.18)"),
    "color-scheme": "light",
    "--hover": "rgba(0,0,0,.045)",
    "--term-bg": "#fbfbfc",
    "--pop-shadow": "0 18px 50px rgba(0,0,0,.15)",
}

# (id, nombre, [tres colores para la muestra], variables)
PALETTES = {
    "default": [
        ("mint", "Mint", ["#0d0f14", "#161a21", "#6ee7b7"], {}),
        ("nord", "Nord", ["#2e3440", "#3b4252", "#88c0d0"],
         dark("#242933", "#2b303b", "#2e3440", "#3b4252", "#3b4252", "#4c566a", "#eceff4", "#b4bccb", "#7b8394",
              "#88c0d0", "#1d2128", "#bf616a", "#ebcb8b", "#a3be8c", "#ebcb8b", "#81a1c1", "rgba(136,192,208,.25)")),
        ("dracula", "Dracula", ["#282a36", "#44475a", "#bd93f9"],
         dark("#21222c", "#1e1f29", "#282a36", "#343746", "#3a3c4e", "#4d5066", "#f8f8f2", "#b6b8c8", "#7b7f99",
              "#bd93f9", "#1e1f29", "#ff5555", "#f1fa8c", "#50fa7b", "#ffb86c", "#8be9fd", "rgba(189,147,249,.3)", "#ff79c6")),
        ("tokyo", "Tokyo Night", ["#1a1b26", "#24283b", "#7aa2f7"],
         dark("#16161e", "#1a1b26", "#1f2335", "#292e42", "#292e42", "#3b4261", "#c0caf5", "#9aa5ce", "#565f89",
              "#7aa2f7", "#16161e", "#f7768e", "#e0af68", "#9ece6a", "#e0af68", "#7dcfff", "rgba(122,162,247,.28)")),
        ("catppuccin", "Catppuccin", ["#1e1e2e", "#313244", "#cba6f7"],
         dark("#11111b", "#181825", "#1e1e2e", "#313244", "#313244", "#45475a", "#cdd6f4", "#a6adc8", "#6c7086",
              "#cba6f7", "#11111b", "#f38ba8", "#f9e2af", "#a6e3a1", "#fab387", "#89b4fa", "rgba(203,166,247,.25)", "#f5e0dc")),
        ("gruvbox", "Gruvbox", ["#282828", "#3c3836", "#fabd2f"],
         dark("#1d2021", "#1d2021", "#282828", "#32302f", "#3c3836", "#504945", "#ebdbb2", "#bdae93", "#7c6f64",
              "#fabd2f", "#1d2021", "#fb4934", "#fe8019", "#b8bb26", "#fe8019", "#83a598", "rgba(250,189,47,.22)")),
        ("paper", "Paper", ["#f6f7f9", "#ffffff", "#0f9d6f"], PAPER),
        ("srcery", "Srcery", ["#1c1b19", "#5c4c39", "#fbb829"], SRCERY_DEFAULT),
    ],
    "warp": [
        ("violet", "Violet", ["#000000", "#8b5cf6", "#22d3ee"], {}),
        ("solar", "Solar Flare", ["#000000", "#f97316", "#facc15"],
         warp("#f97316", "#facc15", "#fb923c", "#1c1206", "rgba(249,115,22,.22)", "rgba(250,204,21,.1)", "rgba(251,146,60,.3)")),
        ("pink", "Pink City", ["#000000", "#ec4899", "#8b5cf6"],
         warp("#ec4899", "#8b5cf6", "#f472b6", "#ffffff", "rgba(236,72,153,.22)", "rgba(139,92,246,.12)", "rgba(244,114,182,.3)")),
        ("jelly", "Jellyfish", ["#000000", "#06b6d4", "#3b82f6"],
         warp("#06b6d4", "#3b82f6", "#22d3ee", "#03141c", "rgba(6,182,212,.2)", "rgba(59,130,246,.12)", "rgba(34,211,238,.28)")),
        ("leafy", "Leafy", ["#000000", "#22c55e", "#a3e635"],
         warp("#22c55e", "#a3e635", "#4ade80", "#05140a", "rgba(34,197,94,.18)", "rgba(163,230,53,.1)", "rgba(74,222,128,.28)")),
        ("redrock", "Red Rock", ["#000000", "#ef4444", "#f97316"],
         warp("#ef4444", "#f97316", "#f87171", "#ffffff", "rgba(239,68,68,.2)", "rgba(249,115,22,.1)", "rgba(248,113,113,.3)")),
        ("ember", "Ember", ["#1c1b19", "#fbb829", "#f75341"],
         {**warp("#fbb829", "#f75341", "#fbb829", "#1c1b19", "rgba(251,184,41,.16)", "rgba(247,83,65,.1)", "rgba(251,184,41,.3)"),
          "--bg": "#121110", "--text": "#fce8c3", "--text-2": "#baa67f", "--text-3": "#918175",
          "--env-prod": "#f75341", "--env-test": "#fbb829", "--env-dev": "#baa67f", "--term-bg": "#121110", "--term-fg": "#fce8c3"}),
    ],
    "poolsuite": [
        ("sunset", "Sunset", ["#aee2ff", "#ffd3e1", "#ff4f9a"], {}),
        ("mint", "Mint Pool", ["#b8f2e6", "#fff6c7", "#0fa3a3"],
         pool("linear-gradient(180deg, #b8f2e6 0%, #d9f7ec 50%, #fff6c7 100%)", "#0fa3a3", "#f8fffc", "#fdfffd", "#e6fbf5")),
        ("tropicana", "Tropicana", ["#ffd166", "#ff8fa3", "#ef476f"],
         pool("linear-gradient(180deg, #ffd166 0%, #ffb38a 50%, #ff8fa3 100%)", "#ef476f", "#fffaf0", "#fffdf6", "#fff0dc")),
        ("lavender", "Lavender", ["#d9ccff", "#ffd8ec", "#7b5cff"],
         pool("linear-gradient(180deg, #d9ccff 0%, #f0d4ff 50%, #ffd8ec 100%)", "#7b5cff", "#fdfbff", "#fffdff", "#f3edff")),
        ("beach", "Beach", ["#7fd3ff", "#fff1c1", "#0077ff"],
         pool("linear-gradient(180deg, #7fd3ff 0%, #bfeaff 45%, #fff1c1 100%)", "#0077ff", "#f7fcff", "#fdfeff", "#eaf6ff")),
        ("platinum", "Platinum", ["#bdbdbd", "#eeeeee", "#3366cc"],
         {**pool("linear-gradient(180deg, #cfcfcf, #bdbdbd)", "#3366cc", "#e3e3e3", "#eeeeee", "#dddddd"), "--term-bg": "#ffffff"}),
        # La app nueva de Poolsuite: crema y casi negro sobre papel tramado, botones negros.
        ("mono", "Monochrome", ["#c9ae82", "#f2e9d4", "#1c1714"],
         {**pool("repeating-conic-gradient(rgba(60,40,20,.07) 0 25%, transparent 0 50%) 0 0 / 3px 3px, "
                 "radial-gradient(ellipse 80% 70% at 30% 20%, #dcc8a2, #c6aa7b 75%)",
                 "#1c1714", "#efe6d0", "#f4ecd8", "#e6dbc1", ink="#f4ecd8"),
          "--line": "#1c1714", "--line-strong": "#1c1714", "--text": "#1c1714", "--text-2": "#4a3f35", "--text-3": "#7d6f5f",
          "--hover": "rgba(28,23,20,.07)", "--card-shadow": "3px 3px 0 #1c1714", "--pop-shadow": "6px 6px 0 #1c1714",
          "--danger": "#8f2d1f", "--warn": "#8a6414", "--ok": "#3d5a3a",
          "--env-prod": "#8f2d1f", "--env-test": "#8a6414", "--env-dev": "#3b4f5c",
          "--term-fg": "#1c1714", "--term-selection": "rgba(28,23,20,.2)",
          "--titlebar": "repeating-linear-gradient(180deg, #1c1714 0 2px, transparent 2px 5px)"}),
        ("vellum", "Vellum", ["#ece1c6", "#f3ead3", "#7c5510"],
         {**pool("linear-gradient(180deg, #f3ead3 0%, #ece1c6 55%, #d3c6a3 100%)", "#7c5510", "#f3ead3", "#fbf5e6", "#ece1c6"),
          "--line": "#1f1914", "--line-strong": "#1f1914", "--text": "#1f1914", "--text-2": "#473c30", "--text-3": "#6e5f4c",
          "--card-shadow": "3px 3px 0 #1f1914", "--pop-shadow": "6px 6px 0 #1f1914", "--danger": "#9a3522", "--env-prod": "#9a3522",
          "--env-test": "#7c5510", "--env-dev": "#473c30", "--term-fg": "#1f1914",
          "--titlebar": "repeating-linear-gradient(180deg, #1f1914 0 2px, transparent 2px 5px)"}),
    ],
    "haze": [
        ("cream", "Cream", ["#1b2126", "#7a6c58", "#f9e2af"], {}),
        ("peach", "Peach", ["#1b2126", "#8c6046", "#fab387"],
         hz("#fab387", "rgba(140,96,70,.45)", "rgba(96,80,100,.36)", "rgba(58,52,48,.5)")),
        ("rose", "Rose", ["#1b2126", "#826e7c", "#f5c2e7"],
         hz("#f5c2e7", "rgba(130,90,110,.42)", "rgba(92,72,112,.38)", "rgba(56,44,62,.5)")),
        ("mauve", "Mauve", ["#1b2126", "#685682", "#cba6f7"],
         hz("#cba6f7", "rgba(104,86,130,.42)", "rgba(62,72,120,.38)", "rgba(42,40,72,.5)")),
        ("sky", "Sky", ["#1b2126", "#406876", "#89dceb"],
         hz("#89dceb", "rgba(64,104,118,.42)", "rgba(60,82,120,.38)", "rgba(30,58,72,.5)")),
        ("sage", "Sage", ["#1b2126", "#546e54", "#a6e3a1"],
         hz("#a6e3a1", "rgba(84,110,84,.42)", "rgba(52,86,86,.4)", "rgba(34,58,48,.5)")),
        ("amber", "Amber", ["#1c1b19", "#5c4c39", "#fbb829"],
         {**hz("#fbb829", "rgba(120,94,58,.45)", "rgba(92,76,57,.4)", "rgba(42,40,36,.55)"),
          "--text": "#fce8c3", "--text-2": "rgba(252,232,195,.66)", "--text-3": "rgba(252,232,195,.42)",
          "--danger": "#f75341", "--env-prod": "#f75341", "--env-dev": "#baa67f"}),
    ],
    "athanor": [
        ("srcery", "Srcery", ["#1c1b19", "#2a2824", "#fbb829"], {}),
        ("umber", "Umber", ["#16120e", "#5c4c39", "#d49a3a"],
         ath("#16120e", "#0d0b08", "#231d16", "#3a3126", "#3a3126", "#5c4c39", "#8d7d65", "#cfc1a3", "#e3d6b8",
             "#d49a3a", "#e8bd6a", "#8f6520", "#dd7357")),
        ("cinnabar", "Cinnabar", ["#1c0e0c", "#6e3428", "#e2a24a"],
         ath("#1c0e0c", "#120807", "#2c1613", "#4a2420", "#4a2420", "#6e3428", "#a8857a", "#d8c4ac", "#ecd9c0",
             "#e2a24a", "#f2be6e", "#9a6a26", "#ff7a56")),
        ("vellum", "Vellum", ["#ece1c6", "#b9aa86", "#7c5510"],
         ath("#ece1c6", "#d3c6a3", "#f3ead3", "#fbf5e6", "#b9aa86", "#b9aa86", "#6e5f4c", "#473c30", "#1f1914",
             "#7c5510", "#a57a2a", "#553a08", "#9a3522", light=True)),
        ("orpiment", "Orpiment", ["#eddca8", "#c9b27a", "#7a4f06"],
         ath("#eddca8", "#d4bd80", "#f4e7bd", "#fbf2d6", "#c9b27a", "#c9b27a", "#6a5737", "#473922", "#241a0c",
             "#7a4f06", "#a3700f", "#553604", "#9a3522", light=True)),
    ],
}

root = pathlib.Path(__file__).resolve().parent.parent / "src"
css = ["/* Generado por scripts/palettes.py: editar ahi, no aca. */\n"]
for theme, pals in PALETTES.items():
    for pid, _, _, vars in pals:
        if not vars:
            continue
        body = "\n".join(f"  {k}: {v};" for k, v in vars.items())
        css.append(f'[data-theme="{theme}"][data-palette="{pid}"] {{\n{body}\n}}\n')
(root / "styles" / "palettes.css").write_text("\n".join(css), encoding="utf-8")

ts = {t: [{"id": p[0], "name": p[1], "swatch": p[2]} for p in pals] for t, pals in PALETTES.items()}
(root / "lib" / "palettes.gen.ts").write_text(
    "// Generado por scripts/palettes.py: editar ahi, no aca.\n"
    "export const PALETTES: Record<string, { id: string; name: string; swatch: string[] }[]> = "
    f"{json.dumps(ts, indent=2)};\n",
    encoding="utf-8",
)
print("ok", sum(len(p) for p in PALETTES.values()), "paletas")
