"""CraftOffice app-ikonok renderelése (ugyanaz a rajz, mint a launcherben): PNG-k és .ico.

Használat: python3 icons.py <app-id> <kimeneti-mappa>
Kimenet: <id>-1024.png, <id>-512.png, <id>-256.png, <id>-128.png, <id>-64.png, <id>-48.png,
<id>-32.png, <id>-16.png, <id>.ico
"""
import math
import sys
from PIL import Image, ImageDraw

NAVY = (0x14, 0x21, 0x3D)
GOLD_LIGHT = (0xF0, 0xC9, 0x6A)

COLORS = {
    "wordcraft": (0x3B, 0x6F, 0xD8), "gridcraft": (0x2E, 0x9E, 0x5B), "deckcraft": (0xE0, 0x6A, 0x3B),
    "designcraft": (0x1F, 0xA3, 0x9A), "pdfcraft": (0xD6, 0x3C, 0x3C), "photocraft": (0x2D, 0x8C, 0xE0),
    "vectorcraft": (0xF0, 0x9A, 0x1A), "lightcraft": (0x5B, 0x7C, 0xA8), "filmcraft": (0x8A, 0x5C, 0xD8),
    "effectcraft": (0xA8, 0x5C, 0xD0), "soundcraft": (0x14, 0xA8, 0xB8), "anicraft": (0xE5, 0x5B, 0x8F),
    "figcraft": (0xC2, 0x4E, 0x9E), "cadcraft": (0x6B, 0x7A, 0x8F), "materialcraft": (0x7A, 0x8B, 0x3A),
    "texturecraft": (0x9C, 0x6B, 0x3F), "craft-anything": (0x3B, 0x6E, 0xF0),
    "sketch": (0xE0, 0x5A, 0x8A),
}


def tile(kind, S=1024):
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    c = COLORS.get(kind, (0x8A, 0x93, 0xA6))
    fill = tuple(int(a * 0.42 + b * 0.58) for a, b in zip(c, NAVY))
    r = int(S * 0.22)
    d.rounded_rectangle([0, 0, S - 1, S - 1], radius=r, fill=fill + (255,))
    # app-színű csík alul
    bh = int(S * 0.09)
    band = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    bd = ImageDraw.Draw(band)
    bd.rounded_rectangle([0, 0, S - 1, S - 1], radius=r, fill=c + (255,))
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rectangle([0, S - bh, S, S], fill=255)
    img.paste(band, (0, 0), mask_and(band, mask))
    # piktogram a felső részen, 70%-os méretben
    size = S * 0.70
    ox = (S - size) / 2
    oy = (S - size) / 2 - bh * 0.35
    glyph(d, kind, ox, oy, size / 24.0, GOLD_LIGHT)
    return img


def mask_and(band, mask):
    from PIL import ImageChops
    return ImageChops.multiply(band.split()[3], mask)


def glyph(d, kind, ox, oy, u, col):
    P = lambda x, y: (ox + x * u, oy + y * u)
    w = max(1, int(u * 1.7))
    thin = max(1, int(u * 1.2))

    def line(pts, width=w):
        d.line([P(x, y) for x, y in pts], fill=col, width=width, joint="curve")
        for x, y in (pts[0], pts[-1]):
            cx, cy = P(x, y)
            d.ellipse([cx - width / 2, cy - width / 2, cx + width / 2, cy + width / 2], fill=col)

    def closed(pts, width=w):
        line(list(pts) + [pts[0]], width)

    def rrect(x0, y0, x1, y1, rr, width=w):
        d.rounded_rectangle([*P(x0, y0), *P(x1, y1)], radius=rr * u, outline=col, width=width)

    def frect(x0, y0, x1, y1, color=col, rr=0):
        d.rounded_rectangle([*P(x0, y0), *P(x1, y1)], radius=rr * u, fill=color)

    def circ(x, y, rad, width=w, filled=False, color=col):
        cx, cy = P(x, y)
        box = [cx - rad * u, cy - rad * u, cx + rad * u, cy + rad * u]
        if filled:
            d.ellipse(box, fill=color)
        else:
            d.ellipse(box, outline=color, width=width)

    def poly(pts, color=col):
        d.polygon([P(x, y) for x, y in pts], fill=color)

    dim = lambda f: tuple(int(v * f) for v in col)

    if kind == "wordcraft":
        rrect(5.5, 2.5, 18.5, 21.5, 1.5)
        for y, x1 in [(7.5, 15.5), (10.5, 15.5), (13.5, 15.5), (16.5, 12.5)]:
            line([(8.5, y), (x1, y)], thin)
    elif kind == "gridcraft":
        frect(3.5, 4.5, 20.5, 9.0, dim(0.55), 1.0)
        rrect(3.5, 4.5, 20.5, 19.5, 1.5)
        for x in (9.2, 14.8):
            line([(x, 4.5), (x, 19.5)], thin)
        for y in (9.0, 14.25):
            line([(3.5, y), (20.5, y)], thin)
    elif kind == "deckcraft":
        rrect(3.0, 3.5, 21.0, 15.0, 1.0)
        line([(12.0, 15.0), (12.0, 18.0)])
        line([(8.0, 21.0), (12.0, 18.0), (16.0, 21.0)])
        for x, h in [(8.0, 3.0), (11.0, 5.0), (14.0, 7.0)]:
            frect(x, 12.5 - h, x + 2.0, 12.5)
    elif kind == "designcraft":
        rrect(4.5, 2.5, 19.5, 21.5, 1.2)
        frect(7.0, 5.0, 17.0, 10.5, dim(0.8), 0.8)
        for y in (13.5, 16.0, 18.5):
            line([(7.0, y), (11.0, y)], thin)
            line([(13.0, y), (17.0, y)], thin)
    elif kind == "pdfcraft":
        closed([(5.5, 2.5), (14.0, 2.5), (18.5, 7.0), (18.5, 21.5), (5.5, 21.5)])
        line([(14.0, 2.5), (14.0, 7.0), (18.5, 7.0)], thin)
        line([(8.0, 16.0), (9.5, 13.5), (11.0, 16.5), (12.5, 13.5), (14.0, 16.5), (15.5, 15.0)], thin)
        line([(8.0, 18.5), (16.0, 18.5)], thin)
    elif kind == "photocraft":
        rrect(2.5, 4.5, 21.5, 19.5, 1.5)
        poly([(4.5, 17.5), (10.0, 10.5), (14.5, 17.5)])
        poly([(11.5, 17.5), (15.5, 12.0), (19.5, 17.5)], dim(0.75))
        circ(16.5, 8.5, 1.8, filled=True)
    elif kind == "vectorcraft":
        a, b, c2, dd = (4.0, 18.0), (6.0, 4.0), (18.0, 20.0), (20.0, 6.0)
        pts = []
        for i in range(41):
            t = i / 40
            m = 1 - t
            pts.append((m**3 * a[0] + 3 * m * m * t * b[0] + 3 * m * t * t * c2[0] + t**3 * dd[0],
                        m**3 * a[1] + 3 * m * m * t * b[1] + 3 * m * t * t * c2[1] + t**3 * dd[1]))
        line(pts)
        line([a, b], thin)
        line([dd, c2], thin)
        for x, y in (a, dd):
            frect(x - 1.6, y - 1.6, x + 1.6, y + 1.6)
        for x, y in (b, c2):
            circ(x, y, 1.4, filled=True)
    elif kind == "lightcraft":
        circ(12, 12, 9.0)
        circ(12, 12, 4.0)
        for k in range(6):
            a = k * math.tau / 6
            b = a + 1.15
            line([(12 + 4 * math.cos(a), 12 + 4 * math.sin(a)), (12 + 9 * math.cos(b), 12 + 9 * math.sin(b))], thin)
    elif kind == "filmcraft":
        rrect(3.0, 4.0, 21.0, 20.0, 1.2)
        for x in (5.0, 8.5, 12.0, 15.5, 19.0):
            frect(x - 0.9, 5.5, x + 0.9, 7.1)
            frect(x - 0.9, 16.9, x + 0.9, 18.5)
        poly([(10.0, 9.0), (15.5, 12.0), (10.0, 15.0)])
    elif kind in ("effectcraft",):
        def star(cx, cy, big):
            s = big * 0.28
            poly([(cx, cy - big), (cx + s, cy - s), (cx + big, cy), (cx + s, cy + s), (cx, cy + big), (cx - s, cy + s), (cx - big, cy), (cx - s, cy - s)])
        star(10.0, 13.0, 8.0)
        star(18.5, 5.5, 3.5)
        star(19.0, 18.5, 2.2)
    elif kind == "soundcraft":
        for i, h in enumerate([3, 7, 11, 6, 15, 9, 12, 5]):
            x = 4.5 + i * 2.15
            line([(x, 12 - h / 2), (x, 12 + h / 2)], max(1, int(u * 1.5)))
    elif kind == "anicraft":
        for i, (x, y) in enumerate([(4.5, 17.0), (8.5, 10.0), (12.5, 7.5)]):
            circ(x, y, 2.2, width=max(1, int(u * 0.9)), color=dim(0.45 + 0.2 * i))
        circ(17.0, 9.5, 3.2, filled=True)
        line([(3.0, 20.5), (21.0, 20.5)], thin)
    elif kind == "figcraft":
        for x, y, dx, dy in [(3.5, 3.5, 1, 1), (20.5, 3.5, -1, 1), (3.5, 20.5, 1, -1), (20.5, 20.5, -1, -1)]:
            line([(x + dx * 4, y), (x, y), (x, y + dy * 4)], thin)
        poly([(8.0, 6.5), (17.0, 13.0), (12.8, 13.8), (8.6, 16.6)])
        line([(12.8, 13.8), (15.3, 18.0)], max(1, int(u * 1.9)))
    elif kind == "cadcraft":
        closed([(3.5, 20.5), (3.5, 6.0), (18.0, 20.5)])
        closed([(6.5, 17.5), (6.5, 13.0), (11.0, 17.5)], thin)
        circ(16.0, 7.5, 4.0)
        circ(16.0, 7.5, 1.0, filled=True)
    elif kind == "materialcraft":
        line([(7.0, 7.0), (12.0, 12.0), (7.0, 17.0)], thin)
        line([(12.0, 12.0), (18.0, 12.0)], thin)
        for x, y, r0 in [(6.0, 6.0, 2.8), (6.0, 18.0, 2.8), (12.0, 12.0, 3.2)]:
            rrect(x - r0, y - r0, x + r0, y + r0, 1.0)
        circ(18.5, 12.0, 3.4, filled=True)
    elif kind == "texturecraft":
        circ(12, 12, 8.5)
        pts = [(12 + 3.8 * math.cos(i / 32 * math.tau), 12 + 8.5 * math.sin(i / 32 * math.tau)) for i in range(33)]
        line(pts, thin)
        line([(3.5, 12.0), (20.5, 12.0)], thin)
        poly([(12.0, 3.5), (15.6, 4.3), (18.6, 6.4), (12.0, 12.0)], dim(0.75))
    elif kind == "sketch":
        line([(3.5, 19.0), (6.5, 16.0), (9.5, 18.0), (12.5, 15.5), (15.0, 17.0)], thin)
        closed([(8.0, 13.0), (17.0, 4.0), (20.0, 7.0), (11.0, 16.0), (7.0, 17.0)])
        line([(15.0, 6.0), (18.0, 9.0)], thin)
    elif kind == "craft-anything":
        line([(8.5, 6.5), (3.5, 12.0), (8.5, 17.5)])
        line([(15.5, 6.5), (20.5, 12.0), (15.5, 17.5)])
        line([(13.8, 5.0), (10.2, 19.0)])
    else:
        for x, y in [(4.5, 4.5), (13.0, 4.5), (4.5, 13.0), (13.0, 13.0)]:
            rrect(x, y, x + 6.5, y + 6.5, 1.5)


def main():
    kind, out = sys.argv[1], sys.argv[2]
    import os
    os.makedirs(out, exist_ok=True)
    big = tile(kind, 2048).resize((1024, 1024), Image.LANCZOS)
    for s in (1024, 512, 256, 128, 64, 48, 32, 24, 16):
        big.resize((s, s), Image.LANCZOS).save(f"{out}/{kind}-{s}.png")
    big.save(f"{out}/{kind}.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])


if __name__ == "__main__":
    main()
