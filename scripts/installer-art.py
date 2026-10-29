# Genera las imágenes del instalador NSIS con el logo Vecipp:
#
#   python scripts/installer-art.py
#
# - src-tauri/windows/sidebar.bmp (164x314): páginas de bienvenida y final.
# - src-tauri/windows/header.bmp (150x57): cabecera de las demás páginas.
#
# Necesita Pillow y fontTools con brotli (pip install pillow fonttools brotli).
# Usa las mismas fuentes que la pantalla de carga (@fontsource, ya en node_modules).
import io
from pathlib import Path

from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "src-tauri" / "windows"
FONTS = ROOT / "node_modules" / "@fontsource"

NAVY = (30, 50, 70)
NAVY_DEEP = (17, 37, 53)
GREEN = (37, 193, 91)
WHITE = (255, 255, 255)
MUTED = (148, 163, 184)

# Se dibuja a 4x y se reduce para suavizar los bordes.
SCALE = 4

# Isotipo (mismas figuras que src-tauri/icons/app-icon.svg), caja 14,20 → 342,310.
ISO_BOX = (14, 20, 328, 290)
ISO_SHAPES = [
    ([(24, 64), (88, 64), (160, 240), (204, 132), (260, 132), (188, 300), (132, 300)], WHITE),
    ([(204, 112), (240, 30), (276, 112)], GREEN),
    ([(284, 128), (332, 170), (284, 212)], WHITE),
]


def font(name: str, size: int) -> ImageFont.FreeTypeFont:
    ttf = TTFont(FONTS / name.split("-latin")[0] / "files" / f"{name}.woff2")
    ttf.flavor = None
    data = io.BytesIO()
    ttf.save(data)
    data.seek(0)
    return ImageFont.truetype(data, size * SCALE)


def gradient(w: int, h: int) -> Image.Image:
    img = Image.new("RGB", (w, h))
    px = img.load()
    for y in range(h):
        t = y / (h - 1)
        color = tuple(round(a + (b - a) * t) for a, b in zip(NAVY, NAVY_DEEP))
        for x in range(w):
            px[x, y] = color
    return img


def isotype(draw: ImageDraw.ImageDraw, cx: float, top: float, width: float) -> float:
    """Dibuja el isotipo centrado en cx; devuelve la coordenada inferior."""
    k = width / ISO_BOX[2]
    left = cx - width / 2
    for points, color in ISO_SHAPES:
        draw.polygon([((left + (x - ISO_BOX[0]) * k) * SCALE, (top + (y - ISO_BOX[1]) * k) * SCALE) for x, y in points], fill=color)
    return top + ISO_BOX[3] * k


def text(draw: ImageDraw.ImageDraw, xy: tuple[float, float], value: str, fnt, fill, anchor: str, spacing: float = 0):
    x, y = xy[0] * SCALE, xy[1] * SCALE
    if not spacing:
        draw.text((x, y), value, font=fnt, fill=fill, anchor=anchor)
        return
    # Espaciado entre letras: Pillow no lo tiene, se coloca letra a letra.
    widths = [draw.textlength(c, font=fnt) for c in value]
    total = sum(widths) + spacing * SCALE * (len(value) - 1)
    cursor = x - total / 2 if anchor[0] == "m" else x
    for c, w in zip(value, widths):
        draw.text((cursor, y), c, font=fnt, fill=fill, anchor="l" + anchor[1])
        cursor += w + spacing * SCALE


def save(img: Image.Image, name: str):
    w, h = img.size
    img.resize((w // SCALE, h // SCALE), Image.LANCZOS).save(OUT / name)
    print(f"{name}: {w // SCALE}x{h // SCALE}")


def sidebar():
    w, h = 164, 314
    img = gradient(w * SCALE, h * SCALE)
    draw = ImageDraw.Draw(img)
    cx = w / 2
    bottom = isotype(draw, cx, 58, 92)
    text(draw, (cx, bottom + 22), "vecipp", font("nunito-latin-800-normal", 17), WHITE, "ms", spacing=2.4)
    draw.rectangle([(cx - 48) * SCALE, (bottom + 31) * SCALE, (cx + 48) * SCALE, (bottom + 31.6) * SCALE], fill=(205, 214, 224))
    text(draw, (cx, bottom + 68), "firma", font("great-vibes-latin-400-normal", 50), WHITE, "ms")
    draw.rectangle([0, (h - 4) * SCALE, w * SCALE, h * SCALE], fill=GREEN)
    text(draw, (cx, h - 18), "Vecipp Soluciones", font("nunito-latin-800-normal", 8), MUTED, "ms", spacing=0.6)
    save(img, "sidebar.bmp")


def header():
    w, h = 150, 57
    img = gradient(w * SCALE, h * SCALE)
    draw = ImageDraw.Draw(img)
    isotype(draw, 30, 12, 38)
    text(draw, (58, 26), "vecipp", font("nunito-latin-800-normal", 11), WHITE, "ls", spacing=1.4)
    text(draw, (60, 47), "firma", font("great-vibes-latin-400-normal", 28), WHITE, "ls")
    draw.rectangle([0, (h - 2) * SCALE, w * SCALE, h * SCALE], fill=GREEN)
    save(img, "header.bmp")


sidebar()
header()
