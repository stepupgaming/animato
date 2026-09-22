#!/usr/bin/env python3
"""Render polished 1280x720 procedural-geometry product-demo loops.

The clips are deliberately rendered as dark, glassy UI cards rather than plain
plots. NumPy does the scalar fields, Pillow supplies high-quality typography
and compositing, and ffmpeg encodes compact H.264 MP4s.
"""

from __future__ import annotations

import argparse
import math
import shutil
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont
from scipy.spatial import Delaunay


WIDTH, HEIGHT = 1280, 720
FPS = 30
SECONDS = 6.0
FRAME_COUNT = int(FPS * SECONDS)

PLOT_X, PLOT_Y, PLOT_W, PLOT_H = 120, 190, 1040, 400
FIELD_W, FIELD_H = 780, 300
BG = (5, 8, 18)
CARD = (12, 18, 34, 247)
PLOT_BG = (5, 12, 25, 242)
CYAN = (102, 224, 255)
BLUE = (47, 151, 239)
ICE = (212, 249, 255)
MUTED = (111, 135, 166)
DIM = (45, 74, 108)
GRID = (23, 44, 71, 115)


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


def lerp(a, b, t):
    return a * (1.0 - t) + b * t


def smoothstep(x):
    x = np.clip(x, 0.0, 1.0)
    return x * x * (3.0 - 2.0 * x)


def hsv_rgb(h: np.ndarray, s: np.ndarray, v: np.ndarray) -> np.ndarray:
    h = np.mod(h, 1.0) * 6.0
    i = np.floor(h).astype(np.int32)
    f = h - i
    p = v * (1.0 - s)
    q = v * (1.0 - s * f)
    tt = v * (1.0 - s * (1.0 - f))
    choices = np.stack(
        [
            np.stack([v, tt, p], -1),
            np.stack([q, v, p], -1),
            np.stack([p, v, tt], -1),
            np.stack([p, q, v], -1),
            np.stack([tt, p, v], -1),
            np.stack([v, p, q], -1),
        ],
        axis=0,
    )
    rows, cols = np.indices(i.shape)
    return (choices[i, rows, cols] * 255.0).astype(np.uint8)


def screen_point(point: np.ndarray | tuple[float, float]) -> tuple[float, float]:
    return (PLOT_X + float(point[0]) * PLOT_W, PLOT_Y + float(point[1]) * PLOT_H)


def build_background() -> Image.Image:
    yy, xx = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float32)
    # A very subtle radial cyan lift behind the card keeps the frame from
    # feeling flat while preserving a dark, readable canvas.
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


def add_glow(image: Image.Image, draw_fn, radius: int = 16, alpha: int = 120) -> None:
    glow, glow_draw = alpha_layer()
    draw_fn(glow_draw, alpha)
    image.alpha_composite(glow.filter(ImageFilter.GaussianBlur(radius)))


def neon_line(image: Image.Image, points, color=CYAN, width=2, glow=12, alpha=210) -> None:
    # The field itself carries the soft light; direct vector strokes keep the
    # many Delaunay edges inexpensive and perfectly sharp at 1280px.
    ImageDraw.Draw(image).line(points, fill=(*color, 255), width=width, joint="curve")


def neon_dot(image: Image.Image, point, radius=5, color=CYAN, alpha=235) -> None:
    x, y = point
    draw = ImageDraw.Draw(image)
    draw.ellipse((x - radius * 2.8, y - radius * 2.8, x + radius * 2.8, y + radius * 2.8), outline=(*color, 255), width=max(2, radius // 2))
    draw.ellipse((x - radius * 1.7, y - radius * 1.7, x + radius * 1.7, y + radius * 1.7), outline=(*color, 255), width=1)
    draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=(*color, 255), outline=(*ICE, 255), width=1)


def put_field(image: Image.Image, rgb: np.ndarray, alpha: int = 224) -> None:
    rgba = np.dstack([rgb, np.full(rgb.shape[:2], alpha, dtype=np.uint8)])
    field = Image.fromarray(rgba, mode="RGBA").resize((PLOT_W, PLOT_H), Image.Resampling.LANCZOS)
    image.alpha_composite(field, (PLOT_X, PLOT_Y))


def base_scene(kicker: str, title: str, subtitle: str, chip: str, right_meta: str) -> tuple[Image.Image, ImageDraw.ImageDraw]:
    image = BACKGROUND.copy()
    draw = ImageDraw.Draw(image)
    # Glass card and a restrained top rim highlight.
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
    # Fine graph paper lines. The content is overlaid later, then a few labels
    # are redrawn above it to keep the card UI legible.
    for x in range(PLOT_X + 40, PLOT_X + PLOT_W, 80):
        draw.line((x, PLOT_Y + 2, x, PLOT_Y + PLOT_H - 2), fill=GRID, width=1)
    for y in range(PLOT_Y + 40, PLOT_Y + PLOT_H, 56):
        draw.line((PLOT_X + 2, y, PLOT_X + PLOT_W - 2, y), fill=GRID, width=1)
    draw.text((PLOT_X + 22, PLOT_Y + 16), "LIVE PREVIEW", font=FONT_PLOT, fill=(78, 139, 181))
    draw.text((PLOT_X + PLOT_W - 105, PLOT_Y + 16), right_meta, font=FONT_TINY, fill=(74, 112, 146))
    draw.text((120, 625), "ANIMATO  /  PROCEDURAL LAB", font=FONT_META_BOLD, fill=(74, 116, 151))
    draw.text((370, 625), "GPU-FRIENDLY GRAPH PRIMITIVES", font=FONT_META, fill=(69, 92, 122))
    draw.text((1018, 625), "01—06 SEC", font=FONT_META_BOLD, fill=(74, 157, 204))
    return image, ImageDraw.Draw(image)


def draw_target_ring(image: Image.Image, point, radius=11, color=(51, 129, 181), alpha=150) -> None:
    x, y = point
    ImageDraw.Draw(image).ellipse((x - radius, y - radius, x + radius, y + radius), outline=(*color, 255), width=1)


def lloyd_goal(points: np.ndarray, iterations: int = 12) -> np.ndarray:
    yy, xx = np.mgrid[0:100, 0:160].astype(float)
    xx /= 159.0
    yy /= 99.0
    result = points.copy()
    for _ in range(iterations):
        dist = (xx[..., None] - result[:, 0]) ** 2 + (yy[..., None] - result[:, 1]) ** 2
        owners = np.argmin(dist, axis=-1)
        for idx in range(len(result)):
            mask = owners == idx
            if np.any(mask):
                result[idx] = (xx[mask].mean(), yy[mask].mean())
    return result


def voronoi_frame(t: float, start: np.ndarray, goal: np.ndarray) -> Image.Image:
    image, draw = base_scene(
        "CARD 01  /  SPATIAL PARTITION",
        "VORONOI",
        "Lloyd relaxation moves each site toward the centroid of its cell",
        "LLOYD RELAXATION",
        "26 SITES  /  CVD",
    )
    q = 0.5 - 0.5 * math.cos(2.0 * math.pi * t)
    q = float(smoothstep(q))
    points = start * (1.0 - q) + goal * q
    xx = np.linspace(0.0, 1.0, FIELD_W)[None, :]
    yy = np.linspace(0.0, 1.0, FIELD_H)[:, None]
    dist = (xx[..., None] - points[:, 0]) ** 2 + (yy[..., None] - points[:, 1]) ** 2
    order = np.argsort(dist, axis=-1)
    owner = order[..., 0]
    nearest = np.take_along_axis(dist, order[..., :2], axis=-1)
    gap = nearest[..., 1] - nearest[..., 0]
    palette = np.zeros((len(points), 3), dtype=np.float32)
    hues = np.linspace(0.56, 0.62, len(points), endpoint=False)
    palette[:] = np.stack(
        [15 + 10 * np.sin(hues * 9), 39 + 21 * np.sin(hues * 8 + 0.8), 67 + 34 * np.sin(hues * 7 + 0.3)], axis=1
    )
    field = np.clip(palette[owner], 0, 255).astype(np.uint8)
    boundary = np.exp(-gap * 15500.0)[..., None]
    field = np.clip(field * (1.0 - 0.18 * boundary) + np.array([25, 126, 177]) * boundary, 0, 255).astype(np.uint8)
    put_field(image, field, 218)

    # Motion vectors to centroids make the Lloyd operation explicit even in a
    # single still frame, while faint target rings add a polished HUD feel.
    for current, target in zip(points, goal):
        a, b = screen_point(current), screen_point(target)
        neon_line(image, (a, b), color=(32, 112, 166), width=1, glow=7, alpha=105)
        draw_target_ring(image, b, radius=9, color=(57, 139, 185), alpha=125)
    for point in points:
        neon_dot(image, screen_point(point), radius=4, color=CYAN, alpha=245)

    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((PLOT_X + 22, PLOT_Y + PLOT_H - 43, PLOT_X + 230, PLOT_Y + PLOT_H - 16), radius=12, fill=(7, 22, 40, 220), outline=(30, 87, 121, 200))
    draw.text((PLOT_X + 38, PLOT_Y + PLOT_H - 37), f"RELAXATION  {int(1 + q * 11):02d} / 12", font=FONT_TINY, fill=(126, 206, 240))
    draw.text((PLOT_X + PLOT_W - 250, PLOT_Y + PLOT_H - 37), "SITE  →  CELL CENTROID", font=FONT_TINY, fill=(93, 159, 197))
    return image.convert("RGB")


def circumcenter(a: np.ndarray, b: np.ndarray, c: np.ndarray) -> np.ndarray:
    ax, ay = a
    bx, by = b
    cx, cy = c
    denominator = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by))
    if abs(denominator) < 1e-8:
        return (a + b + c) / 3.0
    aa, bb, cc = ax * ax + ay * ay, bx * bx + by * by, cx * cx + cy * cy
    ux = (aa * (by - cy) + bb * (cy - ay) + cc * (ay - by)) / denominator
    uy = (aa * (cx - bx) + bb * (ax - cx) + cc * (bx - ax)) / denominator
    return np.array([ux, uy])


def delaunay_frame(t: float, base: np.ndarray) -> Image.Image:
    image, draw = base_scene(
        "CARD 02  /  DUAL MESH",
        "DELAUNAY",
        "A moving triangulation and its Voronoi dual share the same sites",
        "DUAL GRAPH",
        "18 SITES  /  LIVE",
    )
    phase = 2.0 * math.pi * t
    points = base.copy()
    idx = np.arange(len(points), dtype=float)
    points[:, 0] += 0.035 * np.sin(phase * 0.75 + idx * 1.73) + 0.012 * np.sin(phase * 1.7 + idx)
    points[:, 1] += 0.030 * np.cos(phase * 0.63 + idx * 1.31) + 0.010 * np.cos(phase * 1.4 + idx * 0.7)
    points = np.clip(points, 0.035, 0.965)
    triangulation = Delaunay(points)
    centers = np.array([circumcenter(points[a], points[b], points[c]) for a, b, c in triangulation.simplices])
    centers = np.clip(centers, -0.25, 1.25)

    # Translucent triangle surfaces provide depth, while the thinner dual links
    # show the Voronoi relationship without turning the frame into a sketch.
    faces, face_draw = alpha_layer()
    for tri_index, simplex in enumerate(triangulation.simplices):
        coords = [screen_point(points[i]) for i in simplex]
        fill = (15 + (tri_index % 3) * 3, 53 + (tri_index % 3) * 4, 84 + (tri_index % 3) * 8, 64)
        face_draw.polygon(coords, fill=fill)
    image.alpha_composite(faces)

    linked = set()
    for tri_index, neighbors in enumerate(triangulation.neighbors):
        for neighbor in neighbors:
            if neighbor < 0 or (neighbor, tri_index) in linked:
                continue
            linked.add((tri_index, neighbor))
            a, b = screen_point(centers[tri_index]), screen_point(centers[neighbor])
            if -80 < a[0] < WIDTH + 80 and -80 < a[1] < HEIGHT + 80 and -80 < b[0] < WIDTH + 80 and -80 < b[1] < HEIGHT + 80:
                neon_line(image, (a, b), color=(66, 176, 218), width=1, glow=7, alpha=170)
    for simplex in triangulation.simplices:
        for a, b in ((simplex[0], simplex[1]), (simplex[1], simplex[2]), (simplex[2], simplex[0])):
            neon_line(image, (screen_point(points[a]), screen_point(points[b])), color=(45, 133, 198), width=2, glow=9, alpha=210)
    for center in centers:
        if 0.0 <= center[0] <= 1.0 and 0.0 <= center[1] <= 1.0:
            draw_target_ring(image, screen_point(center), radius=4, color=(94, 218, 246), alpha=170)
    for point in points:
        neon_dot(image, screen_point(point), radius=5, color=ICE, alpha=245)

    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((PLOT_X + 22, PLOT_Y + PLOT_H - 43, PLOT_X + 324, PLOT_Y + PLOT_H - 16), radius=12, fill=(7, 22, 40, 220), outline=(30, 87, 121, 200))
    draw.text((PLOT_X + 38, PLOT_Y + PLOT_H - 37), "TRIANGLES  /  VORONOI DUAL", font=FONT_TINY, fill=(126, 206, 240))
    draw.text((PLOT_X + PLOT_W - 226, PLOT_Y + PLOT_H - 37), "TOPOLOGY  UPDATES", font=FONT_TINY, fill=(93, 159, 197))
    return image.convert("RGB")


def poisson_sites(seed: int, count: int = 25, minimum: float = 0.145) -> np.ndarray:
    rng = np.random.default_rng(seed)
    sites: list[np.ndarray] = []
    attempts = 0
    while len(sites) < count and attempts < 100000:
        candidate = rng.uniform(0.055, 0.945, 2)
        if all(np.linalg.norm(candidate - old) >= minimum for old in sites):
            sites.append(candidate)
        attempts += 1
    if len(sites) < count:
        raise RuntimeError("Poisson sampler could not place all feature points")
    return np.array(sites)


def poisson_worley_frame(t: float, sites: np.ndarray) -> Image.Image:
    image, draw = base_scene(
        "CARD 03  /  FEATURE FIELD",
        "POISSON + WORLEY",
        "Minimum-distance samples appear, then illuminate a nearest-feature field",
        "FIELD LIGHTING",
        "25 SITES  /  F1",
    )
    # Ping-pong cycle: points emerge from a sparse seed set, reach full density,
    # and recede to the same seed state so the exported clip loops cleanly.
    q = float(0.5 - 0.5 * math.cos(2.0 * math.pi * t))
    q = float(smoothstep(q))
    active_count = max(2, int(round(2 + (len(sites) - 2) * q)))
    active = sites[:active_count]
    xx = np.linspace(0.0, 1.0, FIELD_W)[None, :]
    yy = np.linspace(0.0, 1.0, FIELD_H)[:, None]
    distances = np.sqrt((xx[..., None] - active[:, 0]) ** 2 + (yy[..., None] - active[:, 1]) ** 2)
    d1 = np.min(distances, axis=-1)
    field_strength = 0.12 + 0.88 * q
    glow_rings = np.exp(-((d1 - (0.047 + 0.012 * math.sin(2.0 * math.pi * t))) / 0.014) ** 2)
    value = np.clip(0.10 + field_strength * (0.92 * (1.0 - np.minimum(d1 / 0.30, 1.0)) + 0.24 * glow_rings), 0.0, 1.0)
    hue = 0.56 + 0.025 * np.sin(d1 * 25.0 + 2.0 * math.pi * t)
    field = hsv_rgb(hue, np.full_like(value, 0.80), value)
    put_field(image, field, 224)

    # Show the guaranteed minimum-distance footprint around each active sample.
    for point in active:
        x, y = screen_point(point)
        ring, ring_draw = alpha_layer()
        radius = 0.072 * PLOT_W
        ring_draw.ellipse((x - radius, y - radius, x + radius, y + radius), outline=(54, 154, 198, 82), width=1)
        image.alpha_composite(ring)
        neon_dot(image, (x, y), radius=5, color=ICE, alpha=235)
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((PLOT_X + 22, PLOT_Y + PLOT_H - 43, PLOT_X + 305, PLOT_Y + PLOT_H - 16), radius=12, fill=(7, 22, 40, 220), outline=(30, 87, 121, 200))
    draw.text((PLOT_X + 38, PLOT_Y + PLOT_H - 37), f"SAMPLES  {active_count:02d} / {len(sites):02d}", font=FONT_TINY, fill=(126, 206, 240))
    draw.text((PLOT_X + PLOT_W - 205, PLOT_Y + PLOT_H - 37), "NEAREST FEATURE  F1", font=FONT_TINY, fill=(93, 159, 197))
    return image.convert("RGB")


def caustics_frame(t: float) -> Image.Image:
    image, draw = base_scene(
        "CARD 04  /  REFRACTED LIGHT",
        "CAUSTICS",
        "Water-like surface ripples focus moving light into bright ribbons",
        "WATER SURFACE",
        "ANALYTICAL  /  LOOP",
    )
    phase = 2.0 * math.pi * t
    x = np.linspace(0.0, 1.0, FIELD_W)[None, :]
    y = np.linspace(0.0, 1.0, FIELD_H)[:, None]
    # Two coupled traveling wave families mimic a shallow-water surface. The
    # product creates concentrated caustic ridges instead of rainbow noise.
    u = x + 0.035 * np.sin(y * 19.0 + phase * 0.72) + 0.015 * np.sin((x + y) * 31.0 - phase)
    v = y + 0.030 * np.sin(x * 16.0 - phase * 0.61) + 0.012 * np.cos((x - y) * 27.0 + phase * 0.8)
    wave_a = np.sin(u * 31.0 + 0.60 * np.sin(v * 11.0 + phase * 0.55) + phase * 0.92)
    wave_b = np.sin(v * 26.0 + 0.52 * np.sin(u * 13.0 - phase * 0.70) - phase * 0.63)
    ridge = np.exp(-np.abs(wave_a * wave_b) * 8.6)
    shimmer = 0.5 + 0.5 * np.sin((u + v) * 18.0 + phase * 0.4)
    intensity = np.clip(0.07 + 0.82 * ridge * (0.44 + 0.56 * shimmer) + 0.10 * np.exp(-np.abs(wave_a) * 5.0), 0.0, 1.0)
    intensity = intensity ** 0.82
    # Restrained cyan/blue water palette with pale highlights at focal ridges.
    field = np.empty((FIELD_H, FIELD_W, 3), dtype=np.uint8)
    field[..., 0] = np.clip(4 + 31 * intensity + 22 * intensity**3, 0, 255)
    field[..., 1] = np.clip(15 + 100 * intensity + 112 * intensity**2, 0, 255)
    field[..., 2] = np.clip(37 + 130 * intensity + 80 * intensity**2, 0, 255)
    put_field(image, field, 238)

    # A soft horizon-like refraction highlight makes it read as water in the
    # first glance, while the field carries the actual animation.
    overlay, overlay_draw = alpha_layer()
    overlay_draw.rectangle((PLOT_X, PLOT_Y + 2, PLOT_X + PLOT_W, PLOT_Y + 12), fill=(67, 175, 222, 18))
    image.alpha_composite(overlay.filter(ImageFilter.GaussianBlur(8)))
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((PLOT_X + 22, PLOT_Y + PLOT_H - 43, PLOT_X + 273, PLOT_Y + PLOT_H - 16), radius=12, fill=(7, 22, 40, 220), outline=(30, 87, 121, 200))
    draw.text((PLOT_X + 38, PLOT_Y + PLOT_H - 37), "REFRACTION  /  INTENSITY", font=FONT_TINY, fill=(126, 206, 240))
    draw.text((PLOT_X + PLOT_W - 183, PLOT_Y + PLOT_H - 37), "CAUSTIC RIBBONS", font=FONT_TINY, fill=(93, 159, 197))
    return image.convert("RGB")


def encode(name: str, renderer, output: Path) -> None:
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise RuntimeError("ffmpeg is required")
    command = [
        ffmpeg,
        "-loglevel", "error",
        "-y",
        "-f", "rawvideo",
        "-vcodec", "rawvideo",
        "-pix_fmt", "rgb24",
        "-s", f"{WIDTH}x{HEIGHT}",
        "-r", str(FPS),
        "-i", "-",
        "-an",
        "-c:v", "libx264",
        "-preset", "medium",
        "-crf", "25",
        "-profile:v", "high",
        "-pix_fmt", "yuv420p",
        "-movflags", "+faststart",
        str(output),
    ]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as process:
        assert process.stdin is not None
        for frame_index in range(FRAME_COUNT):
            if frame_index % FPS == 0:
                print(f"{name}: frame {frame_index:03d}/{FRAME_COUNT}")
            frame = np.asarray(renderer(frame_index / FRAME_COUNT), dtype=np.uint8)
            process.stdin.write(frame.tobytes())
        process.stdin.close()
        if process.wait() != 0:
            raise RuntimeError(f"ffmpeg failed while encoding {name}")
    print(f"{output}: {output.stat().st_size:,} bytes")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(20260921)
    initial = rng.uniform(0.07, 0.93, (26, 2))
    relaxed = lloyd_goal(initial)
    delaunay_sites = rng.uniform(0.07, 0.93, (18, 2))
    poisson = poisson_sites(8091)
    jobs = [
        ("voronoi_lloyd.mp4", lambda t: voronoi_frame(t, initial, relaxed)),
        ("delaunay.mp4", lambda t: delaunay_frame(t, delaunay_sites)),
        ("poisson_worley.mp4", lambda t: poisson_worley_frame(t, poisson)),
        ("caustics.mp4", caustics_frame),
    ]
    for name, renderer in jobs:
        encode(name, renderer, args.output / name)


if __name__ == "__main__":
    main()
