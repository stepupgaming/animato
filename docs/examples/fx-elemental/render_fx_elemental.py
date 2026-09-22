#!/usr/bin/env python3
"""Render cinematic 1280x720 Elemental Lab product-demo clips.

Every transform comes from the REAL Rust pipeline dumps
(``dump_frost_lance`` / ``dump_storm_lance`` / ``dump_cinder_fall`` / ``dump_nova_beam`` / ``dump_voltaic_snare`` / ``dump_glacial_crown`` / ``dump_pyre_crown`` / ``dump_kraken_crown``).
This script only handles presentation — dark glassy card UI, ice/cyan,
storm/blue, cinder/ember, nova/cyan-gold, voltaic/violet, glacial/ice, pyre/fire or kraken/teal palette, ffmpeg H.264 + GIF for issue embeds.

Usage:
    python3 render_fx_elemental.py [--ability frost|storm|cinder|nova|snare|glacial|pyre|all] [--output DIR] [--regen]
"""

from __future__ import annotations

import argparse
import json
import math
import shutil
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).parent
REPO_ROOT = HERE.parent.parent.parent

WIDTH, HEIGHT = 1280, 720
FPS = 30

PLOT_X, PLOT_Y, PLOT_W, PLOT_H = 120, 190, 1040, 400
BG = (5, 8, 18)
CARD = (12, 18, 34, 247)
PLOT_BG = (5, 12, 25, 242)
CYAN = (102, 224, 255)
BLUE = (47, 151, 239)
ICE = (212, 249, 255)
MUTED = (111, 135, 166)
DIM = (45, 74, 108)
GRID = (23, 44, 71, 115)

# World window (metres). The cast runs left (caster) → right (impact):
# world +z maps to plot x, world x maps to plot y. The window extends past
# the 12 m line so the terminal impact cluster (up to ~3 m past the far end)
# stays inside the card.
Z_MIN, Z_MAX = -2.0, 15.5
X_HALF = 5.8


def load_font(size: int, bold: bool = False):
    names = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf" if bold else "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf" if bold else "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
    ]
    for name in names:
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


FONT_KICKER = load_font(13, True)
FONT_TITLE = load_font(26, True)
FONT_SUBTITLE = load_font(13)
FONT_CHIP = load_font(11, True)
FONT_PLOT = load_font(11, True)
FONT_META = load_font(12)
FONT_META_BOLD = load_font(12, True)
FONT_TINY = load_font(10)


def clamp(v, lo, hi):
    return max(lo, min(hi, v))


def lerp3(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def world_to_screen(x: float, z: float) -> tuple[float, float]:
    px = PLOT_X + (z - Z_MIN) / (Z_MAX - Z_MIN) * PLOT_W
    py = PLOT_Y + PLOT_H * 0.5 - x * (PLOT_H * 0.5 / X_HALF)
    return (px, py)


def build_background() -> Image.Image:
    yy, xx = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float32)
    radial = np.exp(-(((xx - 690.0) / 620.0) ** 2 + ((yy - 300.0) / 500.0) ** 2))
    edge = np.clip(np.sqrt(((xx - WIDTH / 2) / (WIDTH / 2)) ** 2 + ((yy - HEIGHT / 2) / (HEIGHT / 2)) ** 2), 0, 1)
    rgb = np.empty((HEIGHT, WIDTH, 3), dtype=np.uint8)
    rgb[..., 0] = np.clip(4 + 4 * radial - 1.5 * edge, 0, 255)
    rgb[..., 1] = np.clip(7 + 10 * radial - 2.0 * edge, 0, 255)
    rgb[..., 2] = np.clip(17 + 24 * radial - 3.0 * edge, 0, 255)
    return Image.fromarray(rgb, mode="RGB").convert("RGBA")


BACKGROUND = build_background()


def alpha_layer() -> tuple[Image.Image, ImageDraw.ImageDraw]:
    layer = Image.new("RGBA", (WIDTH, HEIGHT), (0, 0, 0, 0))
    return layer, ImageDraw.Draw(layer)


def base_scene(kicker: str, title: str, subtitle: str, chip: str, right_meta: str) -> tuple[Image.Image, ImageDraw.ImageDraw]:
    image = BACKGROUND.copy()
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((68, 48, 1212, 672), radius=22, fill=CARD, outline=(30, 57, 88, 230), width=2)
    draw.line((96, 166, 1184, 166), fill=(30, 55, 83, 190), width=1)
    draw.text((120, 78), kicker, font=FONT_KICKER, fill=(85, 184, 236))
    draw.text((120, 101), title, font=FONT_TITLE, fill=(231, 244, 255))
    draw.text((120, 140), subtitle, font=FONT_SUBTITLE, fill=MUTED)
    chip_w = max(130, int(draw.textlength(chip, font=FONT_CHIP) + 48))
    chip_x = 1160 - chip_w
    draw.rounded_rectangle((chip_x, 91, 1160, 125), radius=17, fill=(11, 43, 69, 245), outline=(42, 122, 167, 220), width=1)
    draw.ellipse((chip_x + 14, 103, chip_x + 21, 110), fill=CYAN)
    draw.text((chip_x + 31, 99), chip, font=FONT_CHIP, fill=(167, 230, 255))

    draw.rounded_rectangle((PLOT_X - 1, PLOT_Y - 1, PLOT_X + PLOT_W + 1, PLOT_Y + PLOT_H + 1), radius=12, fill=PLOT_BG, outline=(29, 62, 93, 255), width=2)
    for x in range(PLOT_X + 40, PLOT_X + PLOT_W, 80):
        draw.line((x, PLOT_Y + 2, x, PLOT_Y + PLOT_H - 2), fill=GRID, width=1)
    for y in range(PLOT_Y + 40, PLOT_Y + PLOT_H, 56):
        draw.line((PLOT_X + 2, y, PLOT_X + PLOT_W - 2, y), fill=GRID, width=1)
    draw.text((PLOT_X + 22, PLOT_Y + 16), "LIVE PREVIEW  —  FROM RUST DUMP", font=FONT_PLOT, fill=(78, 139, 181))
    draw.text((PLOT_X + PLOT_W - 150, PLOT_Y + 16), right_meta, font=FONT_TINY, fill=(74, 112, 146))
    draw.text((120, 625), "ANIMATO  /  ELEMENTAL LAB", font=FONT_META_BOLD, fill=(74, 116, 151))
    draw.text((370, 625), "FROST LANCE (Q)  —  SEEDED, SEEKABLE PIPELINE", font=FONT_META, fill=(69, 92, 122))
    return image, ImageDraw.Draw(image)


SUBTITLES = {
    "Travel": "fracture front racing — spike field erupting behind it",
    "Impact": "impact punch — wall of blades + cluster standing",
    "Fade": "withdrawal — field sinking back into the floor",
    "Done": "pipeline complete — ready for the next cast",
    "Idle": "aim solution locked — cast armed",
}

CHIPS = {
    "Travel": "TRAVEL — FRACTURE FRONT",
    "Impact": "IMPACT — FIELD STANDING",
    "Fade": "FADE — WITHDRAWAL",
    "Done": "DONE",
    "Idle": "AIM — CAST ARMED",
}


def load_dump(dump_path: Path, regen: bool) -> dict:
    if regen or not dump_path.exists() or dump_path.stat().st_size == 0:
        cargo = shutil.which("cargo")
        if cargo is None:
            raise RuntimeError("cargo is required to regenerate the frame dump")
        print(f"regenerating {dump_path} via dump_frost_lance example…")
        subprocess.run(
            [cargo, "run", "-q", "-p", "animato-fx-elemental",
             "--example", "dump_frost_lance", "--", str(dump_path)],
            cwd=REPO_ROOT,
            check=True,
        )
    with open(dump_path) as f:
        return json.load(f)


def phase_boundaries(frames: list[dict]) -> tuple[float, float, float]:
    travel_end = fade_start = frames[-1]["t"]
    for fr in frames:
        if fr["phase"] == "Impact":
            travel_end = fr["t"]
            break
    for fr in frames:
        if fr["phase"] == "Fade":
            fade_start = fr["t"]
            break
    return travel_end, fade_start, frames[-1]["t"]


def frost_frame(frame: dict, dump: dict, bounds: tuple[float, float, float]) -> Image.Image:
    travel_end, fade_start, total = bounds
    phase = frame["phase"]
    t = frame["t"]
    n = len(frame["spikes"])
    image, _ = base_scene(
        "ELEMENTAL SANDBOX  /  FROST LANCE (Q)",
        "FROST LANCE",
        SUBTITLES.get(phase, ""),
        CHIPS.get(phase, phase.upper()),
        "SEED 7  /  SEEKABLE",
    )

    # Frost wash over the erupted region (caster → front), soft-blurred.
    wash, wash_draw = alpha_layer()
    _, front_z = world_to_screen(0.0, frame["front_pos"][1])
    wash_draw.rectangle((PLOT_X, PLOT_Y, front_z, PLOT_Y + PLOT_H), fill=(38, 140, 190, 46))
    image.alpha_composite(wash.filter(ImageFilter.GaussianBlur(14)))

    # Cast spine (dim) from caster to impact.
    c0 = world_to_screen(0.0, 0.0)
    c1 = world_to_screen(0.0, frame["impact_pos"][1])
    ImageDraw.Draw(image).line((c0, c1), fill=(40, 95, 140, 255), width=1)

    # Aim overlay for the opening beat: dashed arrow + lock tag (presentation
    # only — the pipeline itself starts at front 0).
    if t < 0.45:
        overlay, odraw = alpha_layer()
        steps = 24
        for i in range(steps):
            s0 = i / steps
            s1 = (i + 0.55) / steps
            if i % 2 == 0:
                p0 = (c0[0] + (c1[0] - c0[0]) * s0, c0[1])
                p1 = (c0[0] + (c1[0] - c0[0]) * min(s1, 1.0), c0[1])
                odraw.line((p0, p1), fill=(150, 225, 255, 235), width=2)
        odraw.polygon([(c1[0], c1[1] - 9), (c1[0] + 14, c1[1]), (c1[0], c1[1] + 9)], fill=(150, 225, 255, 235))
        image.alpha_composite(overlay)
        draw = ImageDraw.Draw(image)
        tag = "AIM LOCKED — 12.0 M"
        tw = draw.textlength(tag, font=FONT_TINY)
        draw.rounded_rectangle((c0[0] - 8, c0[1] - 44, c0[0] + tw + 24, c0[1] - 20),
                               radius=9, fill=(7, 22, 40, 230), outline=(60, 170, 220, 220))
        draw.text((c0[0] + 8, c0[1] - 39), tag, font=FONT_TINY, fill=(150, 225, 255))

    # Impact flash + expanding rings right after the front arrives.
    if phase == "Impact":
        age_i = t - travel_end
        flash = max(0.0, 0.42 - age_i * 0.9)
        if flash > 0.0:
            veil, _ = alpha_layer()
            ImageDraw.Draw(veil).rectangle(
                (PLOT_X, PLOT_Y, PLOT_X + PLOT_W, PLOT_Y + PLOT_H),
                fill=(190, 240, 255, int(flash * 255)))
            image.alpha_composite(veil)
        ip = world_to_screen(frame["impact_pos"][0], frame["impact_pos"][1])
        rings, rdraw = alpha_layer()
        for k in range(3):
            rk = age_i * 130.0 - k * 26.0
            if rk > 6.0:
                rdraw.ellipse((ip[0] - rk, ip[1] - rk, ip[0] + rk, ip[1] + rk),
                              outline=(140, 230, 255, max(0, int(200 - age_i * 160 - k * 40))), width=2)
        image.alpha_composite(rings.filter(ImageFilter.GaussianBlur(2)))

    # Spikes, small → large so blades layer over rubble. All placement, size
    # and brightness derive from the dumped Rust samples.
    glow, _ = alpha_layer()
    order = sorted(frame["spikes"], key=lambda s: s["h"] * max(0.0, s["e"]))
    draw = ImageDraw.Draw(image)
    glow_draw = ImageDraw.Draw(glow)
    for s in order:
        e = s["e"]
        if e < 0.0:
            continue  # still buried ahead of the front
        sx, sy = world_to_screen(s["x"], s["z"])
        if not (PLOT_X - 8 < sx < PLOT_X + PLOT_W + 8 and PLOT_Y - 70 < sy < PLOT_Y + PLOT_H + 10):
            continue
        hv = s["h"] * clamp(e, 0.0, 1.2)
        sink_dim = clamp(1.0 + s["y"] * 0.5, 0.08, 1.0)
        length_px = max(2.0, (3.0 + hv * 7.0) * clamp(1.0 + s["y"] * 0.25, 0.2, 1.0))
        half_w = max(1.5, s["r"] * 9.0 + 1.5)
        lean_px = length_px * 0.32  # away from the caster, as in sample_spike
        e2 = clamp(e, 0.0, 1.0)
        col = lerp3((34, 124, 196), (120, 225, 255), e2)
        b = clamp(s["b"], 0.0, 1.0)
        if b > 0.0:
            col = lerp3(col, (238, 251, 255), b * 0.85)
        alpha = int((140 + 115 * e2) * sink_dim)
        tip = (sx + lean_px, sy - length_px)
        base_l = (sx - half_w, sy)
        base_r = (sx + half_w, sy)
        draw.polygon([tip, base_r, (sx, sy + 2), base_l], fill=(*col, alpha))
        if b > 0.03 or hv > 2.2:
            r = half_w + length_px * 0.35
            glow_draw.ellipse((sx - r, sy - length_px - r * 0.4, sx + r, sy + r * 0.4),
                              fill=(*col, int(90 * sink_dim)))
    image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(9)))
    draw = ImageDraw.Draw(image)

    # Fracture front: bright vertical blade + glow. The tag sits on the side
    # with room so it never collides with the plot header text.
    fx = world_to_screen(0.0, frame["front_pos"][1])[0]
    if phase in ("Travel", "Impact"):
        front, _ = alpha_layer()
        fdraw = ImageDraw.Draw(front)
        fdraw.line((fx, PLOT_Y + 4, fx, PLOT_Y + PLOT_H - 4), fill=(170, 235, 255, 235), width=2)
        image.alpha_composite(front.filter(ImageFilter.GaussianBlur(4)))
        draw.line((fx, PLOT_Y + 4, fx, PLOT_Y + PLOT_H - 4), fill=(225, 250, 255, 255), width=1)
        label = "FRACTURE FRONT"
        tw = draw.textlength(label, font=FONT_TINY)
        if fx + 14 + tw + 12 <= PLOT_X + PLOT_W - 8:
            lx = fx + 14
        else:
            lx = fx - tw - 26
        ly = PLOT_Y + 40
        draw.rounded_rectangle((lx - 6, ly, lx + tw + 8, ly + 20),
                               radius=8, fill=(7, 22, 40, 225), outline=(60, 170, 220, 200))
        draw.text((lx, ly + 4), label, font=FONT_TINY, fill=(150, 225, 255))

    # Caster + impact markers.
    draw.ellipse((c0[0] - 7, c0[1] - 7, c0[0] + 7, c0[1] + 7),
                 outline=(150, 225, 255, 255), width=2)
    draw.ellipse((c0[0] - 2, c0[1] - 2, c0[0] + 2, c0[1] + 2), fill=(*CYAN, 255))
    draw.text((c0[0] - 20, c0[1] + 12), "CASTER", font=FONT_TINY, fill=(93, 159, 197))
    ic = world_to_screen(frame["impact_pos"][0], frame["impact_pos"][1])
    draw.ellipse((ic[0] - 9, ic[1] - 9, ic[0] + 9, ic[1] + 9), outline=(150, 225, 255, 200), width=1)
    draw.text((ic[0] - 18, ic[1] + 13), "IMPACT", font=FONT_TINY, fill=(93, 159, 197))

    # Timeline scrubber on its own row above the HUD pills, with phase ticks
    # (I = impact punch, F = withdrawal start).
    bar_y = PLOT_Y + PLOT_H - 62
    bx0, bx1 = PLOT_X + 40, PLOT_X + PLOT_W - 40
    draw.line((bx0, bar_y, bx1, bar_y), fill=(40, 80, 115, 255), width=2)
    for edge, tag in ((travel_end, "I"), (fade_start, "F")):
        tx = bx0 + (bx1 - bx0) * edge / total
        draw.line((tx, bar_y - 5, tx, bar_y + 5), fill=(90, 170, 215, 255), width=1)
        draw.text((tx - 3, bar_y - 19), tag, font=FONT_TINY, fill=(74, 112, 146))
    knob = bx0 + (bx1 - bx0) * t / total
    draw.ellipse((knob - 5, bar_y - 5, knob + 5, bar_y + 5),
                 fill=(*CYAN, 255), outline=(*ICE, 255), width=1)

    # HUD pills.
    ltext = f"PHASE {phase.upper()}  —  FRONT {frame['front']:04.1f} / {dump['length']:04.1f} M"
    draw.rounded_rectangle((PLOT_X + 22, PLOT_Y + PLOT_H - 43, PLOT_X + 22 + 330, PLOT_Y + PLOT_H - 16),
                           radius=12, fill=(7, 22, 40, 220), outline=(30, 87, 121, 200))
    draw.text((PLOT_X + 38, PLOT_Y + PLOT_H - 37), ltext, font=FONT_TINY, fill=(126, 206, 240))
    rtext = f"ERUPTED {frame['erupted']:03d} / {n:03d}   LIGHT {frame['light']['intensity']:04.1f}"
    rw = draw.textlength(rtext, font=FONT_TINY)
    draw.rounded_rectangle((PLOT_X + PLOT_W - rw - 38, PLOT_Y + PLOT_H - 43, PLOT_X + PLOT_W - 22, PLOT_Y + PLOT_H - 16),
                           radius=12, fill=(7, 22, 40, 220), outline=(30, 87, 121, 200))
    draw.text((PLOT_X + PLOT_W - rw - 30, PLOT_Y + PLOT_H - 37), rtext, font=FONT_TINY, fill=(93, 159, 197))

    # Footer clock.
    draw.text((1018, 625), f"{t:04.1f} / {total:04.1f} SEC", font=FONT_META_BOLD, fill=(74, 157, 204))
    return image.convert("RGB")


def encode_mp4(frames: list[dict], dump: dict, bounds: tuple, output: Path) -> None:
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise RuntimeError("ffmpeg is required")
    command = [
        ffmpeg, "-loglevel", "error", "-y",
        "-f", "rawvideo", "-vcodec", "rawvideo", "-pix_fmt", "rgb24",
        "-s", f"{WIDTH}x{HEIGHT}", "-r", str(FPS), "-i", "-",
        "-an", "-c:v", "libx264", "-preset", "medium", "-crf", "25",
        "-profile:v", "high", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
        str(output),
    ]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as process:
        assert process.stdin is not None
        for i, frame in enumerate(frames):
            if i % FPS == 0:
                print(f"frost_lance: frame {i:03d}/{len(frames)}")
            pixels = np.asarray(frost_frame(frame, dump, bounds), dtype=np.uint8)
            process.stdin.write(pixels.tobytes())
        process.stdin.close()
        if process.wait() != 0:
            raise RuntimeError("ffmpeg failed while encoding frost_lance.mp4")
    print(f"{output}: {output.stat().st_size:,} bytes")


def encode_gif(mp4_path: Path, gif_path: Path) -> None:
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise RuntimeError("ffmpeg is required")
    palette = gif_path.with_suffix(".palette.png")
    subprocess.run(
        [ffmpeg, "-loglevel", "error", "-y", "-i", str(mp4_path),
         "-vf", "fps=15,scale=640:-1:flags=lanczos,palettegen", str(palette)],
        check=True,
    )
    subprocess.run(
        [ffmpeg, "-loglevel", "error", "-y", "-i", str(mp4_path), "-i", str(palette),
         "-lavfi", "fps=15,scale=640:-1:flags=lanczos [x]; [x][1:v] paletteuse",
         str(gif_path)],
        check=True,
    )
    palette.unlink(missing_ok=True)
    print(f"{gif_path}: {gif_path.stat().st_size:,} bytes")



# ── Storm Lance (E) ──────────────────────────────────────────────────────────

STORM_SUBTITLES = {
    "Travel": "strike front racing — filament bundle cracking behind it",
    "Impact": "bolt holding — restrike guttering at the impact point",
    "Fade": "blow-out — cubic fade, sparks thinning",
    "Done": "pipeline complete — ready for the next cast",
    "Idle": "aim solution locked — cast armed",
}

STORM_CHIPS = {
    "Travel": "TRAVEL — STRIKE FRONT",
    "Impact": "IMPACT — BOLT HOLDING",
    "Fade": "FADE — BLOW-OUT",
    "Done": "DONE",
    "Idle": "AIM — CAST ARMED",
}

ELECTRIC = (127, 180, 255)
HOT = (201, 236, 255)
CORE = (255, 255, 255)


def load_storm_dump(dump_path: Path, regen: bool) -> dict:
    if regen or not dump_path.exists() or dump_path.stat().st_size == 0:
        cargo = shutil.which("cargo")
        if cargo is None:
            raise RuntimeError("cargo is required to regenerate the frame dump")
        print(f"regenerating {dump_path} via dump_storm_lance example…")
        subprocess.run(
            [cargo, "run", "-q", "-p", "animato-fx-elemental",
             "--example", "dump_storm_lance", "--", str(dump_path)],
            cwd=REPO_ROOT,
            check=True,
        )
    with open(dump_path) as f:
        return json.load(f)


def storm_base_scene(kicker, title, subtitle, chip, right_meta):
    image = BACKGROUND.copy()
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((68, 48, 1212, 672), radius=22, fill=CARD, outline=(30, 57, 88, 230), width=2)
    draw.line((96, 166, 1184, 166), fill=(30, 55, 83, 190), width=1)
    draw.text((120, 78), kicker, font=FONT_KICKER, fill=(120, 170, 255))
    draw.text((120, 101), title, font=FONT_TITLE, fill=(231, 244, 255))
    draw.text((120, 140), subtitle, font=FONT_SUBTITLE, fill=MUTED)
    chip_w = max(130, int(draw.textlength(chip, font=FONT_CHIP) + 48))
    chip_x = 1160 - chip_w
    draw.rounded_rectangle((chip_x, 91, 1160, 125), radius=17, fill=(11, 30, 69, 245), outline=(70, 130, 220, 220), width=1)
    draw.ellipse((chip_x + 14, 103, chip_x + 21, 110), fill=ELECTRIC)
    draw.text((chip_x + 31, 99), chip, font=FONT_CHIP, fill=(180, 210, 255))

    draw.rounded_rectangle((PLOT_X - 1, PLOT_Y - 1, PLOT_X + PLOT_W + 1, PLOT_Y + PLOT_H + 1), radius=12, fill=PLOT_BG, outline=(29, 62, 93, 255), width=2)
    for x in range(PLOT_X + 40, PLOT_X + PLOT_W, 80):
        draw.line((x, PLOT_Y + 2, x, PLOT_Y + PLOT_H - 2), fill=GRID, width=1)
    for y in range(PLOT_Y + 40, PLOT_Y + PLOT_H, 56):
        draw.line((PLOT_X + 2, y, PLOT_X + PLOT_W - 2, y), fill=GRID, width=1)
    draw.text((PLOT_X + 22, PLOT_Y + 16), "LIVE PREVIEW  —  FROM RUST DUMP", font=FONT_PLOT, fill=(90, 140, 200))
    draw.text((PLOT_X + PLOT_W - 150, PLOT_Y + 16), right_meta, font=FONT_TINY, fill=(74, 112, 146))
    draw.text((120, 625), "ANIMATO  /  ELEMENTAL LAB", font=FONT_META_BOLD, fill=(74, 116, 151))
    draw.text((370, 625), "STORM LANCE (E)  —  SEEDED, SEEKABLE PIPELINE", font=FONT_META, fill=(69, 92, 122))
    return image, ImageDraw.Draw(image)


def storm_frame(frame: dict, dump: dict, bounds: tuple[float, float, float]) -> Image.Image:
    travel_end, fade_start, total = bounds
    phase = frame["phase"]
    t = frame["t"]
    image, _ = storm_base_scene(
        "ELEMENTAL SANDBOX  /  STORM LANCE (E)",
        "STORM LANCE",
        STORM_SUBTITLES.get(phase, ""),
        STORM_CHIPS.get(phase, phase.upper()),
        "SEED 7  /  SEEKABLE",
    )

    # Soft electric wash along the drawn bolt.
    wash, wash_draw = alpha_layer()
    prog_z = frame["front_pos"][1] if phase == "Travel" else frame["impact_pos"][1]
    _, front_z = world_to_screen(0.0, prog_z)
    wash_draw.rectangle((PLOT_X, PLOT_Y, front_z, PLOT_Y + PLOT_H), fill=(40, 100, 220, 36))
    image.alpha_composite(wash.filter(ImageFilter.GaussianBlur(14)))

    c0 = world_to_screen(0.0, 0.0)
    c1 = world_to_screen(0.0, frame["impact_pos"][1])
    ImageDraw.Draw(image).line((c0, c1), fill=(40, 80, 140, 255), width=1)

    if t < 0.25:
        overlay, odraw = alpha_layer()
        steps = 24
        for i in range(steps):
            s0 = i / steps
            s1 = (i + 0.55) / steps
            if i % 2 == 0:
                p0 = (c0[0] + (c1[0] - c0[0]) * s0, c0[1])
                p1 = (c0[0] + (c1[0] - c0[0]) * min(s1, 1.0), c0[1])
                odraw.line((p0, p1), fill=(160, 200, 255, 235), width=2)
        odraw.polygon([(c1[0], c1[1] - 9), (c1[0] + 14, c1[1]), (c1[0], c1[1] + 9)], fill=(160, 200, 255, 235))
        image.alpha_composite(overlay)
        draw = ImageDraw.Draw(image)
        tag = "AIM LOCKED — 12.0 M"
        tw = draw.textlength(tag, font=FONT_TINY)
        draw.rounded_rectangle((c0[0] - 8, c0[1] - 44, c0[0] + tw + 24, c0[1] - 20),
                               radius=9, fill=(7, 18, 40, 230), outline=(90, 150, 255, 220))
        draw.text((c0[0] + 8, c0[1] - 39), tag, font=FONT_TINY, fill=(160, 200, 255))

    if phase == "Impact":
        age_i = t - travel_end
        flash = max(0.0, 0.5 - age_i * 1.4)
        if flash > 0.0:
            veil, _ = alpha_layer()
            ImageDraw.Draw(veil).rectangle(
                (PLOT_X, PLOT_Y, PLOT_X + PLOT_W, PLOT_Y + PLOT_H),
                fill=(200, 230, 255, int(flash * 255)))
            image.alpha_composite(veil)
        ip = world_to_screen(frame["impact_pos"][0], frame["impact_pos"][1])
        rings, rdraw = alpha_layer()
        for k in range(3):
            rk = age_i * 160.0 - k * 28.0
            if rk > 6.0:
                rdraw.ellipse((ip[0] - rk, ip[1] - rk * 0.55, ip[0] + rk, ip[1] + rk * 0.55),
                              outline=(160, 210, 255, max(0, int(210 - age_i * 180 - k * 40))), width=2)
        image.alpha_composite(rings.filter(ImageFilter.GaussianBlur(2)))

    # Filament polylines — outer strands first, spine last.
    glow, _ = alpha_layer()
    glow_draw = ImageDraw.Draw(glow)
    draw = ImageDraw.Draw(image)
    fade = max(0.0, min(1.0, frame.get("fade", 1.0)))
    strands = sorted(frame["strands"], key=lambda s: -s["radial"])
    for s in strands:
        nodes = [n for n in s["nodes"] if n["d"] > 0.02]
        if len(nodes) < 2:
            continue
        pts = []
        for n in nodes:
            # Map world (x,z) to screen; lift y a little in plot-y for height read.
            sx, sy = world_to_screen(n["x"], n["z"])
            sy -= n["y"] * 14.0
            pts.append((sx, sy))
        branch = s.get("branch", 1.0)
        flash = s.get("flash", 1.0)
        alpha = int((90 + 140 * flash) * fade * branch)
        col = lerp3(ELECTRIC, CORE, 0.35 + 0.5 * (1.0 - s["radial"]))
        width = max(1, int(2 + (1.0 - s["radial"]) * 2))
        if len(pts) >= 2:
            glow_draw.line(pts, fill=(*col, max(30, alpha // 2)), width=width + 3)
            draw.line(pts, fill=(*col, alpha), width=width)
            # Hot core on the spine.
            if s["radial"] < 0.05:
                draw.line(pts, fill=(*HOT, min(255, alpha + 40)), width=1)
    image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(6)))
    draw = ImageDraw.Draw(image)

    # Strike front blade.
    fx = world_to_screen(0.0, frame["front_pos"][1])[0]
    if phase in ("Travel", "Impact"):
        front, _ = alpha_layer()
        fdraw = ImageDraw.Draw(front)
        fdraw.line((fx, PLOT_Y + 4, fx, PLOT_Y + PLOT_H - 4), fill=(180, 220, 255, 235), width=2)
        image.alpha_composite(front.filter(ImageFilter.GaussianBlur(4)))
        draw.line((fx, PLOT_Y + 4, fx, PLOT_Y + PLOT_H - 4), fill=(240, 248, 255, 255), width=1)
        label = "STRIKE FRONT"
        tw = draw.textlength(label, font=FONT_TINY)
        lx = fx + 14 if fx + 14 + tw + 12 <= PLOT_X + PLOT_W - 8 else fx - tw - 26
        ly = PLOT_Y + 40
        draw.rounded_rectangle((lx - 6, ly, lx + tw + 8, ly + 20),
                               radius=8, fill=(7, 18, 40, 225), outline=(90, 150, 255, 200))
        draw.text((lx, ly + 4), label, font=FONT_TINY, fill=(160, 200, 255))

    draw.ellipse((c0[0] - 7, c0[1] - 7, c0[0] + 7, c0[1] + 7),
                 outline=(160, 200, 255, 255), width=2)
    draw.ellipse((c0[0] - 2, c0[1] - 2, c0[0] + 2, c0[1] + 2), fill=(*ELECTRIC, 255))
    draw.text((c0[0] - 20, c0[1] + 12), "CASTER", font=FONT_TINY, fill=(100, 150, 210))
    ic = world_to_screen(frame["impact_pos"][0], frame["impact_pos"][1])
    draw.ellipse((ic[0] - 9, ic[1] - 9, ic[0] + 9, ic[1] + 9), outline=(160, 200, 255, 200), width=1)
    draw.text((ic[0] - 18, ic[1] + 13), "IMPACT", font=FONT_TINY, fill=(100, 150, 210))

    bar_y = PLOT_Y + PLOT_H - 62
    bx0, bx1 = PLOT_X + 40, PLOT_X + PLOT_W - 40
    draw.line((bx0, bar_y, bx1, bar_y), fill=(40, 80, 115, 255), width=2)
    for edge, tag in ((travel_end, "I"), (fade_start, "F")):
        tx = bx0 + (bx1 - bx0) * edge / total
        draw.line((tx, bar_y - 5, tx, bar_y + 5), fill=(90, 170, 215, 255), width=1)
        draw.text((tx - 3, bar_y - 19), tag, font=FONT_TINY, fill=(74, 112, 146))
    knob = bx0 + (bx1 - bx0) * t / total
    draw.ellipse((knob - 5, bar_y - 5, knob + 5, bar_y + 5),
                 fill=(*ELECTRIC, 255), outline=(*HOT, 255), width=1)

    nstrands = len(frame["strands"])
    ltext = f"PHASE {phase.upper()}  —  FRONT {frame['front']:04.1f} / {dump['length']:04.1f} M"
    draw.rounded_rectangle((PLOT_X + 22, PLOT_Y + PLOT_H - 43, PLOT_X + 22 + 330, PLOT_Y + PLOT_H - 16),
                           radius=12, fill=(7, 18, 40, 220), outline=(40, 90, 160, 200))
    draw.text((PLOT_X + 38, PLOT_Y + PLOT_H - 37), ltext, font=FONT_TINY, fill=(150, 200, 255))
    rtext = f"STRANDS {nstrands:02d}   LIGHT {frame['light']['intensity']:04.1f}"
    rw = draw.textlength(rtext, font=FONT_TINY)
    draw.rounded_rectangle((PLOT_X + PLOT_W - rw - 38, PLOT_Y + PLOT_H - 43, PLOT_X + PLOT_W - 22, PLOT_Y + PLOT_H - 16),
                           radius=12, fill=(7, 18, 40, 220), outline=(40, 90, 160, 200))
    draw.text((PLOT_X + PLOT_W - rw - 30, PLOT_Y + PLOT_H - 37), rtext, font=FONT_TINY, fill=(110, 160, 220))

    draw.text((1018, 625), f"{t:04.1f} / {total:04.1f} SEC", font=FONT_META_BOLD, fill=(100, 160, 230))
    return image.convert("RGB")


def encode_storm_mp4(frames, dump, bounds, output: Path) -> None:
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise RuntimeError("ffmpeg is required")
    command = [
        ffmpeg, "-loglevel", "error", "-y",
        "-f", "rawvideo", "-vcodec", "rawvideo", "-pix_fmt", "rgb24",
        "-s", f"{WIDTH}x{HEIGHT}", "-r", str(FPS), "-i", "-",
        "-an", "-c:v", "libx264", "-preset", "medium", "-crf", "25",
        "-profile:v", "high", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
        str(output),
    ]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as process:
        assert process.stdin is not None
        for i, frame in enumerate(frames):
            if i % FPS == 0:
                print(f"storm_lance: frame {i:03d}/{len(frames)}")
            pixels = np.asarray(storm_frame(frame, dump, bounds), dtype=np.uint8)
            process.stdin.write(pixels.tobytes())
        process.stdin.close()
        if process.wait() != 0:
            raise RuntimeError("ffmpeg failed while encoding storm_lance.mp4")
    print(f"{output}: {output.stat().st_size:,} bytes")


def render_frost(output: Path, dump_path: Path, regen: bool) -> None:
    dump = load_dump(dump_path, regen)
    if dump_path.resolve() != (output / dump_path.name).resolve():
        shutil.copy(dump_path, output / dump_path.name)
    frames = dump["frames"]
    assert len(frames) >= 60, "dump must cover the full lifecycle"
    assert frames[0]["erupted"] == 0, "first frame must predate the eruption"
    assert any(f["phase"] == "Impact" for f in frames), "dump must reach Impact"
    assert any(f["phase"] == "Fade" for f in frames), "dump must reach Fade"
    assert frames[-1]["phase"] == "Done", "dump must run to Done"
    bounds = phase_boundaries(frames)
    print(f"frost dump: {len(frames)} frames, {len(frames[0]['spikes'])} spikes, "
          f"total {dump['total_duration']:.2f}s "
          f"(travel→{bounds[0]:.2f}s, fade→{bounds[1]:.2f}s)")
    mp4 = output / "frost_lance.mp4"
    encode_mp4(frames, dump, bounds, mp4)
    encode_gif(mp4, output / "frost_lance.gif")


def render_storm(output: Path, dump_path: Path, regen: bool) -> None:
    dump = load_storm_dump(dump_path, regen)
    if dump_path.resolve() != (output / dump_path.name).resolve():
        shutil.copy(dump_path, output / dump_path.name)
    frames = dump["frames"]
    assert len(frames) >= 20, "dump must cover the full lifecycle"
    assert any(f["phase"] == "Impact" for f in frames), "dump must reach Impact"
    assert any(f["phase"] == "Fade" for f in frames), "dump must reach Fade"
    assert frames[-1]["phase"] == "Done", "dump must run to Done"
    bounds = phase_boundaries(frames)
    print(f"storm dump: {len(frames)} frames, {len(frames[0]['strands'])} strands, "
          f"total {dump['total_duration']:.2f}s "
          f"(travel→{bounds[0]:.2f}s, fade→{bounds[1]:.2f}s)")
    mp4 = output / "storm_lance.mp4"
    encode_storm_mp4(frames, dump, bounds, mp4)
    encode_gif(mp4, output / "storm_lance.gif")



# ── Cinder Fall (R) ──────────────────────────────────────────────────────────

CINDER_SUBTITLES = {
    "Travel": "burning rock lofting — charge heat building on the arc",
    "Impact": "detonation — molten fissures racing + debris ballistic spray",
    "Fade": "crater cooling — chunks sinking, cracks fading",
    "Done": "pipeline complete — ready for the next cast",
    "Idle": "aim solution locked — cast armed",
}

CINDER_CHIPS = {
    "Travel": "TRAVEL — BALLISTIC ARC",
    "Impact": "IMPACT — CRATER BURNING",
    "Fade": "FADE — WITHDRAWAL",
    "Done": "DONE",
    "Idle": "AIM — CAST ARMED",
}

EMBER = (255, 138, 60)
HOT_CINDER = (255, 243, 208)
CORE_FIRE = (255, 106, 18)
CHAR = (40, 28, 22)


def load_cinder_dump(dump_path: Path, regen: bool) -> dict:
    if regen or not dump_path.exists() or dump_path.stat().st_size == 0:
        cargo = shutil.which("cargo")
        if cargo is None:
            raise RuntimeError("cargo is required to regenerate the frame dump")
        print(f"regenerating {dump_path} via dump_cinder_fall example…")
        subprocess.run(
            [cargo, "run", "-q", "-p", "animato-fx-elemental",
             "--example", "dump_cinder_fall", "--", str(dump_path)],
            cwd=REPO_ROOT,
            check=True,
        )
    with open(dump_path) as f:
        return json.load(f)


def cinder_base_scene(kicker, title, subtitle, chip, right_meta):
    image = BACKGROUND.copy()
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((68, 48, 1212, 672), radius=22, fill=CARD, outline=(70, 40, 28, 230), width=2)
    draw.line((96, 166, 1184, 166), fill=(80, 45, 30, 190), width=1)
    draw.text((120, 78), kicker, font=FONT_KICKER, fill=(255, 150, 80))
    draw.text((120, 101), title, font=FONT_TITLE, fill=(255, 236, 220))
    draw.text((120, 140), subtitle, font=FONT_SUBTITLE, fill=(166, 120, 95))
    chip_w = max(130, int(draw.textlength(chip, font=FONT_CHIP) + 48))
    chip_x = 1160 - chip_w
    draw.rounded_rectangle((chip_x, 91, 1160, 125), radius=17, fill=(55, 22, 12, 245), outline=(200, 100, 50, 220), width=1)
    draw.ellipse((chip_x + 14, 103, chip_x + 21, 110), fill=EMBER)
    draw.text((chip_x + 31, 99), chip, font=FONT_CHIP, fill=(255, 200, 150))

    draw.rounded_rectangle((PLOT_X - 1, PLOT_Y - 1, PLOT_X + PLOT_W + 1, PLOT_Y + PLOT_H + 1), radius=12,
                           fill=(18, 10, 8, 242), outline=(90, 45, 30, 255), width=2)
    for x in range(PLOT_X + 40, PLOT_X + PLOT_W, 80):
        draw.line((x, PLOT_Y + 2, x, PLOT_Y + PLOT_H - 2), fill=(50, 28, 20, 115), width=1)
    for y in range(PLOT_Y + 40, PLOT_Y + PLOT_H, 56):
        draw.line((PLOT_X + 2, y, PLOT_X + PLOT_W - 2, y), fill=(50, 28, 20, 115), width=1)
    draw.text((PLOT_X + 22, PLOT_Y + 16), "LIVE PREVIEW  —  FROM RUST DUMP", font=FONT_PLOT, fill=(180, 110, 70))
    draw.text((PLOT_X + PLOT_W - 150, PLOT_Y + 16), right_meta, font=FONT_TINY, fill=(140, 90, 60))
    draw.text((120, 625), "ANIMATO  /  ELEMENTAL LAB", font=FONT_META_BOLD, fill=(150, 95, 65))
    draw.text((370, 625), "CINDER FALL (R)  —  SEEDED, SEEKABLE PIPELINE", font=FONT_META, fill=(120, 80, 55))
    return image, ImageDraw.Draw(image)


def cinder_frame(frame: dict, dump: dict, bounds: tuple[float, float, float]) -> Image.Image:
    travel_end, fade_start, total = bounds
    phase = frame["phase"]
    t = frame["t"]
    image, _ = cinder_base_scene(
        "ELEMENTAL SANDBOX  /  CINDER FALL (R)",
        "CINDER FALL",
        CINDER_SUBTITLES.get(phase, ""),
        CINDER_CHIPS.get(phase, phase.upper()),
        "SEED 7  /  SEEKABLE",
    )

    # Ember wash along the travelled arc.
    wash, wash_draw = alpha_layer()
    prog_z = frame["front_pos"][1] if phase == "Travel" else frame["impact_pos"][1]
    _, front_z = world_to_screen(0.0, prog_z)
    wash_draw.rectangle((PLOT_X, PLOT_Y, front_z, PLOT_Y + PLOT_H), fill=(180, 60, 20, 40))
    image.alpha_composite(wash.filter(ImageFilter.GaussianBlur(14)))

    c0 = world_to_screen(0.0, 0.0)
    c1 = world_to_screen(0.0, frame["impact_pos"][1])
    ImageDraw.Draw(image).line((c0, c1), fill=(90, 50, 30, 255), width=1)

    if t < 0.35:
        overlay, odraw = alpha_layer()
        steps = 24
        for i in range(steps):
            s0 = i / steps
            s1 = (i + 0.55) / steps
            if i % 2 == 0:
                p0 = (c0[0] + (c1[0] - c0[0]) * s0, c0[1])
                p1 = (c0[0] + (c1[0] - c0[0]) * min(s1, 1.0), c0[1])
                odraw.line((p0, p1), fill=(255, 170, 100, 235), width=2)
        odraw.polygon([(c1[0], c1[1] - 9), (c1[0] + 14, c1[1]), (c1[0], c1[1] + 9)], fill=(255, 170, 100, 235))
        image.alpha_composite(overlay)
        draw = ImageDraw.Draw(image)
        tag = "AIM LOCKED — 12.0 M"
        tw = draw.textlength(tag, font=FONT_TINY)
        draw.rounded_rectangle((c0[0] - 8, c0[1] - 44, c0[0] + tw + 24, c0[1] - 20),
                               radius=9, fill=(40, 16, 8, 230), outline=(255, 140, 70, 220))
        draw.text((c0[0] + 8, c0[1] - 39), tag, font=FONT_TINY, fill=(255, 190, 130))

    if phase == "Impact":
        age_i = t - travel_end
        flash = max(0.0, 0.55 - age_i * 1.1)
        if flash > 0.0:
            veil, _ = alpha_layer()
            ImageDraw.Draw(veil).rectangle(
                (PLOT_X, PLOT_Y, PLOT_X + PLOT_W, PLOT_Y + PLOT_H),
                fill=(255, 200, 120, int(flash * 255)))
            image.alpha_composite(veil)
        ip = world_to_screen(frame["impact_pos"][0], frame["impact_pos"][1])
        rings, rdraw = alpha_layer()
        for k in range(3):
            rk = age_i * 140.0 - k * 26.0
            if rk > 6.0:
                rdraw.ellipse((ip[0] - rk, ip[1] - rk * 0.55, ip[0] + rk, ip[1] + rk * 0.55),
                              outline=(255, 160, 80, max(0, int(210 - age_i * 160 - k * 40))), width=2)
        image.alpha_composite(rings.filter(ImageFilter.GaussianBlur(2)))

    glow, _ = alpha_layer()
    glow_draw = ImageDraw.Draw(glow)
    draw = ImageDraw.Draw(image)

    # Fissure polylines (floor cracks) — under the rock/chunks.
    for f in frame.get("fissures", []):
        nodes = [n for n in f["nodes"] if n.get("g", 1) > 0.05]
        if len(nodes) < 2:
            continue
        pts = [world_to_screen(n["x"], n["z"]) for n in nodes]
        if len(pts) < 2:
            continue
        rank = f.get("rank", 0.0)
        alpha = int(160 if rank == 0 else 110)
        col = lerp3(CORE_FIRE, HOT_CINDER, 0.35 if rank == 0 else 0.1)
        w = max(1, int(2 + (1.0 - rank) * 2))
        glow_draw.line(pts, fill=(*col, max(40, alpha // 2)), width=w + 3)
        draw.line(pts, fill=(*col, alpha), width=w)

    # Debris chunks.
    for ch in frame.get("chunks", []):
        sx, sy = world_to_screen(ch["x"], ch["z"])
        sy -= ch["y"] * 12.0
        rr = max(2.0, ch["r"] * 14.0)
        heat = clamp(ch.get("heat", 0.0), 0.0, 1.0)
        col = lerp3(CHAR, EMBER, heat)
        draw.ellipse((sx - rr, sy - rr * 0.7, sx + rr, sy + rr * 0.7), fill=(*col, 220))
        if heat > 0.2:
            glow_draw.ellipse((sx - rr * 1.6, sy - rr * 1.2, sx + rr * 1.6, sy + rr * 1.2),
                              fill=(*EMBER, int(70 * heat)))

    # Rock on the arc.
    rock = frame.get("rock", {})
    if rock.get("visible"):
        sx, sy = world_to_screen(rock["x"], rock["z"])
        sy -= rock["y"] * 14.0
        rr = max(4.0, rock["r"] * 16.0)
        charge = clamp(rock.get("charge", 0.0), 0.0, 1.0)
        col = lerp3((90, 80, 70), EMBER, 0.35 + 0.65 * charge)
        glow_draw.ellipse((sx - rr * 2.2, sy - rr * 2.2, sx + rr * 2.2, sy + rr * 2.2),
                          fill=(*EMBER, int(50 + 80 * charge)))
        draw.ellipse((sx - rr, sy - rr, sx + rr, sy + rr), fill=(*col, 255))
        # Hot core
        cr = rr * 0.35
        draw.ellipse((sx - cr, sy - cr, sx + cr, sy + cr),
                     fill=(*lerp3(col, HOT_CINDER, charge), 255))
        # Arc trail ghost behind the rock
        trail, tdraw = alpha_layer()
        for k in range(8):
            s = max(0.0, frame["progress"] - k * 0.03)
            # approximate from rock position only for presentation
            px = rock["x"]
            pz = rock["z"] - k * 0.35
            py = rock["y"] + k * 0.08
            tx, ty = world_to_screen(px, pz)
            ty -= py * 14.0
            tdraw.ellipse((tx - 6, ty - 6, tx + 6, ty + 6),
                          fill=(*EMBER, max(0, 90 - k * 10)))
        image.alpha_composite(trail.filter(ImageFilter.GaussianBlur(5)))

    image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(7)))
    draw = ImageDraw.Draw(image)

    # Markers
    draw.ellipse((c0[0] - 7, c0[1] - 7, c0[0] + 7, c0[1] + 7),
                 outline=(255, 180, 120, 255), width=2)
    draw.ellipse((c0[0] - 2, c0[1] - 2, c0[0] + 2, c0[1] + 2), fill=(*EMBER, 255))
    draw.text((c0[0] - 20, c0[1] + 12), "CASTER", font=FONT_TINY, fill=(180, 110, 70))
    ic = world_to_screen(frame["impact_pos"][0], frame["impact_pos"][1])
    draw.ellipse((ic[0] - 9, ic[1] - 9, ic[0] + 9, ic[1] + 9), outline=(255, 160, 90, 200), width=1)
    draw.text((ic[0] - 18, ic[1] + 13), "IMPACT", font=FONT_TINY, fill=(180, 110, 70))

    bar_y = PLOT_Y + PLOT_H - 62
    bx0, bx1 = PLOT_X + 40, PLOT_X + PLOT_W - 40
    draw.line((bx0, bar_y, bx1, bar_y), fill=(90, 50, 30, 255), width=2)
    for edge, tag in ((travel_end, "I"), (fade_start, "F")):
        tx = bx0 + (bx1 - bx0) * edge / total
        draw.line((tx, bar_y - 5, tx, bar_y + 5), fill=(220, 130, 70, 255), width=1)
        draw.text((tx - 3, bar_y - 19), tag, font=FONT_TINY, fill=(160, 100, 60))
    knob = bx0 + (bx1 - bx0) * t / total
    draw.ellipse((knob - 5, bar_y - 5, knob + 5, bar_y + 5),
                 fill=(*EMBER, 255), outline=(*HOT_CINDER, 255), width=1)

    nchunks = len(frame.get("chunks", []))
    nfiss = len(frame.get("fissures", []))
    ltext = f"PHASE {phase.upper()}  —  FRONT {frame['front']:04.1f} / {dump['length']:04.1f} M"
    draw.rounded_rectangle((PLOT_X + 22, PLOT_Y + PLOT_H - 43, PLOT_X + 22 + 330, PLOT_Y + PLOT_H - 16),
                           radius=12, fill=(40, 16, 8, 220), outline=(140, 70, 35, 200))
    draw.text((PLOT_X + 38, PLOT_Y + PLOT_H - 37), ltext, font=FONT_TINY, fill=(255, 190, 130))
    rtext = f"CHUNKS {nchunks:02d}  FISSURES {nfiss:02d}  LIGHT {frame['light']['intensity']:04.1f}"
    rw = draw.textlength(rtext, font=FONT_TINY)
    draw.rounded_rectangle((PLOT_X + PLOT_W - rw - 38, PLOT_Y + PLOT_H - 43, PLOT_X + PLOT_W - 22, PLOT_Y + PLOT_H - 16),
                           radius=12, fill=(40, 16, 8, 220), outline=(140, 70, 35, 200))
    draw.text((PLOT_X + PLOT_W - rw - 30, PLOT_Y + PLOT_H - 37), rtext, font=FONT_TINY, fill=(200, 140, 90))

    draw.text((1018, 625), f"{t:04.1f} / {total:04.1f} SEC", font=FONT_META_BOLD, fill=(220, 140, 80))
    return image.convert("RGB")


def encode_cinder_mp4(frames, dump, bounds, output: Path) -> None:
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise RuntimeError("ffmpeg is required")
    command = [
        ffmpeg, "-loglevel", "error", "-y",
        "-f", "rawvideo", "-vcodec", "rawvideo", "-pix_fmt", "rgb24",
        "-s", f"{WIDTH}x{HEIGHT}", "-r", str(FPS), "-i", "-",
        "-an", "-c:v", "libx264", "-preset", "medium", "-crf", "25",
        "-profile:v", "high", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
        str(output),
    ]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as process:
        assert process.stdin is not None
        for i, frame in enumerate(frames):
            if i % FPS == 0:
                print(f"cinder_fall: frame {i:03d}/{len(frames)}")
            pixels = np.asarray(cinder_frame(frame, dump, bounds), dtype=np.uint8)
            process.stdin.write(pixels.tobytes())
        process.stdin.close()
        if process.wait() != 0:
            raise RuntimeError("ffmpeg failed while encoding cinder_fall.mp4")
    print(f"{output}: {output.stat().st_size:,} bytes")


def render_cinder(output: Path, dump_path: Path, regen: bool) -> None:
    dump = load_cinder_dump(dump_path, regen)
    if dump_path.resolve() != (output / dump_path.name).resolve():
        shutil.copy(dump_path, output / dump_path.name)
    frames = dump["frames"]
    assert len(frames) >= 40, "dump must cover the full lifecycle"
    assert any(f["phase"] == "Impact" for f in frames), "dump must reach Impact"
    assert any(f["phase"] == "Fade" for f in frames), "dump must reach Fade"
    assert frames[-1]["phase"] == "Done", "dump must run to Done"
    bounds = phase_boundaries(frames)
    print(f"cinder dump: {len(frames)} frames, "
          f"total {dump['total_duration']:.2f}s "
          f"(travel→{bounds[0]:.2f}s, fade→{bounds[1]:.2f}s)")
    mp4 = output / "cinder_fall.mp4"
    encode_cinder_mp4(frames, dump, bounds, mp4)
    encode_gif(mp4, output / "cinder_fall.gif")



# ── Nova Beam (F) ────────────────────────────────────────────────────────────

NOVA_SUBTITLES = {
    "Charge": "charge orb winding up — front held at the hands",
    "Travel": "leading edge racing — column boring down the aim line",
    "Impact": "sustain burn — shock discs racing the standing beam",
    "Fade": "collapse — width snaps to a thread, then blinks out",
    "Done": "pipeline complete — ready for the next cast",
    "Idle": "aim solution locked — cast armed",
}

NOVA_CHIPS = {
    "Charge": "CHARGE — ORB WIND-UP",
    "Travel": "TRAVEL — BEAM FRONT",
    "Impact": "IMPACT — SUSTAIN BURN",
    "Fade": "FADE — COLLAPSE",
    "Done": "DONE",
    "Idle": "AIM — CAST ARMED",
}

NOVA_CYAN = (127, 240, 255)
NOVA_CORE = (255, 255, 255)
NOVA_GOLD = (255, 220, 140)
NOVA_SHEATH = (62, 198, 255)


def load_nova_dump(dump_path: Path, regen: bool) -> dict:
    if regen or not dump_path.exists() or dump_path.stat().st_size == 0:
        cargo = shutil.which("cargo")
        if cargo is None:
            raise RuntimeError("cargo is required to regenerate the frame dump")
        print(f"regenerating {dump_path} via dump_nova_beam example…")
        subprocess.run(
            [cargo, "run", "-q", "-p", "animato-fx-elemental",
             "--example", "dump_nova_beam", "--", str(dump_path)],
            cwd=REPO_ROOT,
            check=True,
        )
    with open(dump_path) as f:
        return json.load(f)


def nova_phase_boundaries(frames: list[dict]) -> tuple[float, float, float, float]:
    charge_end = travel_end = fade_start = frames[-1]["t"]
    for fr in frames:
        if fr["phase"] == "Travel":
            charge_end = fr["t"]
            break
    for fr in frames:
        if fr["phase"] == "Impact":
            travel_end = fr["t"]
            break
    for fr in frames:
        if fr["phase"] == "Fade":
            fade_start = fr["t"]
            break
    return charge_end, travel_end, fade_start, frames[-1]["t"]


def nova_base_scene(kicker, title, subtitle, chip, right_meta):
    image = BACKGROUND.copy()
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((68, 48, 1212, 672), radius=22, fill=CARD, outline=(30, 70, 100, 230), width=2)
    draw.line((96, 166, 1184, 166), fill=(30, 70, 100, 190), width=1)
    draw.text((120, 78), kicker, font=FONT_KICKER, fill=(100, 220, 240))
    draw.text((120, 101), title, font=FONT_TITLE, fill=(231, 244, 255))
    draw.text((120, 140), subtitle, font=FONT_SUBTITLE, fill=MUTED)
    chip_w = max(130, int(draw.textlength(chip, font=FONT_CHIP) + 48))
    chip_x = 1160 - chip_w
    draw.rounded_rectangle((chip_x, 91, 1160, 125), radius=17, fill=(8, 40, 60, 245), outline=(60, 180, 210, 220), width=1)
    draw.ellipse((chip_x + 14, 103, chip_x + 21, 110), fill=NOVA_CYAN)
    draw.text((chip_x + 31, 99), chip, font=FONT_CHIP, fill=(180, 240, 255))
    draw.rounded_rectangle((PLOT_X - 1, PLOT_Y - 1, PLOT_X + PLOT_W + 1, PLOT_Y + PLOT_H + 1), radius=12, fill=PLOT_BG, outline=(29, 80, 110, 255), width=2)
    for x in range(PLOT_X + 40, PLOT_X + PLOT_W, 80):
        draw.line((x, PLOT_Y + 2, x, PLOT_Y + PLOT_H - 2), fill=GRID, width=1)
    for y in range(PLOT_Y + 40, PLOT_Y + PLOT_H, 56):
        draw.line((PLOT_X + 2, y, PLOT_X + PLOT_W - 2, y), fill=GRID, width=1)
    draw.text((PLOT_X + 22, PLOT_Y + 16), "LIVE PREVIEW  —  FROM RUST DUMP", font=FONT_PLOT, fill=(78, 160, 190))
    draw.text((PLOT_X + PLOT_W - 150, PLOT_Y + 16), right_meta, font=FONT_TINY, fill=(74, 130, 160))
    draw.text((120, 625), "ANIMATO  /  ELEMENTAL LAB", font=FONT_META_BOLD, fill=(74, 130, 160))
    draw.text((370, 625), "NOVA BEAM (F)  —  SEEDED, SEEKABLE PIPELINE", font=FONT_META, fill=(69, 110, 140))
    return image, ImageDraw.Draw(image)


def nova_frame(frame: dict, dump: dict, bounds: tuple) -> Image.Image:
    charge_end, travel_end, fade_start, total = bounds
    phase = frame["phase"]
    t = frame["t"]
    image, _ = nova_base_scene(
        "ELEMENTAL SANDBOX  /  NOVA BEAM (F)",
        "NOVA BEAM",
        NOVA_SUBTITLES.get(phase, ""),
        NOVA_CHIPS.get(phase, phase.upper()),
        "SEED 7  /  SEEKABLE",
    )

    fade = max(0.0, min(1.0, frame.get("fade", 1.0)))
    width_fade = max(0.0, min(1.0, frame.get("width_fade", 1.0)))

    # Soft cyan wash along the drawn column.
    wash, wash_draw = alpha_layer()
    prog_z = frame["front_pos"][1] if phase in ("Travel", "Charge") else frame["impact_pos"][1]
    if phase == "Charge":
        prog_z = frame["hand"][2] if "hand" in frame else 0.7
    _, front_z = world_to_screen(0.0, prog_z if phase != "Charge" else frame.get("hand", [0, 0, 0.7])[2])
    # Use progress along floor z for wash extent.
    wash_z = frame["front_pos"][1] if phase == "Travel" else (
        frame["impact_pos"][1] if phase in ("Impact", "Fade", "Done") else frame["hand"][2]
    )
    _, wash_x = world_to_screen(0.0, wash_z)
    wash_draw.rectangle((PLOT_X, PLOT_Y, max(PLOT_X + 8, wash_x), PLOT_Y + PLOT_H), fill=(40, 160, 200, 30))
    image.alpha_composite(wash.filter(ImageFilter.GaussianBlur(14)))

    c0 = world_to_screen(0.0, 0.0)
    c1 = world_to_screen(0.0, frame["impact_pos"][1])
    ImageDraw.Draw(image).line((c0, c1), fill=(40, 100, 130, 255), width=1)

    if t < 0.25 and phase in ("Charge", "Idle", "Travel"):
        overlay, odraw = alpha_layer()
        steps = 24
        for i in range(steps):
            s0 = i / steps
            s1 = (i + 0.55) / steps
            if i % 2 == 0:
                p0 = (c0[0] + (c1[0] - c0[0]) * s0, c0[1])
                p1 = (c0[0] + (c1[0] - c0[0]) * min(s1, 1.0), c0[1])
                odraw.line((p0, p1), fill=(150, 230, 255, 200), width=2)
        odraw.polygon([(c1[0], c1[1] - 9), (c1[0] + 14, c1[1]), (c1[0], c1[1] + 9)], fill=(150, 230, 255, 200))
        image.alpha_composite(overlay)

    # Charge orb at the hand.
    orb = frame.get("orb", {})
    if orb.get("visible"):
        hx, hy = world_to_screen(orb["x"], orb["z"])
        hy -= orb["y"] * 14.0
        rr = max(4.0, orb["radius"] * 28.0)
        glow, gdraw = alpha_layer()
        gdraw.ellipse((hx - rr * 1.8, hy - rr * 1.8, hx + rr * 1.8, hy + rr * 1.8),
                      fill=(*NOVA_CYAN, int(70 * orb["charge"] * fade)))
        image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(10)))
        draw = ImageDraw.Draw(image)
        draw.ellipse((hx - rr, hy - rr, hx + rr, hy + rr),
                     fill=(*lerp3(NOVA_SHEATH, NOVA_CORE, orb["charge"]), int(180 + 70 * orb["charge"])))
        draw.ellipse((hx - rr * 0.4, hy - rr * 0.4, hx + rr * 0.4, hy + rr * 0.4),
                     fill=(*NOVA_CORE, 255))

    # Parametric tube — halo then sheath then core.
    glow, _ = alpha_layer()
    glow_draw = ImageDraw.Draw(glow)
    draw = ImageDraw.Draw(image)
    tube = [n for n in frame.get("tube", []) if n["d"] > 0.02]
    if len(tube) >= 2:
        pts = []
        radii = []
        for n in tube:
            sx, sy = world_to_screen(n["x"], n["z"])
            sy -= n["y"] * 14.0
            pts.append((sx, sy))
            radii.append(n["r"])
        # Halo
        for i in range(len(pts) - 1):
            w = max(2, int(2 + radii[i] * 10 * width_fade))
            glow_draw.line([pts[i], pts[i + 1]], fill=(*NOVA_SHEATH, int(50 * fade)), width=w + 6)
        # Sheath
        for i in range(len(pts) - 1):
            w = max(1, int(1 + radii[i] * 6 * width_fade))
            draw.line([pts[i], pts[i + 1]], fill=(*NOVA_SHEATH, int(160 * fade)), width=w)
        # Core
        for i in range(len(pts) - 1):
            w = max(1, int(1 + radii[i] * 2.2 * width_fade))
            draw.line([pts[i], pts[i + 1]], fill=(*NOVA_CORE, int(220 * fade)), width=w)
    image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(7)))
    draw = ImageDraw.Draw(image)

    # Shock discs.
    rings_layer, rdraw = alpha_layer()
    for r in frame.get("rings", []):
        if r["a"] < 0.05:
            continue
        sx, sy = world_to_screen(r["x"], r["z"])
        sy -= r["y"] * 14.0
        outer = max(3.0, r["outer"] * 9.0)
        inner = max(1.0, r["inner"] * 9.0)
        alpha = int(200 * r["a"] * fade)
        rdraw.ellipse((sx - outer, sy - outer * 0.45, sx + outer, sy + outer * 0.45),
                      outline=(*NOVA_CYAN, alpha), width=2)
        if outer - inner > 2:
            rdraw.ellipse((sx - inner, sy - inner * 0.45, sx + inner, sy + inner * 0.45),
                          outline=(*NOVA_GOLD, max(0, alpha - 40)), width=1)
    image.alpha_composite(rings_layer.filter(ImageFilter.GaussianBlur(1)))
    draw = ImageDraw.Draw(image)

    if phase == "Impact":
        age_i = t - travel_end
        flash = max(0.0, 0.4 - age_i * 1.2)
        if flash > 0.0:
            veil, _ = alpha_layer()
            ImageDraw.Draw(veil).rectangle(
                (PLOT_X, PLOT_Y, PLOT_X + PLOT_W, PLOT_Y + PLOT_H),
                fill=(200, 245, 255, int(flash * 255)))
            image.alpha_composite(veil)

    # Leading-edge blade while travelling.
    fx = world_to_screen(0.0, frame["front_pos"][1])[0]
    if phase == "Travel":
        front, _ = alpha_layer()
        fdraw = ImageDraw.Draw(front)
        fdraw.line((fx, PLOT_Y + 4, fx, PLOT_Y + PLOT_H - 4), fill=(180, 240, 255, 235), width=2)
        image.alpha_composite(front.filter(ImageFilter.GaussianBlur(4)))
        draw.line((fx, PLOT_Y + 4, fx, PLOT_Y + PLOT_H - 4), fill=(240, 252, 255, 255), width=1)
        label = "BEAM FRONT"
        tw = draw.textlength(label, font=FONT_TINY)
        lx = fx + 14 if fx + 14 + tw + 12 <= PLOT_X + PLOT_W - 8 else fx - tw - 26
        ly = PLOT_Y + 40
        draw.rounded_rectangle((lx - 6, ly, lx + tw + 8, ly + 20),
                               radius=8, fill=(7, 28, 40, 225), outline=(80, 200, 230, 200))
        draw.text((lx, ly + 4), label, font=FONT_TINY, fill=(160, 230, 255))

    draw.ellipse((c0[0] - 7, c0[1] - 7, c0[0] + 7, c0[1] + 7),
                 outline=(160, 230, 255, 255), width=2)
    draw.ellipse((c0[0] - 2, c0[1] - 2, c0[0] + 2, c0[1] + 2), fill=(*NOVA_CYAN, 255))
    draw.text((c0[0] - 20, c0[1] + 12), "CASTER", font=FONT_TINY, fill=(100, 170, 200))
    ic = world_to_screen(frame["impact_pos"][0], frame["impact_pos"][1])
    draw.ellipse((ic[0] - 9, ic[1] - 9, ic[0] + 9, ic[1] + 9), outline=(160, 230, 255, 200), width=1)
    draw.text((ic[0] - 18, ic[1] + 13), "IMPACT", font=FONT_TINY, fill=(100, 170, 200))

    bar_y = PLOT_Y + PLOT_H - 62
    bx0, bx1 = PLOT_X + 40, PLOT_X + PLOT_W - 40
    draw.line((bx0, bar_y, bx1, bar_y), fill=(40, 100, 130, 255), width=2)
    for edge, tag in ((charge_end, "C"), (travel_end, "I"), (fade_start, "F")):
        tx = bx0 + (bx1 - bx0) * edge / total
        draw.line((tx, bar_y - 5, tx, bar_y + 5), fill=(90, 190, 220, 255), width=1)
        draw.text((tx - 3, bar_y - 19), tag, font=FONT_TINY, fill=(74, 140, 170))
    knob = bx0 + (bx1 - bx0) * t / total
    draw.ellipse((knob - 5, bar_y - 5, knob + 5, bar_y + 5),
                 fill=(*NOVA_CYAN, 255), outline=(*NOVA_GOLD, 255), width=1)

    nrings = len(frame.get("rings", []))
    ltext = f"PHASE {phase.upper()}  —  FRONT {frame['front']:04.1f} / {dump['length']:04.1f} M"
    draw.rounded_rectangle((PLOT_X + 22, PLOT_Y + PLOT_H - 43, PLOT_X + 22 + 340, PLOT_Y + PLOT_H - 16),
                           radius=12, fill=(7, 28, 40, 220), outline=(40, 130, 160, 200))
    draw.text((PLOT_X + 38, PLOT_Y + PLOT_H - 37), ltext, font=FONT_TINY, fill=(150, 230, 255))
    rtext = f"RINGS {nrings:02d}  CHARGE {frame.get('charge', 0):0.2f}  LIGHT {frame['light']['intensity']:04.1f}"
    rw = draw.textlength(rtext, font=FONT_TINY)
    draw.rounded_rectangle((PLOT_X + PLOT_W - rw - 38, PLOT_Y + PLOT_H - 43, PLOT_X + PLOT_W - 22, PLOT_Y + PLOT_H - 16),
                           radius=12, fill=(7, 28, 40, 220), outline=(40, 130, 160, 200))
    draw.text((PLOT_X + PLOT_W - rw - 30, PLOT_Y + PLOT_H - 37), rtext, font=FONT_TINY, fill=(120, 190, 220))

    draw.text((1018, 625), f"{t:04.1f} / {total:04.1f} SEC", font=FONT_META_BOLD, fill=(100, 200, 230))
    return image.convert("RGB")


def encode_nova_mp4(frames, dump, bounds, output: Path) -> None:
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise RuntimeError("ffmpeg is required")
    command = [
        ffmpeg, "-loglevel", "error", "-y",
        "-f", "rawvideo", "-vcodec", "rawvideo", "-pix_fmt", "rgb24",
        "-s", f"{WIDTH}x{HEIGHT}", "-r", str(FPS), "-i", "-",
        "-an", "-c:v", "libx264", "-preset", "medium", "-crf", "25",
        "-profile:v", "high", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
        str(output),
    ]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as process:
        assert process.stdin is not None
        for i, frame in enumerate(frames):
            if i % FPS == 0:
                print(f"nova_beam: frame {i:03d}/{len(frames)}")
            pixels = np.asarray(nova_frame(frame, dump, bounds), dtype=np.uint8)
            process.stdin.write(pixels.tobytes())
        process.stdin.close()
        if process.wait() != 0:
            raise RuntimeError("ffmpeg failed while encoding nova_beam.mp4")
    print(f"{output}: {output.stat().st_size:,} bytes")


def render_nova(output: Path, dump_path: Path, regen: bool) -> None:
    dump = load_nova_dump(dump_path, regen)
    if dump_path.resolve() != (output / dump_path.name).resolve():
        shutil.copy(dump_path, output / dump_path.name)
    frames = dump["frames"]
    assert len(frames) >= 40, "dump must cover the full lifecycle"
    assert any(f["phase"] == "Charge" for f in frames), "dump must include Charge"
    assert any(f["phase"] == "Impact" for f in frames), "dump must reach Impact"
    assert any(f["phase"] == "Fade" for f in frames), "dump must reach Fade"
    assert frames[-1]["phase"] == "Done", "dump must run to Done"
    bounds = nova_phase_boundaries(frames)
    print(f"nova dump: {len(frames)} frames, "
          f"total {dump['total_duration']:.2f}s "
          f"(charge→{bounds[0]:.2f}s, travel→{bounds[1]:.2f}s, fade→{bounds[2]:.2f}s)")
    mp4 = output / "nova_beam.mp4"
    encode_nova_mp4(frames, dump, bounds, mp4)
    encode_gif(mp4, output / "nova_beam.gif")




# ── Voltaic Snare (V) — top-down cage card ───────────────────────────────────

SNARE_SUBTITLES = {
    "Travel": "leash whipping across the floor — planting the trap",
    "Impact": "cage slam — column, tendrils and rim arcs holding the disc",
    "Fade": "collapse — pillar thinning to a thread",
    "Done": "pipeline complete — ready for the next cast",
    "Idle": "zone aim locked — cast armed",
}

SNARE_CHIPS = {
    "Travel": "TRAVEL — LEASH FLIGHT",
    "Impact": "IMPACT — CAGE HOLD",
    "Fade": "FADE — COLLAPSE",
    "Done": "DONE",
    "Idle": "ZONE — CAST ARMED",
}

VIOLET = (169, 139, 255)
VIOLET_HOT = (220, 208, 255)
VIOLET_CORE = (255, 255, 255)


def load_snare_dump(dump_path: Path, regen: bool) -> dict:
    if regen or not dump_path.exists() or dump_path.stat().st_size == 0:
        cargo = shutil.which("cargo")
        if cargo is None:
            raise RuntimeError("cargo is required to regenerate the frame dump")
        print(f"regenerating {dump_path} via dump_voltaic_snare example…")
        subprocess.run(
            [cargo, "run", "-q", "-p", "animato-fx-elemental",
             "--example", "dump_voltaic_snare", "--", str(dump_path)],
            cwd=REPO_ROOT,
            check=True,
        )
    with open(dump_path) as f:
        return json.load(f)


def snare_phase_boundaries(frames: list[dict]) -> tuple[float, float, float]:
    travel_end = fade_start = frames[-1]["t"]
    for fr in frames:
        if fr["phase"] == "Impact":
            travel_end = fr["t"]
            break
    for fr in frames:
        if fr["phase"] == "Fade":
            fade_start = fr["t"]
            break
    return travel_end, fade_start, frames[-1]["t"]


def snare_world_to_screen(x: float, z: float, cx: float, cz: float) -> tuple[float, float]:
    """Top-down: world XZ → plot, centred on the planted cage."""
    half = 8.5
    px = PLOT_X + PLOT_W * 0.5 + (x - cx) * (PLOT_W * 0.5 / half)
    py = PLOT_Y + PLOT_H * 0.5 - (z - cz) * (PLOT_H * 0.5 / half)
    return (px, py)


def snare_base_scene(kicker, title, subtitle, chip, right_meta):
    image = BACKGROUND.copy()
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((68, 48, 1212, 672), radius=22, fill=CARD, outline=(60, 40, 100, 230), width=2)
    draw.line((96, 166, 1184, 166), fill=(60, 40, 100, 190), width=1)
    draw.text((120, 78), kicker, font=FONT_KICKER, fill=(180, 150, 255))
    draw.text((120, 101), title, font=FONT_TITLE, fill=(231, 244, 255))
    draw.text((120, 140), subtitle, font=FONT_SUBTITLE, fill=MUTED)
    chip_w = max(130, int(draw.textlength(chip, font=FONT_CHIP) + 48))
    chip_x = 1160 - chip_w
    draw.rounded_rectangle((chip_x, 91, 1160, 125), radius=17, fill=(30, 14, 60, 245), outline=(140, 100, 220, 220), width=1)
    draw.ellipse((chip_x + 14, 103, chip_x + 21, 110), fill=VIOLET)
    draw.text((chip_x + 31, 99), chip, font=FONT_CHIP, fill=(210, 190, 255))
    draw.rounded_rectangle(
        (PLOT_X - 1, PLOT_Y - 1, PLOT_X + PLOT_W + 1, PLOT_Y + PLOT_H + 1),
        radius=12, fill=PLOT_BG, outline=(70, 40, 120, 255), width=2,
    )
    for x in range(PLOT_X + 40, PLOT_X + PLOT_W, 80):
        draw.line((x, PLOT_Y + 2, x, PLOT_Y + PLOT_H - 2), fill=GRID, width=1)
    for y in range(PLOT_Y + 40, PLOT_Y + PLOT_H, 56):
        draw.line((PLOT_X + 2, y, PLOT_X + PLOT_W - 2, y), fill=GRID, width=1)
    draw.text((PLOT_X + 22, PLOT_Y + 16), "LIVE PREVIEW  —  FROM RUST DUMP  —  TOP-DOWN", font=FONT_PLOT, fill=(140, 110, 200))
    draw.text((PLOT_X + PLOT_W - 150, PLOT_Y + 16), right_meta, font=FONT_TINY, fill=(110, 90, 160))
    draw.text((120, 625), "ANIMATO  /  ELEMENTAL LAB", font=FONT_META_BOLD, fill=(110, 90, 160))
    draw.text((370, 625), "VOLTAIC SNARE (V)  —  SEEDED, SEEKABLE ZONE CAST", font=FONT_META, fill=(90, 70, 140))
    return image


def snare_frame(frame: dict, dump: dict, bounds: tuple) -> Image.Image:
    travel_end, fade_start, total = bounds
    phase = frame["phase"]
    t = frame["t"]
    image = snare_base_scene(
        "ELEMENTAL SANDBOX  /  VOLTAIC SNARE (V)",
        "VOLTAIC SNARE",
        SNARE_SUBTITLES.get(phase, ""),
        SNARE_CHIPS.get(phase, phase.upper()),
        "SEED 7  /  SEEKABLE",
    )
    cx, cz = frame["center"]
    zone_r = float(dump["zone_radius"])
    fade = max(0.0, min(1.0, float(frame.get("fade", 1.0))))
    open_amt = max(0.0, float(frame.get("open", 0.0)))
    origin = dump.get("origin", [0.0, 0.0])

    # Soft violet wash over the disc once the cage is open.
    if open_amt > 0.02:
        wash, wdraw = alpha_layer()
        r_px = zone_r * min(1.0, open_amt) * (PLOT_W * 0.5 / 8.5)
        c = snare_world_to_screen(cx, cz, cx, cz)
        alpha = int(40 * fade * min(1.0, open_amt))
        wdraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            fill=(90, 50, 180, alpha),
        )
        wdraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            outline=(200, 180, 255, int(180 * fade * min(1.0, open_amt))),
            width=3,
        )
        image.alpha_composite(wash.filter(ImageFilter.GaussianBlur(6)))

    # Aim cue early in travel.
    if t < 0.35 and phase in ("Travel", "Idle"):
        overlay, odraw = alpha_layer()
        o = snare_world_to_screen(origin[0], origin[1], cx, cz)
        c = snare_world_to_screen(cx, cz, cx, cz)
        steps = 20
        for i in range(steps):
            if i % 2 == 0:
                s0 = i / steps
                s1 = min(1.0, (i + 0.55) / steps)
                p0 = (o[0] + (c[0] - o[0]) * s0, o[1] + (c[1] - o[1]) * s0)
                p1 = (o[0] + (c[0] - o[0]) * s1, o[1] + (c[1] - o[1]) * s1)
                odraw.line((p0, p1), fill=(180, 160, 255, 200), width=2)
        r_px = zone_r * (PLOT_W * 0.5 / 8.5)
        odraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            outline=(180, 160, 255, 160),
            width=2,
        )
        image.alpha_composite(overlay)

    # Filament polylines.
    glow, gdraw = alpha_layer()
    core, cdraw = alpha_layer()
    role_alpha = {"Leash": 220, "Column": 255, "Tendril": 200, "Rim": 230}
    for fil in frame.get("filaments", []):
        nodes = fil.get("nodes") or []
        if len(nodes) < 2:
            continue
        flash = max(0.15, min(1.0, float(fil.get("flash", 1.0))))
        dim = max(0.2, min(1.0, float(fil.get("dim", 1.0))))
        a = int(role_alpha.get(fil.get("role", "Column"), 200) * fade * flash * dim)
        pts = [snare_world_to_screen(n["x"], n["z"], cx, cz) for n in nodes]
        w_hint = max((float(n.get("w", 0.03)) for n in nodes), default=0.03)
        width = max(1, min(5, int(w_hint * 80)))
        gdraw.line(pts, fill=(120, 70, 220, max(30, a // 2)), width=width + 3)
        color = VIOLET_CORE if fil.get("role") in ("Column", "Leash") else VIOLET_HOT
        cdraw.line(pts, fill=(*color, a), width=max(1, width))

    image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(2)))
    image.alpha_composite(core)

    draw = ImageDraw.Draw(image)
    o = snare_world_to_screen(origin[0], origin[1], cx, cz)
    c = snare_world_to_screen(cx, cz, cx, cz)
    draw.ellipse((o[0] - 5, o[1] - 5, o[0] + 5, o[1] + 5), fill=(180, 160, 255, 220))
    if open_amt > 0.05:
        draw.ellipse((c[0] - 4, c[1] - 4, c[0] + 4, c[1] + 4), fill=(255, 255, 255, int(200 * fade)))

    if phase == "Impact":
        age_i = t - travel_end
        flash = max(0.0, 0.45 - age_i * 1.5)
        if flash > 0.0:
            veil, _ = alpha_layer()
            ImageDraw.Draw(veil).rectangle(
                (PLOT_X, PLOT_Y, PLOT_X + PLOT_W, PLOT_Y + PLOT_H),
                fill=(200, 180, 255, int(flash * 200)),
            )
            image.alpha_composite(veil)

    progress = 0.0 if total <= 0 else clamp(t / total, 0.0, 1.0)
    bar_x0, bar_y0, bar_x1 = 120, 600, 1160
    draw.rounded_rectangle((bar_x0, bar_y0, bar_x1, bar_y0 + 8), radius=4, fill=(30, 20, 50, 220))
    fill_x = bar_x0 + (bar_x1 - bar_x0) * progress
    draw.rounded_rectangle((bar_x0, bar_y0, fill_x, bar_y0 + 8), radius=4, fill=(*VIOLET, 230))
    for boundary, _tag in ((travel_end, "SNAP"), (fade_start, "FADE")):
        if total > 0:
            bx = bar_x0 + (bar_x1 - bar_x0) * (boundary / total)
            draw.line((bx, bar_y0 - 2, bx, bar_y0 + 10), fill=(160, 140, 220, 200), width=1)

    meta = (
        f"t={t:5.2f}s   open={open_amt:4.2f}   "
        f"climb={float(frame.get('climb', 0)):4.2f}   "
        f"filaments={len(frame.get('filaments', []))}"
    )
    draw.text((120, 575), meta, font=FONT_TINY, fill=(140, 120, 190))
    return image.convert("RGB")


def encode_snare_mp4(frames, dump, bounds, output: Path) -> None:
    command = [
        "ffmpeg", "-y", "-f", "rawvideo", "-vcodec", "rawvideo",
        "-pix_fmt", "rgb24", "-s", f"{WIDTH}x{HEIGHT}", "-r", str(FPS),
        "-i", "-", "-an", "-c:v", "libx264", "-preset", "medium", "-crf", "18",
        "-profile:v", "high", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
        str(output),
    ]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as process:
        assert process.stdin is not None
        for i, frame in enumerate(frames):
            if i % FPS == 0:
                print(f"voltaic_snare: frame {i:03d}/{len(frames)}")
            pixels = np.asarray(snare_frame(frame, dump, bounds), dtype=np.uint8)
            process.stdin.write(pixels.tobytes())
        process.stdin.close()
        if process.wait() != 0:
            raise RuntimeError("ffmpeg failed while encoding voltaic_snare.mp4")
    print(f"{output}: {output.stat().st_size:,} bytes")


def render_snare(output: Path, dump_path: Path, regen: bool) -> None:
    dump = load_snare_dump(dump_path, regen)
    dest = output / dump_path.name
    if dump_path.resolve() != dest.resolve():
        shutil.copy(dump_path, dest)
    frames = dump["frames"]
    assert len(frames) >= 40, "dump must cover the full lifecycle"
    assert any(f["phase"] == "Travel" for f in frames), "dump must include Travel"
    assert any(f["phase"] == "Impact" for f in frames), "dump must reach Impact"
    assert any(f["phase"] == "Fade" for f in frames), "dump must reach Fade"
    assert frames[-1]["phase"] == "Done", "dump must run to Done"
    bounds = snare_phase_boundaries(frames)
    print(
        f"snare dump: {len(frames)} frames, "
        f"total {dump['total_duration']:.2f}s "
        f"(travel→{bounds[0]:.2f}s, fade→{bounds[1]:.2f}s)"
    )
    mp4 = output / "voltaic_snare.mp4"
    encode_snare_mp4(frames, dump, bounds, mp4)
    encode_gif(mp4, output / "voltaic_snare.gif")




# ── Glacial Crown (X) — top-down ice crown/ring card ─────────────────────────

GLACIAL_SUBTITLES = {
    "Travel": "freeze front racing across the floor — planting the circle",
    "Impact": "crown bloom — ring of blades + skirt banking against it",
    "Fade": "shatter — plates crumbling, sheet thawing inward",
    "Done": "pipeline complete — ready for the next cast",
    "Idle": "zone aim locked — cast armed",
}

GLACIAL_CHIPS = {
    "Travel": "TRAVEL — FREEZE FRONT",
    "Impact": "IMPACT — CROWN HOLD",
    "Fade": "FADE — SHATTER",
    "Done": "DONE",
    "Idle": "ZONE — CAST ARMED",
}

ICE_HOT = (200, 248, 255)
ICE_CORE = (255, 255, 255)
ICE_EDGE = (120, 210, 240)
ICE_SKIRT = (160, 220, 245)


def load_glacial_dump(dump_path: Path, regen: bool) -> dict:
    if regen or not dump_path.exists() or dump_path.stat().st_size == 0:
        cargo = shutil.which("cargo")
        if cargo is None:
            raise RuntimeError("cargo is required to regenerate the frame dump")
        print(f"regenerating {dump_path} via dump_glacial_crown example…")
        subprocess.run(
            [cargo, "run", "-q", "-p", "animato-fx-elemental",
             "--example", "dump_glacial_crown", "--", str(dump_path)],
            cwd=REPO_ROOT,
            check=True,
        )
    with open(dump_path) as f:
        return json.load(f)


def glacial_phase_boundaries(frames: list[dict]) -> tuple[float, float, float]:
    travel_end = fade_start = frames[-1]["t"]
    for fr in frames:
        if fr["phase"] == "Impact":
            travel_end = fr["t"]
            break
    for fr in frames:
        if fr["phase"] == "Fade":
            fade_start = fr["t"]
            break
    return travel_end, fade_start, frames[-1]["t"]


def glacial_world_to_screen(x: float, z: float, cx: float, cz: float) -> tuple[float, float]:
    """Top-down: world XZ → plot, centred on the planted crown."""
    half = 8.5
    px = PLOT_X + PLOT_W * 0.5 + (x - cx) * (PLOT_W * 0.5 / half)
    py = PLOT_Y + PLOT_H * 0.5 - (z - cz) * (PLOT_H * 0.5 / half)
    return (px, py)


def glacial_base_scene(kicker, title, subtitle, chip, right_meta):
    image = BACKGROUND.copy()
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((68, 48, 1212, 672), radius=22, fill=CARD, outline=(30, 70, 100, 230), width=2)
    draw.line((96, 166, 1184, 166), fill=(30, 70, 100, 190), width=1)
    draw.text((120, 78), kicker, font=FONT_KICKER, fill=(120, 210, 240))
    draw.text((120, 101), title, font=FONT_TITLE, fill=(231, 244, 255))
    draw.text((120, 140), subtitle, font=FONT_SUBTITLE, fill=MUTED)
    chip_w = max(130, int(draw.textlength(chip, font=FONT_CHIP) + 48))
    chip_x = 1160 - chip_w
    draw.rounded_rectangle((chip_x, 91, 1160, 125), radius=17, fill=(8, 40, 60, 245), outline=(80, 180, 220, 220), width=1)
    draw.ellipse((chip_x + 14, 103, chip_x + 21, 110), fill=ICE_EDGE)
    draw.text((chip_x + 31, 99), chip, font=FONT_CHIP, fill=(180, 240, 255))
    draw.rounded_rectangle(
        (PLOT_X - 1, PLOT_Y - 1, PLOT_X + PLOT_W + 1, PLOT_Y + PLOT_H + 1),
        radius=12, fill=PLOT_BG, outline=(40, 100, 140, 255), width=2,
    )
    for x in range(PLOT_X + 40, PLOT_X + PLOT_W, 80):
        draw.line((x, PLOT_Y + 2, x, PLOT_Y + PLOT_H - 2), fill=GRID, width=1)
    for y in range(PLOT_Y + 40, PLOT_Y + PLOT_H, 56):
        draw.line((PLOT_X + 2, y, PLOT_X + PLOT_W - 2, y), fill=GRID, width=1)
    draw.text((PLOT_X + 22, PLOT_Y + 16), "LIVE PREVIEW  —  FROM RUST DUMP  —  TOP-DOWN", font=FONT_PLOT, fill=(90, 170, 210))
    draw.text((PLOT_X + PLOT_W - 150, PLOT_Y + 16), right_meta, font=FONT_TINY, fill=(80, 140, 180))
    draw.text((120, 625), "ANIMATO  /  ELEMENTAL LAB", font=FONT_META_BOLD, fill=(80, 140, 180))
    draw.text((370, 625), "GLACIAL CROWN (X)  —  SEEDED, SEEKABLE ZONE CAST", font=FONT_META, fill=(70, 120, 160))
    return image


def glacial_frame(frame: dict, dump: dict, bounds: tuple) -> Image.Image:
    travel_end, fade_start, total = bounds
    phase = frame["phase"]
    t = frame["t"]
    image = glacial_base_scene(
        "ELEMENTAL SANDBOX  /  GLACIAL CROWN (X)",
        "GLACIAL CROWN",
        GLACIAL_SUBTITLES.get(phase, ""),
        GLACIAL_CHIPS.get(phase, phase.upper()),
        "SEED 7  /  SEEKABLE",
    )
    cx, cz = frame["center"]
    zone_r = float(dump["zone_radius"])
    fade = max(0.0, min(1.0, float(frame.get("fade", 1.0))))
    open_amt = max(0.0, float(frame.get("open", 0.0)))
    origin = dump.get("origin", [0.0, 0.0])

    # Soft ice wash / sheet once the crown is open.
    field = frame.get("field")
    if field and open_amt > 0.02:
        wash, wdraw = alpha_layer()
        r_px = float(field["r"]) * float(field.get("freeze", open_amt)) * (PLOT_W * 0.5 / 8.5)
        c = glacial_world_to_screen(cx, cz, cx, cz)
        alpha = int(50 * fade * min(1.0, float(field.get("fade", open_amt))))
        wdraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            fill=(60, 150, 200, alpha),
        )
        # Boundary band
        band = max(4, int(0.4 * (PLOT_W * 0.5 / 8.5)))
        wdraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            outline=(220, 250, 255, int(200 * fade * min(1.0, open_amt))),
            width=band,
        )
        image.alpha_composite(wash.filter(ImageFilter.GaussianBlur(4)))

    # Veil ring outline.
    veil = frame.get("veil")
    if veil and float(veil.get("opacity", 0)) > 0.01:
        overlay, odraw = alpha_layer()
        c = glacial_world_to_screen(cx, cz, cx, cz)
        r_px = float(veil["r"]) * (PLOT_W * 0.5 / 8.5)
        a = int(90 * fade * float(veil["opacity"]))
        odraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            outline=(180, 235, 255, a),
            width=3,
        )
        image.alpha_composite(overlay.filter(ImageFilter.GaussianBlur(2)))

    # Aim cue early in travel.
    if t < 0.35 and phase in ("Travel", "Idle"):
        overlay, odraw = alpha_layer()
        o = glacial_world_to_screen(origin[0], origin[1], cx, cz)
        c = glacial_world_to_screen(cx, cz, cx, cz)
        steps = 20
        for i in range(steps):
            if i % 2 == 0:
                s0 = i / steps
                s1 = min(1.0, (i + 0.55) / steps)
                p0 = (o[0] + (c[0] - o[0]) * s0, o[1] + (c[1] - o[1]) * s0)
                p1 = (o[0] + (c[0] - o[0]) * s1, o[1] + (c[1] - o[1]) * s1)
                odraw.line((p0, p1), fill=(160, 220, 255, 200), width=2)
        r_px = zone_r * (PLOT_W * 0.5 / 8.5)
        odraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            outline=(160, 220, 255, 160),
            width=2,
        )
        image.alpha_composite(overlay)

    # Traveling front tip.
    if phase == "Travel":
        tip = frame.get("front_pos", [0.0, 0.0])
        tp = glacial_world_to_screen(tip[0], tip[1], cx, cz)
        glow, gdraw = alpha_layer()
        gdraw.ellipse((tp[0] - 10, tp[1] - 10, tp[0] + 10, tp[1] + 10), fill=(180, 240, 255, 120))
        image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(4)))
        ImageDraw.Draw(image).ellipse((tp[0] - 4, tp[1] - 4, tp[0] + 4, tp[1] + 4), fill=ICE_CORE)

    # Shards as radial blades (top-down: lean → outward length).
    glow, gdraw = alpha_layer()
    core, cdraw = alpha_layer()
    for sh in frame.get("shards", []):
        e = max(0.0, min(1.2, float(sh.get("e", 0.0))))
        if e <= 0.0:
            continue
        birth = max(0.0, float(sh.get("b", 0.0)))
        shatter = max(0.0, min(1.0, float(sh.get("s", 0.0))))
        visibility = max(0.05, (1.0 - shatter * 0.85) * fade)
        sx, sz = float(sh["x"]), float(sh["z"])
        p = glacial_world_to_screen(sx, sz, cx, cz)
        # Outward lean direction from centre.
        dx, dz = sx - cx, sz - cz
        dist = math.hypot(dx, dz) or 1.0
        ux, uz = dx / dist, dz / dist
        lean = float(sh.get("lean", 0.3))
        h = float(sh.get("h", 1.0)) * e
        # Project lean as radial length in plot space.
        reach = (h * math.sin(lean) + float(sh.get("r", 0.2))) * (PLOT_W * 0.5 / 8.5) * 1.2
        tip = (
            p[0] + ux * reach,
            p[1] - uz * reach,
        )
        role = sh.get("role", "Ring")
        if role == "Ring":
            color = ICE_CORE
            width = max(1, min(4, int(float(sh.get("r", 0.3)) * 10)))
            a = int(255 * visibility)
        elif role == "Skirt":
            color = ICE_SKIRT
            width = max(1, min(3, int(float(sh.get("r", 0.3)) * 8)))
            a = int(200 * visibility)
        else:
            color = ICE_HOT
            width = max(1, min(4, int(float(sh.get("r", 0.3)) * 10)))
            a = int(230 * visibility)
        if birth > 0.2:
            a = min(255, a + int(birth * 40))
        gdraw.line((p, tip), fill=(80, 180, 220, max(20, a // 2)), width=width + 2)
        cdraw.line((p, tip), fill=(*color, a), width=width)
        # Foot seat
        rr = max(1, int(float(sh.get("r", 0.2)) * (PLOT_W * 0.5 / 8.5) * 0.35))
        cdraw.ellipse((p[0] - rr, p[1] - rr, p[0] + rr, p[1] + rr), fill=(*color, a))

    image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(1)))
    image.alpha_composite(core)

    draw = ImageDraw.Draw(image)
    o = glacial_world_to_screen(origin[0], origin[1], cx, cz)
    c = glacial_world_to_screen(cx, cz, cx, cz)
    draw.ellipse((o[0] - 5, o[1] - 5, o[0] + 5, o[1] + 5), fill=(160, 220, 255, 220))
    if open_amt > 0.05:
        draw.ellipse((c[0] - 4, c[1] - 4, c[0] + 4, c[1] + 4), fill=(255, 255, 255, int(200 * fade)))

    if phase == "Impact":
        age_i = t - travel_end
        flash = max(0.0, 0.4 - age_i * 1.2)
        if flash > 0.0:
            veil_flash, _ = alpha_layer()
            ImageDraw.Draw(veil_flash).rectangle(
                (PLOT_X, PLOT_Y, PLOT_X + PLOT_W, PLOT_Y + PLOT_H),
                fill=(200, 245, 255, int(flash * 180)),
            )
            image.alpha_composite(veil_flash)

    progress = 0.0 if total <= 0 else clamp(t / total, 0.0, 1.0)
    bar_x0, bar_y0, bar_x1 = 120, 600, 1160
    draw.rounded_rectangle((bar_x0, bar_y0, bar_x1, bar_y0 + 8), radius=4, fill=(20, 40, 60, 220))
    fill_x = bar_x0 + (bar_x1 - bar_x0) * progress
    draw.rounded_rectangle((bar_x0, bar_y0, fill_x, bar_y0 + 8), radius=4, fill=(*ICE_EDGE, 230))
    for boundary, _tag in ((travel_end, "BLOOM"), (fade_start, "FADE")):
        if total > 0:
            bx = bar_x0 + (bar_x1 - bar_x0) * (boundary / total)
            draw.line((bx, bar_y0 - 2, bx, bar_y0 + 10), fill=(140, 200, 230, 200), width=1)

    meta = (
        f"t={t:5.2f}s   open={open_amt:4.2f}   "
        f"thaw={float(frame.get('thaw', 0)):4.2f}   "
        f"shards={len(frame.get('shards', []))}"
    )
    draw.text((120, 575), meta, font=FONT_TINY, fill=(120, 180, 210))
    return image.convert("RGB")


def encode_glacial_mp4(frames, dump, bounds, output: Path) -> None:
    command = [
        "ffmpeg", "-y", "-f", "rawvideo", "-vcodec", "rawvideo",
        "-pix_fmt", "rgb24", "-s", f"{WIDTH}x{HEIGHT}", "-r", str(FPS),
        "-i", "-", "-an", "-c:v", "libx264", "-preset", "medium", "-crf", "18",
        "-profile:v", "high", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
        str(output),
    ]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as process:
        assert process.stdin is not None
        for i, frame in enumerate(frames):
            if i % FPS == 0:
                print(f"glacial_crown: frame {i:03d}/{len(frames)}")
            pixels = np.asarray(glacial_frame(frame, dump, bounds), dtype=np.uint8)
            process.stdin.write(pixels.tobytes())
        process.stdin.close()
        if process.wait() != 0:
            raise RuntimeError("ffmpeg failed while encoding glacial_crown.mp4")
    print(f"{output}: {output.stat().st_size:,} bytes")


def render_glacial(output: Path, dump_path: Path, regen: bool) -> None:
    dump = load_glacial_dump(dump_path, regen)
    dest = output / dump_path.name
    if dump_path.resolve() != dest.resolve():
        shutil.copy(dump_path, dest)
    frames = dump["frames"]
    assert len(frames) >= 40, "dump must cover the full lifecycle"
    assert any(f["phase"] == "Travel" for f in frames), "dump must include Travel"
    assert any(f["phase"] == "Impact" for f in frames), "dump must reach Impact"
    assert any(f["phase"] == "Fade" for f in frames), "dump must reach Fade"
    assert frames[-1]["phase"] == "Done", "dump must run to Done"
    bounds = glacial_phase_boundaries(frames)
    print(
        f"glacial dump: {len(frames)} frames, "
        f"total {dump['total_duration']:.2f}s "
        f"(travel→{bounds[0]:.2f}s, fade→{bounds[1]:.2f}s)"
    )
    mp4 = output / "glacial_crown.mp4"
    encode_glacial_mp4(frames, dump, bounds, mp4)
    encode_gif(mp4, output / "glacial_crown.gif")




# ── Pyre Crown (Q, Ext) — top-down fire crown/ring card ───────────────────────

PYRE_SUBTITLES = {
    "Travel": "fire front racing across the floor — planting the circle",
    "Impact": "crown catch — ring of flame blades + skirt banking against it",
    "Fade": "burn-out — tips to ash, crater cooling inward",
    "Done": "pipeline complete — ready for the next cast",
    "Idle": "zone aim locked — cast armed",
}

PYRE_CHIPS = {
    "Travel": "TRAVEL — FIRE FRONT",
    "Impact": "IMPACT — BLAZE",
    "Fade": "FADE — BURN OUT",
    "Done": "DONE",
    "Idle": "ZONE — CAST ARMED",
}

FIRE_HOT = (255, 240, 189)
FIRE_CORE = (255, 125, 26)
FIRE_EDGE = (255, 106, 30)
FIRE_SKIRT = (192, 24, 7)
FIRE_ASH = (74, 64, 56)


def load_pyre_dump(dump_path: Path, regen: bool) -> dict:
    if regen or not dump_path.exists() or dump_path.stat().st_size == 0:
        cargo = shutil.which("cargo")
        if cargo is None:
            raise RuntimeError("cargo is required to regenerate the frame dump")
        print(f"regenerating {dump_path} via dump_pyre_crown example…")
        subprocess.run(
            [cargo, "run", "-q", "-p", "animato-fx-elemental",
             "--example", "dump_pyre_crown", "--", str(dump_path)],
            cwd=REPO_ROOT,
            check=True,
        )
    with open(dump_path) as f:
        return json.load(f)


def pyre_phase_boundaries(frames: list[dict]) -> tuple[float, float, float]:
    travel_end = fade_start = frames[-1]["t"]
    for fr in frames:
        if fr["phase"] == "Impact":
            travel_end = fr["t"]
            break
    for fr in frames:
        if fr["phase"] == "Fade":
            fade_start = fr["t"]
            break
    return travel_end, fade_start, frames[-1]["t"]


def pyre_world_to_screen(x: float, z: float, cx: float, cz: float) -> tuple[float, float]:
    half = 8.5
    px = PLOT_X + PLOT_W * 0.5 + (x - cx) * (PLOT_W * 0.5 / half)
    py = PLOT_Y + PLOT_H * 0.5 - (z - cz) * (PLOT_H * 0.5 / half)
    return (px, py)


def pyre_base_scene(kicker, title, subtitle, chip, right_meta):
    image = BACKGROUND.copy()
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((68, 48, 1212, 672), radius=22, fill=CARD, outline=(100, 40, 20, 230), width=2)
    draw.line((96, 166, 1184, 166), fill=(100, 40, 20, 190), width=1)
    draw.text((120, 78), kicker, font=FONT_KICKER, fill=(255, 154, 60))
    draw.text((120, 101), title, font=FONT_TITLE, fill=(255, 236, 210))
    draw.text((120, 140), subtitle, font=FONT_SUBTITLE, fill=MUTED)
    chip_w = max(130, int(draw.textlength(chip, font=FONT_CHIP) + 48))
    chip_x = 1160 - chip_w
    draw.rounded_rectangle((chip_x, 91, 1160, 125), radius=17, fill=(40, 12, 6, 245), outline=(255, 106, 30, 220), width=1)
    draw.ellipse((chip_x + 14, 103, chip_x + 21, 110), fill=FIRE_EDGE)
    draw.text((chip_x + 31, 99), chip, font=FONT_CHIP, fill=(255, 200, 140))
    draw.rounded_rectangle(
        (PLOT_X - 1, PLOT_Y - 1, PLOT_X + PLOT_W + 1, PLOT_Y + PLOT_H + 1),
        radius=12, fill=PLOT_BG, outline=(120, 50, 20, 255), width=2,
    )
    for x in range(PLOT_X + 40, PLOT_X + PLOT_W, 80):
        draw.line((x, PLOT_Y + 2, x, PLOT_Y + PLOT_H - 2), fill=GRID, width=1)
    for y in range(PLOT_Y + 40, PLOT_Y + PLOT_H, 56):
        draw.line((PLOT_X + 2, y, PLOT_X + PLOT_W - 2, y), fill=GRID, width=1)
    draw.text((PLOT_X + 22, PLOT_Y + 16), "LIVE PREVIEW  —  FROM RUST DUMP  —  TOP-DOWN", font=FONT_PLOT, fill=(210, 120, 60))
    draw.text((PLOT_X + PLOT_W - 150, PLOT_Y + 16), right_meta, font=FONT_TINY, fill=(180, 100, 50))
    draw.text((120, 625), "ANIMATO  /  ELEMENTAL LAB", font=FONT_META_BOLD, fill=(180, 100, 50))
    draw.text((370, 625), "PYRE CROWN (Q, EXT)  —  SEEDED, SEEKABLE ZONE CAST", font=FONT_META, fill=(160, 90, 40))
    return image


def pyre_frame(frame: dict, dump: dict, bounds: tuple) -> Image.Image:
    travel_end, fade_start, total = bounds
    phase = frame["phase"]
    t = frame["t"]
    image = pyre_base_scene(
        "EXT SANDBOX  /  PYRE CROWN (Q)",
        "PYRE CROWN",
        PYRE_SUBTITLES.get(phase, ""),
        PYRE_CHIPS.get(phase, phase.upper()),
        "SEED 7  /  SEEKABLE",
    )
    cx, cz = frame["center"]
    zone_r = float(dump["zone_radius"])
    fade = max(0.0, min(1.0, float(frame.get("fade", 1.0))))
    open_amt = max(0.0, float(frame.get("open", 0.0)))
    origin = dump.get("origin", [0.0, 0.0])

    field = frame.get("field")
    if field and open_amt > 0.02:
        wash, wdraw = alpha_layer()
        r_px = float(field["r"]) * float(field.get("burn", open_amt)) * (PLOT_W * 0.5 / 8.5)
        c = pyre_world_to_screen(cx, cz, cx, cz)
        alpha = int(70 * fade * min(1.0, float(field.get("fade", open_amt))))
        wdraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            fill=(255, 67, 20, alpha),
        )
        band = max(4, int(0.56 * (PLOT_W * 0.5 / 8.5)))
        wdraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            outline=(255, 189, 128, int(220 * fade * min(1.0, open_amt))),
            width=band,
        )
        image.alpha_composite(wash.filter(ImageFilter.GaussianBlur(4)))

    veil = frame.get("veil")
    if veil and float(veil.get("opacity", 0)) > 0.01:
        overlay, odraw = alpha_layer()
        c = pyre_world_to_screen(cx, cz, cx, cz)
        r_px = float(veil["r"]) * (PLOT_W * 0.5 / 8.5)
        a = int(110 * fade * float(veil["opacity"]))
        odraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            outline=(255, 159, 128, a),
            width=4,
        )
        image.alpha_composite(overlay.filter(ImageFilter.GaussianBlur(2)))

    if t < 0.35 and phase in ("Travel", "Idle"):
        overlay, odraw = alpha_layer()
        o = pyre_world_to_screen(origin[0], origin[1], cx, cz)
        c = pyre_world_to_screen(cx, cz, cx, cz)
        steps = 20
        for i in range(steps):
            if i % 2 == 0:
                s0 = i / steps
                s1 = min(1.0, (i + 0.55) / steps)
                p0 = (o[0] + (c[0] - o[0]) * s0, o[1] + (c[1] - o[1]) * s0)
                p1 = (o[0] + (c[0] - o[0]) * s1, o[1] + (c[1] - o[1]) * s1)
                odraw.line((p0, p1), fill=(255, 154, 60, 200), width=2)
        r_px = zone_r * (PLOT_W * 0.5 / 8.5)
        odraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            outline=(255, 154, 60, 160),
            width=2,
        )
        image.alpha_composite(overlay)

    if phase == "Travel":
        tip = frame.get("front_pos", [0.0, 0.0])
        tp = pyre_world_to_screen(tip[0], tip[1], cx, cz)
        glow, gdraw = alpha_layer()
        gdraw.ellipse((tp[0] - 10, tp[1] - 10, tp[0] + 10, tp[1] + 10), fill=(255, 140, 40, 140))
        image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(4)))
        ImageDraw.Draw(image).ellipse((tp[0] - 4, tp[1] - 4, tp[0] + 4, tp[1] + 4), fill=FIRE_HOT)

    glow, gdraw = alpha_layer()
    core, cdraw = alpha_layer()
    for sh in frame.get("blades", []):
        e = max(0.0, min(1.2, float(sh.get("e", 0.0))))
        if e <= 0.0:
            continue
        birth = max(0.0, float(sh.get("b", 0.0)))
        char = max(0.0, min(1.0, float(sh.get("c", 0.0))))
        visibility = max(0.05, (1.0 - char * 0.9) * fade)
        sx, sz = float(sh["x"]), float(sh["z"])
        p = pyre_world_to_screen(sx, sz, cx, cz)
        dx, dz = sx - cx, sz - cz
        dist = math.hypot(dx, dz) or 1.0
        ux, uz = dx / dist, dz / dist
        lean = float(sh.get("lean", -0.3))
        h = float(sh.get("h", 1.0)) * min(1.0, e)
        # Inward lean (negative) still draws a visible radial segment.
        reach = (h * abs(math.sin(lean)) + float(sh.get("r", 0.2))) * (PLOT_W * 0.5 / 8.5) * 1.2
        tip = (p[0] + ux * reach * (1.0 if lean >= 0 else -0.35), p[1] - uz * reach * (1.0 if lean >= 0 else -0.35))
        if lean < 0:
            tip = (p[0] - ux * reach * 0.55, p[1] + uz * reach * 0.55)
        role = sh.get("role", "Ring")
        if role == "Ring":
            color = FIRE_CORE if char < 0.5 else FIRE_ASH
            width = max(1, min(4, int(float(sh.get("r", 0.3)) * 10)))
            a = int(255 * visibility)
        elif role == "Skirt":
            color = FIRE_SKIRT if char < 0.5 else FIRE_ASH
            width = max(1, min(3, int(float(sh.get("r", 0.3)) * 8)))
            a = int(200 * visibility)
        else:
            color = FIRE_HOT
            width = max(1, min(4, int(float(sh.get("r", 0.3)) * 10)))
            a = int(230 * visibility)
        if birth > 0.2:
            a = min(255, a + int(birth * 50))
        gdraw.line((p, tip), fill=(255, 80, 20, max(20, a // 2)), width=width + 2)
        cdraw.line((p, tip), fill=(*color, a), width=width)
        rr = max(1, int(float(sh.get("r", 0.2)) * (PLOT_W * 0.5 / 8.5) * 0.35))
        cdraw.ellipse((p[0] - rr, p[1] - rr, p[0] + rr, p[1] + rr), fill=(*color, a))

    image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(1)))
    image.alpha_composite(core)

    draw = ImageDraw.Draw(image)
    o = pyre_world_to_screen(origin[0], origin[1], cx, cz)
    c = pyre_world_to_screen(cx, cz, cx, cz)
    draw.ellipse((o[0] - 5, o[1] - 5, o[0] + 5, o[1] + 5), fill=(255, 154, 60, 220))
    if open_amt > 0.05:
        draw.ellipse((c[0] - 4, c[1] - 4, c[0] + 4, c[1] + 4), fill=(255, 240, 189, int(200 * fade)))

    if phase == "Impact":
        age_i = t - travel_end
        flash = max(0.0, 0.4 - age_i * 1.2)
        if flash > 0.0:
            veil_flash, _ = alpha_layer()
            ImageDraw.Draw(veil_flash).rectangle(
                (PLOT_X, PLOT_Y, PLOT_X + PLOT_W, PLOT_Y + PLOT_H),
                fill=(255, 160, 60, int(flash * 180)),
            )
            image.alpha_composite(veil_flash)

    progress = 0.0 if total <= 0 else clamp(t / total, 0.0, 1.0)
    bar_x0, bar_y0, bar_x1 = 120, 600, 1160
    draw.rounded_rectangle((bar_x0, bar_y0, bar_x1, bar_y0 + 8), radius=4, fill=(40, 16, 8, 220))
    fill_x = bar_x0 + (bar_x1 - bar_x0) * progress
    draw.rounded_rectangle((bar_x0, bar_y0, fill_x, bar_y0 + 8), radius=4, fill=(*FIRE_EDGE, 230))
    for boundary, _tag in ((travel_end, "BLOOM"), (fade_start, "FADE")):
        if total > 0:
            bx = bar_x0 + (bar_x1 - bar_x0) * (boundary / total)
            draw.line((bx, bar_y0 - 2, bx, bar_y0 + 10), fill=(255, 180, 100, 200), width=1)

    meta = (
        f"t={t:5.2f}s   open={open_amt:4.2f}   "
        f"cool={float(frame.get('cool', 0)):4.2f}   "
        f"blades={len(frame.get('blades', []))}"
    )
    draw.text((120, 575), meta, font=FONT_TINY, fill=(210, 140, 80))
    return image.convert("RGB")


def encode_pyre_mp4(frames, dump, bounds, output: Path) -> None:
    command = [
        "ffmpeg", "-y", "-f", "rawvideo", "-vcodec", "rawvideo",
        "-pix_fmt", "rgb24", "-s", f"{WIDTH}x{HEIGHT}", "-r", str(FPS),
        "-i", "-", "-an", "-c:v", "libx264", "-preset", "medium", "-crf", "18",
        "-profile:v", "high", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
        str(output),
    ]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as process:
        assert process.stdin is not None
        for i, frame in enumerate(frames):
            if i % FPS == 0:
                print(f"pyre_crown: frame {i:03d}/{len(frames)}")
            pixels = np.asarray(pyre_frame(frame, dump, bounds), dtype=np.uint8)
            process.stdin.write(pixels.tobytes())
        process.stdin.close()
        if process.wait() != 0:
            raise RuntimeError("ffmpeg failed while encoding pyre_crown.mp4")
    print(f"{output}: {output.stat().st_size:,} bytes")


def render_pyre(output: Path, dump_path: Path, regen: bool) -> None:
    dump = load_pyre_dump(dump_path, regen)
    frames = dump["frames"]
    bounds = pyre_phase_boundaries(frames)
    print(
        f"pyre dump: {len(frames)} frames, "
        f"total={dump['total_duration']:.2f}s, zone_r={dump['zone_radius']:.2f}m"
    )
    mp4 = output / "pyre_crown.mp4"
    encode_pyre_mp4(frames, dump, bounds, mp4)
    encode_gif(mp4, output / "pyre_crown.gif")



# ── Kraken Crown (E, Ext) — top-down abyss rift / tentacle card ───────────────

KRAKEN_SUBTITLES = {
    "Travel": "wet surge racing across the floor — planting the circle",
    "Impact": "rift tear — arms haul out and hammer the middle",
    "Fade": "withdrawal — arms pulled back, water closing over",
    "Done": "pipeline complete — ready for the next cast",
    "Idle": "zone aim locked — cast armed",
}

KRAKEN_CHIPS = {
    "Travel": "TRAVEL — WET SURGE",
    "Impact": "IMPACT — HAMMERING",
    "Fade": "FADE — WITHDRAW",
    "Done": "DONE",
    "Idle": "ZONE — CAST ARMED",
}

TEAL_HOT = (232, 255, 248)
TEAL_CORE = (63, 224, 200)
TEAL_EDGE = (46, 214, 200)
TEAL_INK = (14, 40, 48)
TEAL_WHIP = (127, 184, 198)


def load_kraken_dump(dump_path: Path, regen: bool) -> dict:
    if regen or not dump_path.exists() or dump_path.stat().st_size == 0:
        cargo = shutil.which("cargo")
        if cargo is None:
            raise RuntimeError("cargo is required to regenerate the frame dump")
        print(f"regenerating {dump_path} via dump_kraken_crown example…")
        subprocess.run(
            [cargo, "run", "-q", "-p", "animato-fx-elemental",
             "--example", "dump_kraken_crown", "--", str(dump_path)],
            cwd=REPO_ROOT,
            check=True,
        )
    with open(dump_path) as f:
        return json.load(f)


def kraken_phase_boundaries(frames: list[dict]) -> tuple[float, float, float]:
    travel_end = fade_start = frames[-1]["t"]
    for fr in frames:
        if fr["phase"] == "Impact":
            travel_end = fr["t"]
            break
    for fr in frames:
        if fr["phase"] == "Fade":
            fade_start = fr["t"]
            break
    return travel_end, fade_start, frames[-1]["t"]


def kraken_world_to_screen(x: float, z: float, cx: float, cz: float) -> tuple[float, float]:
    half = 9.0
    px = PLOT_X + PLOT_W * 0.5 + (x - cx) * (PLOT_W * 0.5 / half)
    py = PLOT_Y + PLOT_H * 0.5 - (z - cz) * (PLOT_H * 0.5 / half)
    return (px, py)


def kraken_base_scene(kicker, title, subtitle, chip, right_meta):
    image = BACKGROUND.copy()
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((68, 48, 1212, 672), radius=22, fill=CARD, outline=(20, 80, 90, 230), width=2)
    draw.line((96, 166, 1184, 166), fill=(20, 80, 90, 190), width=1)
    draw.text((120, 78), kicker, font=FONT_KICKER, fill=(94, 230, 220))
    draw.text((120, 101), title, font=FONT_TITLE, fill=(220, 248, 245))
    draw.text((120, 140), subtitle, font=FONT_SUBTITLE, fill=MUTED)
    chip_w = max(130, int(draw.textlength(chip, font=FONT_CHIP) + 48))
    chip_x = 1160 - chip_w
    draw.rounded_rectangle((chip_x, 91, 1160, 125), radius=17, fill=(6, 28, 32, 245), outline=(63, 224, 200, 220), width=1)
    draw.ellipse((chip_x + 14, 103, chip_x + 21, 110), fill=TEAL_EDGE)
    draw.text((chip_x + 31, 99), chip, font=FONT_CHIP, fill=(180, 240, 230))
    draw.rounded_rectangle(
        (PLOT_X - 1, PLOT_Y - 1, PLOT_X + PLOT_W + 1, PLOT_Y + PLOT_H + 1),
        radius=12, fill=PLOT_BG, outline=(30, 100, 110, 255), width=2,
    )
    for x in range(PLOT_X + 40, PLOT_X + PLOT_W, 80):
        draw.line((x, PLOT_Y + 2, x, PLOT_Y + PLOT_H - 2), fill=GRID, width=1)
    for y in range(PLOT_Y + 40, PLOT_Y + PLOT_H, 56):
        draw.line((PLOT_X + 2, y, PLOT_X + PLOT_W - 2, y), fill=GRID, width=1)
    draw.text((PLOT_X + 22, PLOT_Y + 16), "LIVE PREVIEW  —  FROM RUST DUMP  —  TOP-DOWN", font=FONT_PLOT, fill=(60, 170, 170))
    draw.text((PLOT_X + PLOT_W - 150, PLOT_Y + 16), right_meta, font=FONT_TINY, fill=(50, 140, 140))
    draw.text((120, 625), "ANIMATO  /  ELEMENTAL LAB", font=FONT_META_BOLD, fill=(50, 140, 140))
    draw.text((370, 625), "KRAKEN CROWN (E, EXT)  —  SEEDED, SEEKABLE ZONE CAST", font=FONT_META, fill=(40, 120, 120))
    return image


def kraken_frame(frame: dict, dump: dict, bounds: tuple) -> Image.Image:
    travel_end, fade_start, total = bounds
    phase = frame["phase"]
    t = frame["t"]
    image = kraken_base_scene(
        "EXT SANDBOX  /  KRAKEN CROWN (E)",
        "KRAKEN CROWN",
        KRAKEN_SUBTITLES.get(phase, ""),
        KRAKEN_CHIPS.get(phase, phase.upper()),
        "SEED 7  /  SEEKABLE",
    )
    cx, cz = frame["center"]
    zone_r = float(dump["zone_radius"])
    fade = max(0.0, min(1.0, float(frame.get("fade", 1.0))))
    open_amt = max(0.0, float(frame.get("open", 0.0)))
    origin = dump.get("origin", [0.0, 0.0])
    half = 9.0

    field = frame.get("field")
    if field and open_amt > 0.02:
        wash, wdraw = alpha_layer()
        r_px = float(field["r"]) * float(field.get("open", open_amt)) * (PLOT_W * 0.5 / half)
        c = kraken_world_to_screen(cx, cz, cx, cz)
        alpha = int(90 * fade * min(1.0, float(field.get("fade", open_amt))))
        wdraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            fill=(8, 40, 50, alpha),
        )
        band = max(4, int(0.5 * (PLOT_W * 0.5 / half)))
        wdraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            outline=(127, 245, 226, int(220 * fade * min(1.0, open_amt))),
            width=band,
        )
        # throat
        throat = r_px * 0.28
        wdraw.ellipse(
            (c[0] - throat, c[1] - throat, c[0] + throat, c[1] + throat),
            fill=(5, 13, 20, int(180 * fade * open_amt)),
        )
        image.alpha_composite(wash.filter(ImageFilter.GaussianBlur(3)))

    veil = frame.get("veil")
    if veil and float(veil.get("opacity", 0)) > 0.01:
        overlay, odraw = alpha_layer()
        c = kraken_world_to_screen(cx, cz, cx, cz)
        r_px = float(veil["r"]) * (PLOT_W * 0.5 / half)
        a = int(100 * fade * float(veil["opacity"]))
        odraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            outline=(180, 240, 245, a),
            width=5,
        )
        image.alpha_composite(overlay.filter(ImageFilter.GaussianBlur(2)))

    if t < 0.35 and phase in ("Travel", "Idle"):
        overlay, odraw = alpha_layer()
        o = kraken_world_to_screen(origin[0], origin[1], cx, cz)
        c = kraken_world_to_screen(cx, cz, cx, cz)
        steps = 20
        for i in range(steps):
            if i % 2 == 0:
                s0 = i / steps
                s1 = min(1.0, (i + 0.55) / steps)
                p0 = (o[0] + (c[0] - o[0]) * s0, o[1] + (c[1] - o[1]) * s0)
                p1 = (o[0] + (c[0] - o[0]) * s1, o[1] + (c[1] - o[1]) * s1)
                odraw.line((p0, p1), fill=(94, 230, 220, 200), width=2)
        r_px = zone_r * (PLOT_W * 0.5 / half)
        odraw.ellipse(
            (c[0] - r_px, c[1] - r_px, c[0] + r_px, c[1] + r_px),
            outline=(94, 230, 220, 160),
            width=2,
        )
        image.alpha_composite(overlay)

    if phase == "Travel":
        tip = frame.get("front_pos", [0.0, 0.0])
        tp = kraken_world_to_screen(tip[0], tip[1], cx, cz)
        glow, gdraw = alpha_layer()
        gdraw.ellipse((tp[0] - 10, tp[1] - 10, tp[0] + 10, tp[1] + 10), fill=(63, 224, 200, 140))
        image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(4)))
        ImageDraw.Draw(image).ellipse((tp[0] - 4, tp[1] - 4, tp[0] + 4, tp[1] + 4), fill=TEAL_HOT)

    glow, gdraw = alpha_layer()
    core, cdraw = alpha_layer()
    for arm in frame.get("arms", []):
        e = max(0.0, min(1.2, float(arm.get("emerge", 0.0))))
        if e <= 0.02:
            continue
        sx, sz = float(arm["x"]), float(arm["z"])
        tip = arm.get("tip", [sx, 0.0, sz])
        p = kraken_world_to_screen(sx, sz, cx, cz)
        tip_p = kraken_world_to_screen(float(tip[0]), float(tip[2]), cx, cz)
        # Bend toward tip using lean as curvature hint — draw seat→tip arc via mid pull.
        lean = float(arm.get("lean", 0.0))
        mx = (p[0] + tip_p[0]) * 0.5
        my = (p[1] + tip_p[1]) * 0.5
        # Pull mid point outward (negative lean) or inward.
        dx, dy = tip_p[0] - p[0], tip_p[1] - p[1]
        nx, ny = -dy, dx
        nlen = math.hypot(nx, ny) or 1.0
        pull = lean * 12.0
        mid = (mx + (nx / nlen) * pull, my + (ny / nlen) * pull)
        role = arm.get("role", "Arm")
        flash = max(0.0, float(arm.get("flash", 0.0)))
        striking = bool(arm.get("striking", False))
        if role == "Arm":
            color = TEAL_CORE
            width = max(2, min(5, int(float(arm.get("th", 0.3)) * 8)))
            a = int(245 * fade * min(1.0, e))
        else:
            color = TEAL_WHIP
            width = max(1, min(3, int(float(arm.get("th", 0.2)) * 10)))
            a = int(200 * fade * min(1.0, e))
        if flash > 0.15 or striking:
            a = min(255, a + int(flash * 60))
            color = TEAL_HOT
        gdraw.line((p, mid), fill=(20, 80, 90, max(20, a // 2)), width=width + 3)
        gdraw.line((mid, tip_p), fill=(20, 80, 90, max(20, a // 2)), width=width + 2)
        cdraw.line((p, mid), fill=(*color, a), width=width)
        cdraw.line((mid, tip_p), fill=(*color, a), width=max(1, width - 1))
        rr = max(1, int(float(arm.get("th", 0.2)) * (PLOT_W * 0.5 / half) * 0.4))
        cdraw.ellipse((p[0] - rr, p[1] - rr, p[0] + rr, p[1] + rr), fill=(*TEAL_INK, a))
        if flash > 0.2:
            tr = max(2, int(4 + flash * 6))
            gdraw.ellipse(
                (tip_p[0] - tr, tip_p[1] - tr, tip_p[0] + tr, tip_p[1] + tr),
                fill=(232, 255, 248, int(180 * flash)),
            )

    image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(1)))
    image.alpha_composite(core)

    draw = ImageDraw.Draw(image)
    o = kraken_world_to_screen(origin[0], origin[1], cx, cz)
    c = kraken_world_to_screen(cx, cz, cx, cz)
    draw.ellipse((o[0] - 5, o[1] - 5, o[0] + 5, o[1] + 5), fill=(94, 230, 220, 220))
    if open_amt > 0.05:
        draw.ellipse((c[0] - 4, c[1] - 4, c[0] + 4, c[1] + 4), fill=(232, 255, 248, int(200 * fade)))

    if phase == "Impact":
        age_i = t - travel_end
        flash = max(0.0, 0.35 - age_i * 1.0)
        if flash > 0.0:
            veil_flash, _ = alpha_layer()
            ImageDraw.Draw(veil_flash).rectangle(
                (PLOT_X, PLOT_Y, PLOT_X + PLOT_W, PLOT_Y + PLOT_H),
                fill=(94, 230, 220, int(flash * 140)),
            )
            image.alpha_composite(veil_flash)

    progress = 0.0 if total <= 0 else clamp(t / total, 0.0, 1.0)
    bar_x0, bar_y0, bar_x1 = 120, 600, 1160
    draw.rounded_rectangle((bar_x0, bar_y0, bar_x1, bar_y0 + 8), radius=4, fill=(8, 32, 36, 220))
    fill_x = bar_x0 + (bar_x1 - bar_x0) * progress
    draw.rounded_rectangle((bar_x0, bar_y0, fill_x, bar_y0 + 8), radius=4, fill=(*TEAL_EDGE, 230))
    for boundary, _tag in ((travel_end, "TEAR"), (fade_start, "FADE")):
        if total > 0:
            bx = bar_x0 + (bar_x1 - bar_x0) * (boundary / total)
            draw.line((bx, bar_y0 - 2, bx, bar_y0 + 10), fill=(140, 230, 220, 200), width=1)

    meta = (
        f"t={t:5.2f}s   open={open_amt:4.2f}   "
        f"close={float(frame.get('close', 0)):4.2f}   "
        f"arms={len(frame.get('arms', []))}"
    )
    draw.text((120, 575), meta, font=FONT_TINY, fill=(100, 190, 180))
    return image.convert("RGB")


def encode_kraken_mp4(frames, dump, bounds, output: Path) -> None:
    command = [
        "ffmpeg", "-y", "-f", "rawvideo", "-vcodec", "rawvideo",
        "-pix_fmt", "rgb24", "-s", f"{WIDTH}x{HEIGHT}", "-r", str(FPS),
        "-i", "-", "-an", "-c:v", "libx264", "-preset", "medium", "-crf", "18",
        "-profile:v", "high", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
        str(output),
    ]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as process:
        assert process.stdin is not None
        for i, frame in enumerate(frames):
            if i % FPS == 0:
                print(f"kraken_crown: frame {i:03d}/{len(frames)}")
            pixels = np.asarray(kraken_frame(frame, dump, bounds), dtype=np.uint8)
            process.stdin.write(pixels.tobytes())
        process.stdin.close()
        if process.wait() != 0:
            raise RuntimeError("ffmpeg failed while encoding kraken_crown.mp4")
    print(f"{output}: {output.stat().st_size:,} bytes")


def render_kraken(output: Path, dump_path: Path, regen: bool) -> None:
    dump = load_kraken_dump(dump_path, regen)
    frames = dump["frames"]
    bounds = kraken_phase_boundaries(frames)
    print(
        f"kraken dump: {len(frames)} frames, "
        f"total={dump['total_duration']:.2f}s, zone_r={dump['zone_radius']:.2f}m"
    )
    mp4 = output / "kraken_crown.mp4"
    encode_kraken_mp4(frames, dump, bounds, mp4)
    encode_gif(mp4, output / "kraken_crown.gif")



def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ability", choices=("frost", "storm", "cinder", "nova", "snare", "glacial", "pyre", "kraken", "all"), default="kraken",
                        help="which cinematic card to render (default: kraken)")
    parser.add_argument("--output", type=Path, default=HERE)
    parser.add_argument("--dump", type=Path, default=None,
                        help="override dump path (defaults per ability)")
    parser.add_argument("--regen", action="store_true",
                        help="force regeneration of the Rust frame dump")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    abilities = ["frost", "storm", "cinder", "nova", "snare", "glacial", "pyre", "kraken"] if args.ability == "all" else [args.ability]
    for ability in abilities:
        if ability == "frost":
            dump = args.dump or (HERE / "frost_lance_frames.json")
            render_frost(args.output, dump, args.regen)
        elif ability == "storm":
            dump = args.dump or (HERE / "storm_lance_frames.json")
            render_storm(args.output, dump, args.regen)
        elif ability == "cinder":
            dump = args.dump or (HERE / "cinder_fall_frames.json")
            render_cinder(args.output, dump, args.regen)
        elif ability == "nova":
            dump = args.dump or (HERE / "nova_beam_frames.json")
            render_nova(args.output, dump, args.regen)
        elif ability == "snare":
            dump = args.dump or (HERE / "voltaic_snare_frames.json")
            render_snare(args.output, dump, args.regen)
        elif ability == "glacial":
            dump = args.dump or (HERE / "glacial_crown_frames.json")
            render_glacial(args.output, dump, args.regen)
        elif ability == "pyre":
            dump = args.dump or (HERE / "pyre_crown_frames.json")
            render_pyre(args.output, dump, args.regen)
        else:
            dump = args.dump or (HERE / "kraken_crown_frames.json")
            render_kraken(args.output, dump, args.regen)


if __name__ == "__main__":
    main()
