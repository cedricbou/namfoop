#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyte", "fonttools", "pillow"]
# ///
"""Refait les images du démarrage rapide (`docs/demarrage-rapide.md`) : captures et animations.

    uv run scripts/gen-demarrage.py                 # tout, dans docs/demarrage/
    uv run scripts/gen-demarrage.py --only chaine   # une seule image (voir STAGES)
    uv run scripts/gen-demarrage.py --work /tmp/d   # garde la bibliothèque de démonstration

Le script joue le parcours d'un nouveau venu — `setup`, `calibrate`, `import`, `browse`,
`run --set` — dans un pseudo-terminal, sur la **carte simulée** : aucune carte son n'est ouverte,
rien n'est audible. Il lit l'écran avec un émulateur de terminal (pyte) et le redessine aux couleurs
et aux polices de `docs/identite/charte.md`.

Rien de ce qui appartient à l'utilisateur n'est touché : la config et la bibliothèque sont bâties
dans un dossier jetable, à partir des zips de `nam/` (non versionnés — `--packs` pour un autre
dossier). Les presets, le set et le prompteur de la démonstration sont écrits ici.

Ce qu'il faut sur la machine, et que le script vérifie avant de commencer : le binaire en release,
`rsvg-convert`, `ffmpeg`, et les trois polices de la charte (JetBrains Mono NL, Inter, Chakra
Petch) connues de fontconfig — ou rangées dans un dossier donné par `NAMFOOP_FONTS`. La variante
« NL » de JetBrains Mono n'a pas de ligatures : `->` reste deux caractères, comme dans un terminal.

Les pièges du pilotage d'un écran ratatui sont ceux de `scripts/check-e2e.py`. Deux s'ajoutent ici,
parce qu'on **lit** l'écran au lieu d'y chercher un mot : pyte ignore l'écran alterné (on efface
donc à sa place quand il s'ouvre), et les seize couleurs de base lui arrivent en palette xterm
(`XTERM16` les ramène à la charte).
"""

import argparse
import atexit
import copy
import fcntl
import os
import pty
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
from xml.sax.saxutils import escape

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "docs", "demarrage")
BIN = os.path.join(ROOT, "target", "release", "namfoop")

# Les packs de la démonstration : cinq de modèles, quatre d'IR. Le dernier est importé à l'écran.
PACKS = [
    "Bass Ampeg SVT 8x10", "Darkglass DG212NE Cab IR", "Mesa Powerhouse 410 & 410+215",
    "Ampeg B15N", "Ampeg SVT Classic with 6x10", "Darkglass B7K Ultra",
    "Fender Bassman 800 Full Rig Capture", "deluxe bass Big Muff Pi",
]
SHOWN_PACK = "MESA _ BOOGIE BASS 800D"

# --- La charte (docs/identite/charte.md) ----------------------------------------------------------

INK, GRAPHITE, STEEL, WHITE = "#0F0F14", "#1E1E28", "#8A8A9C", "#FFFFFF"
MAGENTA, CYAN, GREEN, AMBER, RED = "#FF007F", "#00F0FF", "#00E676", "#FFD600", "#FF3B30"
TEXT = "#D9D9E3"  # le texte courant d'un terminal (`Color::Gray`) : un blanc qui ne fatigue pas
BLUE, ORANGE, EDGE = "#5C8DFF", "#FF8A5C", "#2C2C3A"  # hors charte : bleu et rouge clair du terminal
MONO, SANS, TITLE = "JetBrains Mono NL", "Inter", "Chakra Petch"

BASE16 = [INK, RED, GREEN, AMBER, BLUE, MAGENTA, CYAN, TEXT,
          STEEL, ORANGE, GREEN, AMBER, BLUE, MAGENTA, CYAN, WHITE]
NAMES16 = ["black", "red", "green", "brown", "blue", "magenta", "cyan", "white",
           "brightblack", "brightred", "brightgreen", "brightbrown", "brightblue", "brightmagenta",
           "brightcyan", "brightwhite"]
XTERM16 = ["000000", "cd0000", "00cd00", "cdcd00", "0000ee", "cd00cd", "00cdcd", "e5e5e5",
           "7f7f7f", "ff0000", "00ff00", "ffff00", "5c5cff", "ff00ff", "00ffff", "ffffff"]
PALETTE = {**dict(zip(NAMES16, BASE16)), **dict(zip(XTERM16, BASE16))}


def color(name, default):
    if name == "default":
        return default
    if name in PALETTE:
        return PALETTE[name]
    return "#" + name.upper() if len(name) == 6 else default


# --- Le pseudo-terminal -----------------------------------------------------------------------------

UP, DOWN, RIGHT, LEFT = "\x1b[A", "\x1b[B", "\x1b[C", "\x1b[D"
ENTER, ESC, TAB, SPACE, BACKSPACE = "\r", "\x1b", "\t", " ", "\x7f"
PGDN = "\x1b[6~"


class Term:
    """Un programme dans un pseudo-terminal, et son écran tel qu'un terminal le montrerait."""

    def __init__(self, argv, rows, cols, env, cwd, subst=None):
        import pyte

        self.rows, self.cols = rows, cols
        self.screen = pyte.Screen(cols, rows)
        self.stream = pyte.ByteStream(self.screen)
        self.lock = threading.Lock()
        self.subst = subst or {}
        master, slave = pty.openpty()
        # sans taille posée, ratatui ne dessine rien
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        e = dict(os.environ, TERM="xterm-256color", LANG="fr_FR.UTF-8", LC_ALL="fr_FR.UTF-8", RUST_LOG="off")
        e.pop("COLORTERM", None)
        e.update(env)
        self.proc = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave, env=e, cwd=cwd,
                                     close_fds=True, start_new_session=True)
        os.close(slave)
        self.master = master
        self.alive = True
        threading.Thread(target=self._pump, daemon=True).start()

    def _pump(self):
        """Vide le pseudo-terminal en continu : un programme qui ne peut plus écrire se fige."""
        while self.alive:
            try:
                ready, _, _ = select.select([self.master], [], [], 0.05)
                if not ready:
                    continue
                data = os.read(self.master, 65536)
            except (OSError, ValueError):
                break
            if not data:
                break
            # `Terminal::clear` de ratatui demande où est le curseur et attend la réponse
            if b"\x1b[6n" in data:
                try:
                    os.write(self.master, b"\x1b[1;1R")
                except OSError:
                    pass
            for old, new in self.subst.items():
                data = data.replace(old, new)
            with self.lock:
                # l'écran alterné s'ouvre vide ; pyte l'ignore, on efface donc à sa place
                for i, part in enumerate(data.split(b"\x1b[?1049h")):
                    if i:
                        self.screen.reset()
                    self.stream.feed(part)

    def key(self, keys, wait=0.5):
        os.write(self.master, keys.encode())
        time.sleep(wait)

    def type(self, text, wait=0.5):
        for ch in text:
            os.write(self.master, ch.encode())
            time.sleep(0.04)
        time.sleep(wait)

    def text(self):
        with self.lock:
            return "\n".join(self.screen.display)

    def wait_for(self, needle, timeout=40.0):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if needle in self.text():
                return
            time.sleep(0.1)
        raise SystemExit(f"✗ « {needle} » n'est pas apparu à l'écran :\n{self.text()}")

    def wait_exit(self, timeout=60.0):
        end = time.monotonic() + timeout
        while time.monotonic() < end and self.proc.poll() is None:
            time.sleep(0.1)
        time.sleep(0.3)
        return self.proc.poll()

    def warn_xruns(self, name):
        """Une machine occupée fait décrocher la carte simulée, et l'écran capturé le montre : le
        dire, pour que la scène soit rejouée (`--only`) plutôt que publiée avec ses décrochages."""
        if "xruns" in self.text() and "xruns 0" not in self.text():
            print(f"  ⚠ {name} : la carte simulée a décroché pendant la capture, la rejouer (--only {name})")

    def grid(self):
        with self.lock:
            buf = self.screen.buffer
            return [[copy.copy(buf[y][x]) for x in range(self.cols)] for y in range(self.rows)]

    def close(self):
        self.alive = False
        try:
            self.proc.kill()
            self.proc.wait(timeout=2)
        except Exception:
            pass
        try:
            os.close(self.master)
        except OSError:
            pass


# --- Le dessin d'un écran ---------------------------------------------------------------------------

CW, CH, FS = 9, 19, 15  # la cellule et le corps : JetBrains Mono avance de 0,6 em
PAD, BAR, CAP = 14, 34, 46  # marge, barre de titre, bandeau de légende

# traits de boîte : (gauche, droite, haut, bas) ; 1 = fin, 2 = gras
BOX = {
    "─": (1, 1, 0, 0), "│": (0, 0, 1, 1), "┌": (0, 1, 0, 1), "┐": (1, 0, 0, 1), "└": (0, 1, 1, 0),
    "┘": (1, 0, 1, 0), "├": (0, 1, 1, 1), "┤": (1, 0, 1, 1), "┬": (1, 1, 0, 1), "┴": (1, 1, 1, 0),
    "┼": (1, 1, 1, 1), "━": (2, 2, 0, 0), "┃": (0, 0, 2, 2), "╭": (0, 1, 0, 1), "╮": (1, 0, 0, 1),
    "╰": (0, 1, 1, 0), "╯": (1, 0, 1, 0), "═": (2, 2, 0, 0), "║": (0, 0, 2, 2),
}
LOWER, LEFTB = "▁▂▃▄▅▆▇█", "▏▎▍▌▋▊▉"
_cmap = set()


def geometry(ch, x, y, fg):
    """Les caractères de dessin, en rectangles : jointifs quel que soit l'interligne, là où les
    glyphes d'une police laissent un jour entre deux lignes. Rend None pour un caractère de texte."""
    def rect(rx, ry, rw, rh, extra=""):
        return f'<rect x="{rx:g}" y="{ry:g}" width="{rw:g}" height="{rh:g}" fill="{fg}"{extra}/>'

    if ch in BOX:
        left, right, up, down = BOX[ch]
        cx, cy = x + CW / 2, y + CH / 2
        out = []
        if left:
            out.append(rect(x, cy - left / 2, CW / 2 + left / 2, left))
        if right:
            out.append(rect(cx - right / 2, cy - right / 2, CW / 2 + right / 2, right))
        if up:
            out.append(rect(cx - up / 2, y, up, CH / 2 + up / 2))
        if down:
            out.append(rect(cx - down / 2, cy - down / 2, down, CH / 2 + down / 2))
        return "".join(out)
    if ch in LOWER:
        h = CH * (LOWER.index(ch) + 1) / 8
        return rect(x, y + CH - h, CW, h)
    if ch in LEFTB:
        return rect(x, y, CW * (LEFTB.index(ch) + 1) / 8, CH)
    if ch == "▀":
        return rect(x, y, CW, CH / 2)
    if ch == "▐":
        return rect(x + CW / 2, y, CW / 2, CH)
    if ch in "░▒▓":
        return rect(x, y, CW, CH, f' opacity="{ {"░": 0.25, "▒": 0.5, "▓": 0.75}[ch]}"')
    if 0x2800 <= ord(ch) <= 0x28FF:  # braille : huit points, en 2 × 4
        dots = [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2), (0, 3), (1, 3)]
        return "".join(
            f'<circle cx="{x + CW * (0.25 + 0.5 * dx):g}" cy="{y + CH * (0.125 + 0.25 * dy):g}" r="1.25" fill="{fg}"/>'
            for i, (dx, dy) in enumerate(dots) if (ord(ch) - 0x2800) & (1 << i))
    return None


def in_font(ch):
    return len(ch) == 1 and ord(ch) in _cmap and geometry(ch, 0, 0, "") is None


def cells(grid, ox, oy):
    """Les cellules pyte en SVG : fonds, géométrie, texte. Le texte part en séries d'un même style ;
    un symbole absent de la police est posé seul, centré dans sa cellule, pour ne rien décaler."""
    backgrounds, shapes, texts = [], [], []
    for row, line in enumerate(grid):
        y = oy + row * CH
        n = len(line)
        colors = []
        for c in line:
            fg, bg = color(c.fg, TEXT), color(c.bg, None)
            if c.reverse:
                fg, bg = (bg or INK), fg
            colors.append((fg, bg))
        col = 0
        while col < n:
            bg = colors[col][1]
            start = col
            while col < n and colors[col][1] == bg:
                col += 1
            if bg:
                backgrounds.append(f'<rect x="{ox + start * CW}" y="{y}" width="{(col - start) * CW}" height="{CH}" fill="{bg}"/>')

        def style(i):
            c = line[i]
            return (colors[i][0], c.bold, c.italics, c.strikethrough, c.underscore)

        col = 0
        while col < n:
            ch = line[col].data
            if ch in ("", " "):
                col += 1
                continue
            fg, x = colors[col][0], ox + col * CW
            shape = geometry(ch, x, y, fg)
            if shape is not None:
                shapes.append(shape)
                col += 1
            elif in_font(ch):
                start, st, run = col, style(col), []
                while col < n:
                    d = line[col].data
                    if in_font(d) and style(col) == st:
                        run.append(d)
                    elif d == " " and col + 1 < n and in_font(line[col + 1].data) and style(col + 1) == st:
                        run.append(" ")
                    else:
                        break
                    col += 1
                attrs = f'fill="{fg}"'
                if st[1]:
                    attrs += ' font-weight="700"'
                if st[2]:
                    attrs += ' font-style="italic"'
                deco = [d for d, on in (("line-through", st[3]), ("underline", st[4])) if on]
                if deco:
                    attrs += f' text-decoration="{" ".join(deco)}"'
                texts.append(f'<text x="{ox + start * CW}" y="{y + 14.5}" {attrs} xml:space="preserve">{escape("".join(run))}</text>')
            else:
                wide = col + 1 < n and line[col + 1].data == ""  # un emoji tient deux cellules
                w = CW * (2 if wide else 1)
                texts.append(f'<text x="{x + w / 2:g}" y="{y + 14.5}" fill="{fg}" text-anchor="middle" '
                             f'font-family="Noto Sans Symbols 2, Noto Sans Symbols, DejaVu Sans Mono, Noto Color Emoji" '
                             f'font-size="{FS - (1 if wide else 0)}">{escape(ch)}</text>')
                col += 2 if wide else 1
    return backgrounds, shapes, texts


def wave(x, y, h, step=4.2, width=2.4):
    """L'onde du logo : sept barres magenta."""
    out = []
    for i, k in enumerate([0.12, 0.28, 0.56, 1.0, 0.56, 0.28, 0.12]):
        bh = max(width, h * k)
        out.append(f'<rect x="{x + i * step:g}" y="{y + (h - bh) / 2:g}" width="{width}" height="{bh:g}" rx="{width / 2}" fill="{MAGENTA}"/>')
    return "".join(out)


def keycap(x, y, label):
    """Une touche du clavier ; rend (svg, largeur)."""
    w = max(28, 16 + len(label) * 8.6)
    return (f'<rect x="{x}" y="{y}" width="{w:g}" height="26" rx="5" fill="{INK}" stroke="{CYAN}" stroke-width="1.2"/>'
            f'<text x="{x + w / 2:g}" y="{y + 18}" text-anchor="middle" font-family="{MONO}" font-size="14" '
            f'font-weight="700" fill="{CYAN}">{escape(label)}</text>'), w


def window(grid, title, caption=None, keys=None, caption_bar=False):
    """Un écran dans sa fenêtre : barre de titre à l'onde, et bandeau de légende pour les animations."""
    rows, cols = len(grid), len(grid[0])
    w = cols * CW + 2 * PAD
    body = rows * CH + 2 * PAD
    cap = CAP if (caption_bar or caption or keys) else 0
    h = BAR + body + cap
    backgrounds, shapes, texts = cells(grid, PAD, BAR + PAD)
    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">',
        f'<defs><clipPath id="c"><rect x="0.5" y="0.5" width="{w - 1}" height="{h - 1}" rx="10"/></clipPath></defs>',
        '<g clip-path="url(#c)">',
        f'<rect width="{w}" height="{h}" fill="{INK}"/><rect width="{w}" height="{BAR}" fill="{GRAPHITE}"/>',
        wave(PAD, 9, 16),
        f'<text x="{PAD + 40}" y="22" font-family="{MONO}" font-size="13.5" fill="{STEEL}">'
        f'<tspan fill="{CYAN}" font-weight="700">❯</tspan> {escape(title)}</text>',
        '<g shape-rendering="crispEdges">', *backgrounds, *shapes, "</g>",
        f'<g font-family="{MONO}" font-size="{FS}">', *texts, "</g>",
    ]
    if cap:
        y0 = BAR + body
        out.append(f'<rect y="{y0}" width="{w}" height="{cap}" fill="{GRAPHITE}"/>')
        out.append(f'<rect y="{y0}" width="{w / 2:g}" height="2" fill="{MAGENTA}"/>'
                   f'<rect x="{w / 2:g}" y="{y0}" width="{w / 2:g}" height="2" fill="{CYAN}"/>')
        x = PAD
        for k in keys or []:
            svg, kw = keycap(x, y0 + 11, k)
            out.append(svg)
            x += kw + 6
        if caption:
            out.append(f'<text x="{x + (6 if keys else 0)}" y="{y0 + 29.5}" font-family="{SANS}" font-weight="500" '
                       f'font-size="15.5" fill="{WHITE}">{escape(caption)}</text>')
    out += ["</g>", f'<rect x="0.5" y="0.5" width="{w - 1}" height="{h - 1}" rx="10" fill="none" stroke="{EDGE}"/>', "</svg>"]
    return "\n".join(out)


FONT_ENV = dict(os.environ)


def to_png(svg, path, zoom):
    tmp = path + ".svg"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(svg)
    subprocess.run(["rsvg-convert", "-z", str(zoom), "-o", path, tmp], check=True, env=FONT_ENV)
    os.remove(tmp)


def trimmed(grid):
    """L'écran sans ses lignes vides du bas : une commande en mode ligne n'en remplit qu'une partie."""
    last = max((i for i, line in enumerate(grid) if any(c.data.strip() for c in line)), default=0)
    return grid[:last + 1]


def still(svg, name, zoom=1.5, colors=96):
    """Une capture : un écran de terminal tient en peu de couleurs, la palette divise le poids."""
    from PIL import Image

    path = os.path.join(OUT, name)
    to_png(svg, path, zoom)
    im = Image.open(path).convert("RGBA")
    im.quantize(colors=colors, method=Image.Quantize.FASTOCTREE, dither=Image.Dither.NONE).save(path, optimize=True)
    print(f"  ✓ {name}  {os.path.getsize(path) // 1024} Ko")


class Film:
    """Une animation : des écrans tenus chacun un temps, avec la touche pressée et sa légende.

    Les écrans sont **retenus** pendant la session et dessinés après elle (`save`) : dessiner
    pendant que namfoop joue lui prend du processeur, et l'écran capturé afficherait des décrochages
    audio que la démonstration a provoqués elle-même."""

    def __init__(self, tmp, title, zoom=1.0):
        self.dir, self.title, self.zoom, self.frames = tempfile.mkdtemp(dir=tmp), title, zoom, []

    def shot(self, term, hold, caption, keys=None):
        self.frames.append((term.grid(), hold, caption, keys))

    def live(self, term, seconds, caption, keys=None, fps=4):
        """Plusieurs écrans d'affilée, pour que les vumètres et la battue bougent."""
        for _ in range(max(1, round(seconds * fps))):
            self.shot(term, 1 / fps, caption, keys)
            time.sleep(1 / fps)

    def save(self, name):
        listing = os.path.join(self.dir, "frames.txt")
        with open(listing, "w", encoding="utf-8") as f:
            for i, (grid, hold, caption, keys) in enumerate(self.frames):
                path = os.path.join(self.dir, f"{i:03d}.png")
                to_png(window(grid, self.title, caption, keys, caption_bar=True), path, self.zoom)
                f.write(f"file '{path}'\nduration {hold:.3f}\n")
            f.write(f"file '{path}'\n")
        palette, out = os.path.join(self.dir, "palette.png"), os.path.join(OUT, name)
        base = ["ffmpeg", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", listing]
        subprocess.run(base + ["-vf", "palettegen=max_colors=64:stats_mode=diff", palette], check=True)
        subprocess.run(base + ["-i", palette, "-lavfi", "paletteuse=dither=none:diff_mode=rectangle",
                               "-fps_mode", "vfr", "-loop", "0", out], check=True)
        print(f"  ✓ {name}  {os.path.getsize(out) // 1024} Ko, {len(self.frames)} images")


# --- Le schéma du parcours du son -------------------------------------------------------------------


def signal_path():
    """De la basse aux sorties : le seul dessin de la page qui n'est pas une capture."""
    w, h = 1000, 292
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">',
           f'<rect x="0.5" y="0.5" width="{w - 1}" height="{h - 1}" rx="10" fill="{INK}" stroke="{EDGE}"/>']

    def label(x, y, text, size=13.5, fill=STEEL, family=SANS, weight=500):
        out.append(f'<text x="{x}" y="{y}" text-anchor="middle" font-family="{family}" font-size="{size}" '
                   f'font-weight="{weight}" fill="{fill}">{escape(text)}</text>')

    def arrow(x0, x1, y, dash=""):
        out.append(f'<path d="M{x0},{y} H{x1 - 7}" stroke="{STEEL}" stroke-width="2" fill="none"{dash}/>'
                   f'<path d="M{x1},{y} l-9,-5 v10 z" fill="{STEEL}"/>')

    def chip(x, y, cw, text, sub, stroke=EDGE):
        out.append(f'<rect x="{x}" y="{y}" width="{cw}" height="58" rx="8" fill="{GRAPHITE}" stroke="{stroke}" stroke-width="1.5"/>')
        label(x + cw / 2, y + 26, text, 17, WHITE, TITLE, 700)
        label(x + cw / 2, y + 45, sub, 12.5)

    y = 98  # la ligne de la chaîne
    out.append(wave(28, y + 7, 44, step=7.5, width=4.5))
    label(53, y + 78, "ta basse")
    label(53, y + 96, "+ carte son")
    arrow(92, 122, y + 29)
    out.append(f'<rect x="124" y="40" width="690" height="216" rx="12" fill="none" stroke="{MAGENTA}" stroke-width="1.5"/>')
    out.append(f'<rect x="144" y="28" width="108" height="24" fill="{INK}"/>')
    label(198, 46, "namfoop", 18, WHITE, TITLE, 700)
    stages = [("trim", "ton niveau", EDGE), ("effets", "gate, comp, EQ", EDGE), ("NAM", "ampli, pédale", MAGENTA),
              ("IR", "baffle", CYAN), ("effets", "delay, réverbe", EDGE), ("niveau", "+ limiteur", EDGE)]
    x = 139
    for i, (name, sub, stroke) in enumerate(stages):
        chip(x, y, 100, name, sub, stroke)
        if i < len(stages) - 1:
            arrow(x + 100, x + 112, y + 29)
        x += 112
    label(469, 80, "la chaîne, bloc par bloc : tu la bâtis à l'écran, tu l'enregistres en preset")
    # le son direct passe sous la chaîne
    out.append(f'<path d="M158,{y + 58} V222 H814" stroke="{STEEL}" stroke-width="2" stroke-dasharray="5 5" fill="none"/>')
    label(469, 243, "le son direct (DI), sans traitement")
    # les sorties : chacune a son rôle
    arrow(814, 848, y + 29)
    out.append(f'<path d="M848,{y - 9} V{y + 67}" stroke="{STEEL}" stroke-width="2" fill="none"/>')
    arrow(814, 856, 222)
    chip(858, y - 38, 122, "sondier", "son traité", GREEN)
    chip(858, y + 38, 122, "in-ears", "traité + clic", GREEN)
    chip(858, 204, 122, "DI", "basse brute")
    out.append("</svg>")
    return "\n".join(out)


# --- La démonstration : config, bibliothèque, presets et set jetables ---------------------------------

PRESETS = {
    "SVT clair": [("fx", "comp", {"threshold": -22.0, "ratio": 3.0}),
                  ("nam", "nam/Ampeg SVT Classic with 6x10/Ampeg SVT - Ultra Lo SM57.nam"),
                  ("fx", "reverb", {"mix": 10.0})],
    "Slap funk": [("fx", "comp", {"threshold": -24.0}),
                  ("nam", "nam/MESA _ BOOGIE BASS 800D/MESA BASS 800D - SEND - SLAP FUNK FINGERSTYLE.nam"),
                  ("ir", "ir/Mesa Powerhouse 410 & 410+215/Mesa Powerhouse 410 (SM57 + MD421).wav"),
                  ("fx", "chorus", {})],
    "B7K drive": [("fx", "gate", {}),
                  ("nam", "nam/Darkglass B7K Ultra/Darkglass Drive - Microtubes B7K Ultra.nam")],
    "Muff fuzz": [("fx", "gate", {}),
                  ("nam", "nam/deluxe bass Big Muff Pi/Deluxe Bass Big Muff Pi G5.nam"),
                  ("ir", "ir/Mesa Powerhouse 410 & 410+215/Mesa Powerhouse 410 (SM57 + MD421).wav"),
                  ("fx", "delay", {"mix": 18.0})],
}

SET = '''name = "Samedi · La Péniche"

[[song]]
title = "Ouverture"
pc = 1
tempo_bpm = 96
  [[song.sound]]
  preset = "SVT clair"

[[song]]
title = "Le Phare"
pc = 2
tempo_bpm = 124
prompt = "prompts/le-phare.cho"
  [[song.sound]]
  preset = "SVT clair"
  [[song.sound]]
  preset = "Muff fuzz"
  volume_db = -1.5

[[song]]
title = "Marée basse"
pc = 3
tempo_bpm = 88
time = "6/8"
  [[song.sound]]
  preset = "Reggae rond"

[[song]]
title = "Néon"
pc = 4
tempo_bpm = 112
  [[song.sound]]
  preset = "Slap funk"
  [[song.sound]]
  preset = "B7K drive"

[[song]]
title = "Dernier métro"
pc = 5
tempo_bpm = 140
  [[song.sound]]
  preset = "B7K drive"
  [[song.sound]]
  preset = "Muff fuzz"
'''

PROMPT = '''{title: Le Phare}
{tempo: 124}
{time: 4/4}

{start_of_verse: Couplet 1}
{x_namfoop_sound: SVT clair}
{x_namfoop_bars: 4}
[Am]Le phare tourne [F]encore
Et la [C]mer se [G]tait
[Am]Trois bateaux [F]dehors
Per[C]sonne sur le [G]quai
{end_of_verse}

{start_of_chorus: Refrain}
{x_namfoop_sound: Muff fuzz}
{x_namfoop_bars: 8}
{comment: fuzz — monte d'un cran}
[F]Tout casse, [G]tout revient
[Am]La lumière [Em]nous tient
{end_of_chorus}

{start_of_verse: Couplet 2}
{x_namfoop_sound: SVT clair}
{x_namfoop_bars: 8}
[Am]Le vent change [F]de bord
On re[C]garde sans [G]bouger
{end_of_verse}
'''


class Demo:
    """Le dossier jetable où vit la démonstration : un `HOME`, et un dossier de travail avec ses zips."""

    def __init__(self, work, packs):
        self.work = work
        self.home = os.path.join(work, "home")
        self.cwd = os.path.join(work, "namfoop")
        self.packs = packs
        self.env = {"HOME": self.home, "XDG_CONFIG_HOME": os.path.join(self.home, ".config"),
                    "PATH": os.path.dirname(BIN) + os.pathsep + os.environ["PATH"], "EDITOR": "true"}
        # à l'écran, le dossier jetable se lit comme le dossier personnel de n'importe qui
        self.subst = {self.home.encode(): b"/home/bassiste"}
        os.makedirs(os.path.join(self.cwd, "nam"), exist_ok=True)
        os.makedirs(self.env["XDG_CONFIG_HOME"], exist_ok=True)

    def done(self, step):
        return os.path.exists(os.path.join(self.work, f".{step}"))

    def mark(self, step):
        open(os.path.join(self.work, f".{step}"), "w").close()

    def screen(self, args, rows, cols):
        """Un écran plein de namfoop."""
        return Term([BIN, *args], rows, cols, self.env, self.cwd, self.subst)

    def play(self, args, rows, cols):
        """Un écran qui joue (`browse`, `run --set`), sans MIDI — la liste des ports dépend de la
        machine — et avec un buffer large : le fil audio de la carte simulée n'a pas la priorité
        temps réel, et au buffer de `setup` un bureau occupé le fait décrocher de temps en temps."""
        return self.screen(["--no-midi", "--buffer", "256", *args], rows, cols)

    def shell(self, command, rows, cols):
        """Une commande en mode ligne, précédée de son invite — ce qu'on lirait dans son terminal."""
        script = 'printf "\\033[1;36m❯\\033[0m \\033[1m%s\\033[0m\\n" "$1"; eval "$1"'
        return Term(["sh", "-c", script, "sh", command], rows, cols, self.env, self.cwd, self.subst)

    def run(self, *args):
        subprocess.run([BIN, *args], check=True, env=dict(os.environ, **self.env), cwd=self.cwd,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def pack(self, name):
        src = os.path.join(self.packs, name + ".zip")
        if not os.path.exists(src):
            raise SystemExit(f"✗ pack absent : {src}\n  (les sons ne sont pas versionnés ; --packs DOSSIER pour les chercher ailleurs)")
        dst = os.path.join(self.cwd, "nam", name + ".zip")
        shutil.copy(src, dst)
        return dst

    def write_presets(self):
        folder = os.path.join(self.cwd, "library", "presets")
        os.makedirs(folder, exist_ok=True)
        for name, blocks in PRESETS.items():
            lines = [f'name = "{name}"', ""]
            for kind, what, *params in blocks:
                lines.append(f'[[block]]\nkind = "{kind}"')
                if kind == "fx":
                    lines.append(f'\n[block.fx]\ntype = "{what}"')
                    lines += [f"{k} = {v}" for k, v in params[0].items()]
                else:
                    lines.append(f'file = "{what}"')
                lines.append("")
            with open(os.path.join(folder, f"{name}.preset.toml"), "w", encoding="utf-8") as f:
                f.write("\n".join(lines))

    def write_set(self):
        folder = os.path.join(self.cwd, "library", "sets", "prompts")
        os.makedirs(folder, exist_ok=True)
        with open(os.path.join(self.cwd, "library", "sets", "samedi.set.toml"), "w", encoding="utf-8") as f:
            f.write(SET)
        with open(os.path.join(folder, "le-phare.cho"), "w", encoding="utf-8") as f:
            f.write(PROMPT)


# --- Les scènes -------------------------------------------------------------------------------------
# Chacune suppose les précédentes jouées : la config vient de `setup`, la calibration précède
# l'import (les modèles sont alors profilés avec la basse), et le set joue le preset de `chaine`.


def stage_parcours(demo, tmp):
    still(signal_path(), "parcours.png", colors=64)


def stage_setup(demo, tmp):
    t = demo.screen(["--backend", "sim", "setup"], 15, 100)
    film = Film(tmp, "namfoop setup")
    try:
        t.wait_for("étape 1/6")
        film.shot(t, 2.6, "Le backend : ALSA direct pour ta carte — ici la carte simulée, pour essayer sans matériel")
        t.key(ENTER, 1.0)
        t.wait_for("étape 2/6")
        film.shot(t, 2.2, "La carte son, avec son nombre d'entrées et de sorties", ["Entrée"])
        t.key(ENTER, 1.2)
        t.wait_for("étape 3/6")
        film.live(t, 3.0, "Joue : l'entrée dont le vumètre bouge est celle de ta basse", ["Entrée"])
        t.key(RIGHT, 1.0)
        t.wait_for("étape 4/6")
        film.shot(t, 3.0, "Le rôle de chaque sortie : sondier, in-ear, DI… et un bip pour la repérer", ["←", "→", "t"])
        t.key(ENTER, 1.5)
        t.wait_for("étape 5/6")
        film.shot(t, 2.0, "Chaque taille de buffer est essayée 5 s avec un vrai modèle", ["Entrée"])
        t.wait_for("choisir", 60)
        time.sleep(0.5)
        film.shot(t, 2.8, "Le plus petit buffer sans décrochage est présélectionné")
        t.key(ENTER, 1.0)
        t.wait_for("étape 6/6")
        film.shot(t, 3.6, "Le résumé : Entrée enregistre la carte et le routage, une fois pour toutes", ["Entrée"])
        t.key(ENTER, 1.0)
        t.wait_exit(5)
    finally:
        t.close()
    film.save("setup.gif")
    demo.mark("setup")


def stage_calibrate(demo, tmp):
    t = demo.shell("namfoop calibrate", 30, 100)
    try:
        if t.wait_exit(40) != 0:
            raise SystemExit(f"✗ calibrate a échoué :\n{t.text()}")
        still(window(trimmed(t.grid()), "namfoop calibrate"), "calibrate.png")
    finally:
        t.close()
    demo.mark("calibrate")


def stage_import(demo, tmp):
    for folder in ("nam", "library"):
        shutil.rmtree(os.path.join(demo.cwd, folder), ignore_errors=True)
    os.makedirs(os.path.join(demo.cwd, "nam"))
    for name in PACKS:
        demo.pack(name)
    demo.run("import", "nam/")
    zipped = demo.pack(SHOWN_PACK)
    t = demo.shell(f'namfoop import "nam/{os.path.basename(zipped)}"', 40, 128)
    try:
        if t.wait_exit(90) != 0:
            raise SystemExit(f"✗ import a échoué :\n{t.text()}")
        still(window(trimmed(t.grid()), "namfoop import"), "import.png")
    finally:
        t.close()
    demo.write_presets()
    demo.mark("import")


def stage_bibliotheque(demo, tmp):
    t = demo.play(["browse"], 31, 128)
    film = Film(tmp, "namfoop browse")
    try:
        t.wait_for("BANC D'ESSAI")
        time.sleep(1.5)
        film.shot(t, 3.0, "La Bibliothèque : chaque modèle NAM avec son spectre, son grain et son caractère", ["1"])
        for _ in range(3):
            t.key(DOWN, 1.3)
            film.shot(t, 1.3, "↓ écoute le modèle sous le curseur, au banc d'essai — sans coupure ni clic", ["↑", "↓"])
        t.key("/", 0.3)
        t.type("mesa", 0.6)
        film.shot(t, 1.8, "/ filtre la liste : ici « mesa »", ["/"])
        t.key(ENTER, 0.5)
        t.key(DOWN, 0.4)
        t.key(DOWN, 1.4)
        film.shot(t, 2.6, "Un ampli sans baffle reçoit d'office l'IR conseillée", ["↓"])
        t.key(TAB, 1.4)
        film.shot(t, 2.4, "Tab : les IR de baffle — les ★ sont conseillées pour ce modèle", ["Tab"])
        t.key(DOWN, 1.4)
        film.shot(t, 3.0, "À droite, la courbe de l'IR écoutée et ce que le son fait à ta basse, zone par zone", ["↓"])
        hero = t.grid()
        for _ in range(2):
            t.key(DOWN, 1.4)
            film.shot(t, 1.8, "Chaque IR s'écoute aussitôt, à volume égal", ["↑", "↓"])
        t.key(UP, 0.4)
        t.key(UP, 1.4)
        t.key(ENTER, 1.4)
        film.shot(t, 3.4, "Entrée met ce que tu entends dans la chaîne : modèle › IR", ["Entrée"])
        t.warn_xruns("bibliotheque")
    finally:
        t.key("q", 0.4)
        t.key("Q", 0.4)
        t.close()
    still(window(hero, "namfoop browse"), "bibliotheque.png")
    film.save("bibliotheque.gif")


def stage_chaine(demo, tmp):
    t = demo.play(["browse"], 28, 128)
    film = Film(tmp, "namfoop browse")

    def add(key, name):
        t.key(key, 0.6)
        t.type(name, 0.5)

    try:
        t.wait_for("BANC D'ESSAI")
        t.key("/", 0.3)
        t.type("reggae", 0.5)
        t.key(ENTER, 1.5)
        t.key(ENTER, 1.2)
        t.key("2", 1.5)
        film.shot(t, 3.0, "L'espace Chaîne : le son de l'entrée à la sortie, un vumètre au-dessus de chaque bloc", ["2"])
        add("i", "gate")
        film.shot(t, 2.6, "i ouvre le catalogue et ajoute avant le curseur (a : après). Tape pour filtrer", ["i"])
        t.key(ENTER, 1.2)
        add("a", "comp")
        t.key(ENTER, 1.2)
        film.shot(t, 2.4, "Un gate, puis un compresseur : chaque bloc arrive avec ses réglages", ["a"])
        t.key(RIGHT, 0.3)
        t.key(RIGHT, 0.3)
        add("a", "delay")
        t.key(ENTER, 1.2)
        t.key(ENTER, 0.6)
        film.shot(t, 2.2, "Entrée ouvre les réglages du bloc", ["Entrée"])
        for _ in range(5):
            t.key(DOWN, 0.15)
        for _ in range(4):
            t.key(LEFT, 0.15)
        time.sleep(1.0)
        film.shot(t, 2.8, "↑↓ choisit le réglage, ←→ change la valeur : tu l'entends aussitôt", ["↑", "↓", "←", "→"])
        t.key(ESC, 0.5)
        add("a", "reverb")
        t.key(ENTER, 1.2)
        film.shot(t, 2.6, "Une réverbe en bout de chaîne. [ et ] déplacent un bloc, x le retire, b le coupe", ["a"])
        t.key("S", 0.8)
        t.key(BACKSPACE * 60, 0.4)
        t.type("Reggae rond", 0.5)
        film.shot(t, 2.4, "S enregistre la chaîne en preset : donne-lui un nom", ["S"])
        t.key(ENTER, 1.5)
        film.shot(t, 3.4, "Le preset est dans ta bibliothèque, prêt pour la scène", ["Entrée"])
        t.warn_xruns("chaine")
    finally:
        t.key("q", 0.4)
        t.key("Q", 0.4)
        t.close()
    film.save("chaine.gif")

    # le parallèle, sur une chaîne courte : avec six blocs en série il déborderait de l'écran
    t = demo.play(["browse"], 24, 128)
    try:
        t.wait_for("BANC D'ESSAI")
        t.key("/", 0.3)
        t.type("muff g5", 0.5)
        t.key(ENTER, 1.5)
        t.key(ENTER, 1.2)
        t.key("2", 1.2)
        t.key("s", 2.0)
        parallel = t.grid()
        t.warn_xruns("chaine")
    finally:
        t.key("q", 0.4)
        t.key("Q", 0.4)
        t.close()
    still(window(parallel, "namfoop browse"), "parallele.png")
    demo.mark("chaine")


def stage_set(demo, tmp):
    demo.write_set()
    t = demo.shell("namfoop sets", 30, 100)
    try:
        t.wait_exit(20)
        still(window(trimmed(t.grid()), "namfoop sets"), "sets.png")
    finally:
        t.close()

    t = demo.play(["browse", "--set", "samedi"], 16, 128)
    try:
        t.wait_for("La Péniche")
        time.sleep(1.5)
        t.key("3", 1.0)
        t.key(DOWN, 0.3)
        t.key(ENTER, 1.5)
        t.key("e", 1.0)
        t.key(RIGHT, 0.6)
        edition = t.grid()
        t.warn_xruns("set")
    finally:
        t.key(ESC, 0.3)
        t.key("q", 0.4)
        t.key("Q", 0.4)
        t.close()
    still(window(edition, "namfoop browse --set samedi"), "set-edition.png")
    demo.mark("set")


def stage_scene(demo, tmp):
    t = demo.play(["run", "--set", "samedi"], 25, 128)
    film = Film(tmp, "namfoop run --set samedi")
    try:
        t.wait_for("Ouverture")
        time.sleep(1.5)
        film.shot(t, 3.0, "L'écran de scène, verrouillé : la set list à gauche, le morceau à droite")
        t.key(DOWN, 0.6)
        film.shot(t, 1.8, "↑↓ vise un morceau — rien ne change à l'oreille", ["↑", "↓"])
        t.key(ENTER, 1.5)
        film.shot(t, 3.0, "Entrée le joue : son premier son, son tempo, son prompteur", ["Entrée"])
        t.key(SPACE, 0.1)
        film.live(t, 4.0, "Espace lance la battue : l'appui est le 1 de la mesure", ["Espace"], fps=2)
        film.live(t, 3.5, "Le changement de son approche : le décompte prévient", fps=2)
        t.key(PGDN, 0.5)
        film.live(t, 3.0, "PgDn — ou le pédalier : son suivant en 20 ms, la réverbe du précédent finit de sonner", ["PgDn"], fps=2)
        t.warn_xruns("scene")
        t.key("S", 0.8)
        film.shot(t, 3.2, "Toute autre touche est refusée : sur scène, rien ne s'édite par accident", ["S"])
    finally:
        t.key("q", 0.4)
        t.key("Q", 0.4)
        t.close()
    film.save("scene.gif")


STAGES = {
    "parcours": stage_parcours, "setup": stage_setup, "calibrate": stage_calibrate, "import": stage_import,
    "bibliotheque": stage_bibliotheque, "chaine": stage_chaine, "set": stage_set, "scene": stage_scene,
}
# ce qu'une scène demande d'avoir joué avant elle, quand on n'en refait qu'une
NEEDS = {"calibrate": ["setup"], "import": ["setup", "calibrate"], "bibliotheque": ["import"],
         "chaine": ["import"], "set": ["chaine"], "scene": ["chaine", "set"]}


def check_tools():
    global _cmap
    for tool in ("rsvg-convert", "ffmpeg", "fc-list", "fc-match"):
        if not shutil.which(tool):
            raise SystemExit(f"✗ outil absent : {tool}")
    if not os.path.exists(BIN):
        raise SystemExit(f"✗ binaire absent : {BIN} (cargo build --release)")
    extra = os.environ.get("NAMFOOP_FONTS")
    if extra:
        folder = tempfile.mkdtemp(prefix="namfoop-fonts-")
        atexit.register(shutil.rmtree, folder, ignore_errors=True)
        conf = os.path.join(folder, "fonts.conf")
        with open(conf, "w", encoding="utf-8") as f:
            f.write('<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "fonts.dtd"><fontconfig>'
                    '<include ignore_missing="yes">/etc/fonts/fonts.conf</include>'
                    f"<dir>{escape(os.path.abspath(extra))}</dir></fontconfig>")
        FONT_ENV["FONTCONFIG_FILE"] = conf
    known = subprocess.run(["fc-list", ":", "family"], capture_output=True, text=True, env=FONT_ENV).stdout
    missing = [f for f in (MONO, SANS, TITLE) if f not in known]
    if missing:
        raise SystemExit(f"✗ polices absentes : {', '.join(missing)}\n  les installer, ou donner leur dossier dans NAMFOOP_FONTS")
    from fontTools.ttLib import TTFont

    mono = subprocess.run(["fc-match", "-f", "%{file}", f"{MONO}:style=Regular"], capture_output=True, text=True,
                          env=FONT_ENV).stdout
    _cmap = set(TTFont(mono).getBestCmap())


def main():
    p = argparse.ArgumentParser(description="Refait les images du démarrage rapide.")
    p.add_argument("--only", help=f"scènes à refaire, séparées par des virgules : {', '.join(STAGES)}")
    p.add_argument("--work", help="dossier de travail à garder d'un lancement à l'autre (défaut : jetable)")
    p.add_argument("--packs", default=os.path.join(ROOT, "nam", "Tone3000"), help="dossier des zips de sons")
    args = p.parse_args()
    check_tools()
    wanted = args.only.split(",") if args.only else list(STAGES)
    for name in wanted:
        if name not in STAGES:
            raise SystemExit(f"✗ scène inconnue : {name} (connues : {', '.join(STAGES)})")
    os.makedirs(OUT, exist_ok=True)
    work = args.work or tempfile.mkdtemp(prefix="namfoop-demarrage-")
    os.makedirs(work, exist_ok=True)
    tmp = tempfile.mkdtemp(prefix="namfoop-images-")
    demo = Demo(os.path.abspath(work), args.packs)
    try:
        needed = set()
        todo = list(wanted)
        while todo:
            for dep in NEEDS.get(todo.pop(), []):
                if dep not in needed:
                    needed.add(dep)
                    todo.append(dep)
        for name in STAGES:
            if name in wanted or (name in needed and not demo.done(name)):
                print(f"— {name}")
                STAGES[name](demo, tmp)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
        if not args.work:
            shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
