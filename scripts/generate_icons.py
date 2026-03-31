#!/usr/bin/env python3
"""
Generate PWA icons for agent-deck.
Produces icon-192.png and icon-512.png in the same directory as this script.
"""

import math
import os
import sys

try:
    from PIL import Image, ImageDraw, ImageFilter, ImageFont
except ImportError:
    print("ERROR: Pillow not found. Install with: pip3 install Pillow")
    sys.exit(1)

# ── Palette ──────────────────────────────────────────────────────────────────
BG_COLOR = (28, 28, 26)  # #1C1C1A  – near-black warm dark
ACCENT_COLOR = (99, 179, 237)  # #63B3ED  – calm sky-blue accent
ACCENT2_COLOR = (49, 130, 206)  # #3182CE  – deeper accent for depth
WHITE = (255, 255, 255)
TEXT_COLOR = (255, 255, 255, 255)

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))


def lerp_color(c1, c2, t):
    return tuple(int(c1[i] + (c2[i] - c1[i]) * t) for i in range(3))


def draw_rounded_rect(draw, xy, radius, fill):
    """Draw a filled rounded rectangle."""
    x0, y0, x1, y1 = xy
    draw.rectangle([x0 + radius, y0, x1 - radius, y1], fill=fill)
    draw.rectangle([x0, y0 + radius, x1, y1 - radius], fill=fill)
    draw.ellipse([x0, y0, x0 + radius * 2, y0 + radius * 2], fill=fill)
    draw.ellipse([x1 - radius * 2, y0, x1, y0 + radius * 2], fill=fill)
    draw.ellipse([x0, y1 - radius * 2, x0 + radius * 2, y1], fill=fill)
    draw.ellipse([x1 - radius * 2, y1 - radius * 2, x1, y1], fill=fill)


def make_radial_gradient_bg(size, center_color, edge_color):
    """Create a subtle radial gradient background layer."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    pixels = img.load()
    cx = cy = size / 2.0
    max_r = size * 0.72
    for y in range(size):
        for x in range(size):
            dx = x - cx
            dy = y - cy
            dist = math.sqrt(dx * dx + dy * dy)
            t = min(dist / max_r, 1.0)
            # ease-in-out
            t = t * t * (3 - 2 * t)
            col = lerp_color(center_color, edge_color, t)
            pixels[x, y] = (*col, 255)
    return img


def draw_terminal_bracket(draw, cx, cy, size, color, thickness):
    """
    Draw a subtle terminal-style '[ ]' bracket frame around the AD letters.
    This gives a techy / agent feel without being too complex.
    """
    half = size * 0.38
    arm = size * 0.12  # length of horizontal arm
    lw = max(1, int(thickness))

    # left bracket  [
    lx = cx - half
    draw.line([(lx, cy - half), (lx + arm, cy - half)], fill=color, width=lw)
    draw.line([(lx, cy - half), (lx, cy + half)], fill=color, width=lw)
    draw.line([(lx, cy + half), (lx + arm, cy + half)], fill=color, width=lw)

    # right bracket  ]
    rx = cx + half
    draw.line([(rx - arm, cy - half), (rx, cy - half)], fill=color, width=lw)
    draw.line([(rx, cy - half), (rx, cy + half)], fill=color, width=lw)
    draw.line([(rx - arm, cy + half), (rx, cy + half)], fill=color, width=lw)


def try_load_font(size):
    """
    Try to load a bold system font. Falls back to default if none found.
    """
    candidates = [
        # macOS
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/Library/Fonts/Arial Bold.ttf",
        "/System/Library/Fonts/SFNSDisplay.ttf",
        "/System/Library/Fonts/SFNS.ttf",
        # Linux fallbacks
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    ]
    for path in candidates:
        if os.path.exists(path):
            try:
                return ImageFont.truetype(path, size)
            except Exception:
                continue
    # Pillow built-in bitmap fallback
    return ImageFont.load_default()


def generate_icon(px: int):
    s = px  # shorthand

    # ── 1. Background: radial gradient (slightly lighter centre) ─────────────
    centre_col = lerp_color(BG_COLOR, ACCENT2_COLOR, 0.08)  # very subtle blue tint
    edge_col = lerp_color(BG_COLOR, (0, 0, 0), 0.35)  # slightly darker edges
    bg = make_radial_gradient_bg(s, centre_col, edge_col)

    # ── 2. Mask to rounded-square (corner radius ≈ 22 % of size) ─────────────
    radius = int(s * 0.22)
    mask = Image.new("L", (s, s), 0)
    mask_draw = ImageDraw.Draw(mask)
    draw_rounded_rect(mask_draw, (0, 0, s - 1, s - 1), radius, 255)
    bg.putalpha(mask)

    # ── 3. Compose onto final RGBA canvas ────────────────────────────────────
    canvas = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    canvas.alpha_composite(bg)

    draw = ImageDraw.Draw(canvas)
    cx = cy = s / 2

    # ── 4. Accent ring (thin circle) ─────────────────────────────────────────
    ring_r = s * 0.38
    ring_w = max(1, int(s * 0.018))
    ring_col = (*ACCENT_COLOR, 55)  # very subtle
    draw.ellipse(
        [cx - ring_r, cy - ring_r, cx + ring_r, cy + ring_r],
        outline=ring_col,
        width=ring_w,
    )

    # ── 5. Terminal bracket decoration ───────────────────────────────────────
    bracket_col = (*ACCENT_COLOR, 140)
    bracket_thick = max(1, s * 0.025)
    draw_terminal_bracket(draw, cx, cy, s, bracket_col, bracket_thick)

    # ── 6. "AD" monogram ─────────────────────────────────────────────────────
    font_size = int(s * 0.36)
    font = try_load_font(font_size)

    text = "AD"
    # Measure text
    bbox = draw.textbbox((0, 0), text, font=font)
    tw = bbox[2] - bbox[0]
    th = bbox[3] - bbox[1]
    tx = cx - tw / 2 - bbox[0]
    ty = cy - th / 2 - bbox[1]

    # Subtle glow: draw text slightly blurred in accent colour first
    glow_layer = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow_layer)
    gd.text((tx, ty), text, font=font, fill=(*ACCENT2_COLOR, 120))
    glow_layer = glow_layer.filter(ImageFilter.GaussianBlur(radius=s * 0.025))
    canvas.alpha_composite(glow_layer)

    # Crisp white text on top
    draw.text((tx, ty), text, font=font, fill=TEXT_COLOR)

    # ── 7. Tiny dot accent (bottom-right quadrant) ───────────────────────────
    dot_r = max(2, int(s * 0.028))
    dot_x = cx + s * 0.21
    dot_y = cy + s * 0.26
    draw.ellipse(
        [dot_x - dot_r, dot_y - dot_r, dot_x + dot_r, dot_y + dot_r],
        fill=(*ACCENT_COLOR, 200),
    )

    # ── 8. Flatten to RGB and save ────────────────────────────────────────────
    final = Image.new("RGB", (s, s), BG_COLOR)
    final.paste(canvas, mask=canvas.split()[3])

    out_path = os.path.join(SCRIPT_DIR, f"icon-{px}.png")
    final.save(out_path, "PNG", optimize=True)

    size_kb = os.path.getsize(out_path) / 1024
    print(f"  ✓  icon-{px}.png  →  {out_path}  ({size_kb:.1f} KB)")
    return out_path


def main():
    print("agent-deck PWA icon generator")
    print("=" * 44)
    for px in (192, 512):
        generate_icon(px)
    print("=" * 44)
    print("Done. Both icons generated successfully.")


if __name__ == "__main__":
    main()
