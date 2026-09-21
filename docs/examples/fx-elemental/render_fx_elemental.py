#!/usr/bin/env python3
"""Render a cinematic 1280x720 Frost Lance product-demo clip.

Every spike transform, the fracture-front position, phase transitions and
light state come from the REAL Rust pipeline: the script (re)generates
``frost_lance_frames.json`` via the ``dump_frost_lance`` cargo example
(``animato-fx-elemental``) and only handles presentation — dark glassy card
UI, ice/cyan palette, ffmpeg H.264 + GIF for issue embeds.

The clip reads left-to-right: caster at left, impact at right. The fracture
front races down the line, the spike field erupts behind it (dense and
ankle-high at the caster, wall of blades + impact cluster at the far end),
the impact punches, then the field withdraws.

Usage:
    python3 render_fx_elemental.py [--output DIR] [--regen]
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
X_HALF = 4.2


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
    if regen or not dump_path.exists():
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=HERE)
    parser.add_argument("--dump", type=Path, default=HERE / "frost_lance_frames.json")
    parser.add_argument("--regen", action="store_true",
                        help="force regeneration of the Rust frame dump")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    dump = load_dump(args.dump, args.regen)
    # Copy the dump next to the media when a different output dir is used.
    if args.dump.resolve() != (args.output / args.dump.name).resolve():
        shutil.copy(args.dump, args.output / args.dump.name)
    frames = dump["frames"]
    assert len(frames) >= 60, "dump must cover the full lifecycle"
    assert frames[0]["erupted"] == 0, "first frame must predate the eruption"
    assert any(f["phase"] == "Impact" for f in frames), "dump must reach Impact"
    assert any(f["phase"] == "Fade" for f in frames), "dump must reach Fade"
    assert frames[-1]["phase"] == "Done", "dump must run to Done"
    bounds = phase_boundaries(frames)
    print(f"dump: {len(frames)} frames, {len(frames[0]['spikes'])} spikes, "
          f"total {dump['total_duration']:.2f}s "
          f"(travel→{bounds[0]:.2f}s, fade→{bounds[1]:.2f}s)")

    mp4 = args.output / "frost_lance.mp4"
    encode_mp4(frames, dump, bounds, mp4)
    encode_gif(mp4, args.output / "frost_lance.gif")


if __name__ == "__main__":
    main()
