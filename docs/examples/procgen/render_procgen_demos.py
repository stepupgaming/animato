#!/usr/bin/env python3
"""Render short, looping procedural-geometry card demos.

The renderer intentionally has no project-specific dependencies: NumPy, Pillow,
SciPy, and a local ffmpeg are enough. Run it from this directory (or pass an
output directory) to regenerate the four compact MP4 examples.
"""

from __future__ import annotations

import argparse
import math
import shutil
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from scipy.spatial import Delaunay


W, H = 640, 360
FPS = 18
SECONDS = 3.0
FRAMES = int(FPS * SECONDS)
BG = (8, 12, 26)
CARD = (13, 19, 38)
PLOT = (44, 78, 596, 294)
PLOT_BG = (9, 16, 33)
BLUE = (58, 174, 255)
CYAN = (100, 230, 255)
MUTED = (104, 130, 166)
GRID = (25, 47, 75)


def font(size: int, bold: bool = False):
    name = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf" if bold else "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
    try:
        return ImageFont.truetype(name, size)
    except OSError:
        return ImageFont.load_default()


FONT_TITLE = font(13, True)
FONT_LABEL = font(10)
FONT_SMALL = font(9)


def smoothstep(x: float) -> float:
    return x * x * (3.0 - 2.0 * x)


def hsv_rgb(h: np.ndarray, s: np.ndarray, v: np.ndarray) -> np.ndarray:
    """Vectorized HSV-to-RGB helper for field visualizations."""
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


def base_card(title: str, subtitle: str) -> tuple[Image.Image, ImageDraw.ImageDraw]:
    image = Image.new("RGB", (W, H), BG)
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((20, 16, 620, 344), radius=14, fill=CARD, outline=(30, 55, 88), width=1)
    draw.text((42, 29), title, font=FONT_TITLE, fill=(224, 238, 255))
    draw.text((42, 47), subtitle, font=FONT_SMALL, fill=MUTED)
    draw.rounded_rectangle((537, 28, 598, 49), radius=9, fill=(13, 47, 76), outline=(34, 116, 164))
    draw.ellipse((547, 35, 553, 41), fill=CYAN)
    draw.text((559, 32), "LIVE", font=FONT_SMALL, fill=(150, 225, 255))
    draw.rounded_rectangle(PLOT, radius=8, fill=PLOT_BG, outline=(28, 61, 94), width=1)
    px0, py0, px1, py1 = PLOT
    for x in range(px0 + 24, px1, 46):
        draw.line((x, py0 + 1, x, py1 - 1), fill=GRID, width=1)
    for y in range(py0 + 22, py1, 35):
        draw.line((px0 + 1, y, px1 - 1, y), fill=GRID, width=1)
    return image, draw


def plot_grid() -> tuple[np.ndarray, np.ndarray]:
    x = np.linspace(0.0, 1.0, 276)
    y = np.linspace(0.0, 1.0, 108)
    return np.meshgrid(x, y)


def paste_field(image: Image.Image, field: np.ndarray) -> None:
    """Paste a 276x108 RGB field into the 552x216 plot rectangle."""
    field_image = Image.fromarray(field, mode="RGB").resize((552, 216), Image.Resampling.BILINEAR)
    image.paste(field_image, (44, 78))


def draw_footer(draw: ImageDraw.ImageDraw, left: str, right: str) -> None:
    draw.text((45, 309), left, font=FONT_SMALL, fill=MUTED)
    draw.text((497, 309), right, font=FONT_SMALL, fill=(78, 162, 205))


def voronoi_frame(t: float, start: np.ndarray, goal: np.ndarray) -> Image.Image:
    image, draw = base_card("VORONOI / LLOYD RELAXATION", "iterative centroidal cells · blue-noise spacing")
    # Ping-pong easing makes the three-second clip seamless while retaining a
    # visible settle toward the centroidal configuration.
    p = 0.5 - 0.5 * math.cos(2.0 * math.pi * t)
    p = smoothstep(p)
    sites = start * (1.0 - p) + goal * p
    xx, yy = plot_grid()
    dx = xx[..., None] - sites[:, 0]
    dy = yy[..., None] - sites[:, 1]
    dist = dx * dx + dy * dy
    nearest = np.argmin(dist, axis=-1)
    palette = np.array(
        [[12, 41, 69], [13, 49, 79], [14, 57, 91], [16, 62, 101], [17, 53, 91], [14, 45, 78]],
        dtype=np.uint8,
    )
    field = palette[nearest % len(palette)].copy()
    edge = np.zeros(nearest.shape, dtype=bool)
    edge[:, 1:] |= nearest[:, 1:] != nearest[:, :-1]
    edge[1:, :] |= nearest[1:, :] != nearest[:-1, :]
    field[edge] = (25, 117, 164)
    paste_field(image, field)
    for sx, sy in sites:
        x, y = 44 + sx * 552, 78 + sy * 216
        draw.ellipse((x - 4, y - 4, x + 4, y + 4), fill=CYAN, outline=(207, 247, 255), width=1)
        draw.ellipse((x - 8, y - 8, x + 8, y + 8), outline=(39, 141, 194), width=1)
    draw_footer(draw, "18 SITES  ·  CENTROIDAL ITERATION", "ITERATE 01 / 04")
    return image


def lloyd_goal(points: np.ndarray, iterations: int = 10) -> np.ndarray:
    """Approximate Lloyd relaxation using a dense ownership grid."""
    yy, xx = np.mgrid[0:80, 0:120].astype(float)
    xx /= 119.0
    yy /= 79.0
    result = points.copy()
    for _ in range(iterations):
        d = (xx[..., None] - result[:, 0]) ** 2 + (yy[..., None] - result[:, 1]) ** 2
        owner = np.argmin(d, axis=-1)
        for idx in range(len(result)):
            mask = owner == idx
            if np.any(mask):
                result[idx] = (xx[mask].mean(), yy[mask].mean())
    return result


def delaunay_frame(t: float, base: np.ndarray) -> Image.Image:
    image, draw = base_card("DELAUNAY TRIANGULATION", "moving sites · adjacency updates every frame")
    phase = 2.0 * math.pi * t
    points = base.copy()
    points[:, 0] += 0.030 * np.sin(phase * 1.7 + np.arange(len(points)) * 1.9)
    points[:, 1] += 0.028 * np.cos(phase * 1.3 + np.arange(len(points)) * 1.3)
    points = np.clip(points, 0.04, 0.96)
    tri = Delaunay(points)
    for simplex in tri.simplices:
        coords = [(44 + points[i, 0] * 552, 78 + points[i, 1] * 216) for i in simplex]
        draw.polygon(coords, fill=(12, 39, 64), outline=(28, 93, 131))
    for a, b in tri.simplices[:, [(0, 1), (1, 2), (2, 0)]].reshape(-1, 2):
        x1, y1 = 44 + points[a, 0] * 552, 78 + points[a, 1] * 216
        x2, y2 = 44 + points[b, 0] * 552, 78 + points[b, 1] * 216
        draw.line((x1, y1, x2, y2), fill=(54, 175, 228), width=1)
    for sx, sy in points:
        x, y = 44 + sx * 552, 78 + sy * 216
        draw.ellipse((x - 4, y - 4, x + 4, y + 4), fill=(12, 29, 52), outline=CYAN, width=2)
        draw.ellipse((x - 1, y - 1, x + 1, y + 1), fill=(224, 251, 255))
    draw_footer(draw, "20 SITES  ·  34 EDGES", "TOPOLOGY  /  LIVE")
    return image


def poisson_sites(seed: int, count: int = 23, minimum: float = 0.14) -> np.ndarray:
    rng = np.random.default_rng(seed)
    sites: list[np.ndarray] = []
    attempts = 0
    while len(sites) < count and attempts < 50000:
        candidate = rng.uniform(0.06, 0.94, 2)
        if all(np.linalg.norm(candidate - old) >= minimum for old in sites):
            sites.append(candidate)
        attempts += 1
    return np.array(sites)


def worley_frame(t: float, base: np.ndarray) -> Image.Image:
    image, draw = base_card("POISSON DISK / WORLEY", "minimum-radius sites · nearest-feature field")
    phase = 2.0 * math.pi * t
    sites = base.copy()
    sites[:, 0] += 0.020 * np.sin(phase + np.arange(len(sites)) * 2.4)
    sites[:, 1] += 0.020 * np.cos(phase * 0.8 + np.arange(len(sites)) * 1.7)
    sites = np.clip(sites, 0.045, 0.955)
    xx, yy = plot_grid()
    distances = np.sqrt((xx[..., None] - sites[:, 0]) ** 2 + (yy[..., None] - sites[:, 1]) ** 2)
    d1 = np.min(distances, axis=-1)
    rings = np.exp(-((d1 - (0.055 + 0.012 * math.sin(phase))) / 0.018) ** 2)
    value = np.clip(0.18 + 0.78 * (1.0 - np.minimum(d1 / 0.30, 1.0)) + 0.26 * rings, 0.0, 1.0)
    hue = 0.56 + 0.035 * np.sin(phase + d1 * 17.0)
    field = hsv_rgb(hue, np.full_like(value, 0.78), value)
    paste_field(image, field)
    for sx, sy in sites:
        x, y = 44 + sx * 552, 78 + sy * 216
        draw.ellipse((x - 3, y - 3, x + 3, y + 3), fill=(235, 252, 255), outline=CYAN)
        draw.ellipse((x - 9, y - 9, x + 9, y + 9), outline=(35, 129, 178), width=1)
    draw_footer(draw, "23 POISSON SITES  ·  F1 DISTANCE", "FIELD  /  WORLEY")
    return image


def caustics_frame(t: float) -> Image.Image:
    image, draw = base_card("ANALYTICAL CAUSTICS", "interference ripples · animated intensity accumulation")
    xx, yy = plot_grid()
    phase = 2.0 * math.pi * t
    centers = np.array(
        [
            [0.32 + 0.10 * math.cos(phase), 0.48 + 0.10 * math.sin(phase)],
            [0.68 + 0.08 * math.cos(phase * 1.3 + 1.7), 0.47 + 0.11 * math.sin(phase * 1.3 + 1.7)],
            [0.50 + 0.12 * math.cos(phase * 0.7 + 3.1), 0.56 + 0.07 * math.sin(phase * 0.7 + 3.1)],
        ]
    )
    field = np.zeros_like(xx)
    for cx, cy in centers:
        radius = np.sqrt((xx - cx) ** 2 + (yy - cy) ** 2)
        field += (0.5 + 0.5 * np.cos(radius * 72.0 - phase * 2.0)) / (1.0 + radius * 12.0)
    field += 0.25 * (0.5 + 0.5 * np.sin(xx * 33.0 + yy * 21.0 - phase * 1.5))
    value = np.clip((field - 0.10) / 1.75, 0.0, 1.0) ** 0.72
    hue = 0.57 - 0.045 * value
    field_rgb = hsv_rgb(hue, 0.86 - 0.16 * value, value)
    paste_field(image, field_rgb)
    for cx, cy in centers:
        x, y = 44 + cx * 552, 78 + cy * 216
        draw.ellipse((x - 3, y - 3, x + 3, y + 3), fill=CYAN)
        draw.ellipse((x - 13, y - 13, x + 13, y + 13), outline=(30, 120, 175), width=1)
    draw_footer(draw, "3 SOURCES  ·  PHASE-SAFE LOOP", "INTENSITY  /  0—1")
    return image


def encode(name: str, render, output: Path) -> None:
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        raise RuntimeError("ffmpeg is required to encode the compact MP4 demos")
    command = [
        ffmpeg, "-loglevel", "error", "-y", "-f", "rawvideo", "-vcodec", "rawvideo",
        "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-", "-an",
        "-c:v", "libx264", "-preset", "medium", "-crf", "30", "-pix_fmt", "yuv420p",
        "-movflags", "+faststart", str(output),
    ]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as process:
        assert process.stdin is not None
        for frame in range(FRAMES):
            process.stdin.write(np.asarray(render(frame / FRAMES), dtype=np.uint8).tobytes())
        process.stdin.close()
        if process.wait() != 0:
            raise RuntimeError(f"ffmpeg failed while encoding {name}")
    print(f"{output.name}: {output.stat().st_size:,} bytes")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(20260921)
    initial = rng.uniform(0.08, 0.92, (18, 2))
    relaxed = lloyd_goal(initial)
    delaunay_sites = rng.uniform(0.08, 0.92, (20, 2))
    poisson = poisson_sites(81)
    jobs = [
        ("voronoi_lloyd.mp4", lambda t: voronoi_frame(t, initial, relaxed)),
        ("delaunay.mp4", lambda t: delaunay_frame(t, delaunay_sites)),
        ("poisson_worley.mp4", lambda t: worley_frame(t, poisson)),
        ("caustics.mp4", caustics_frame),
    ]
    for name, render in jobs:
        encode(name, render, args.output / name)


if __name__ == "__main__":
    main()
