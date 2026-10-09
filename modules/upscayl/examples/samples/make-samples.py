"""Regenerate the example inputs. Every image here is drawn by this script, so the
samples carry no third-party rights: CC0, like this script.

    python make-samples.py        (needs Pillow)
"""
import io
import math
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent


def scene(size: int) -> Image.Image:
    """Gradients, hard edges, thin lines and small text: what an upscaler has to keep."""
    img = Image.new("RGB", (size, size))
    px = img.load()
    for y in range(size):
        for x in range(size):
            r = int(128 + 127 * math.sin(x / size * 6.283))
            g = int(128 + 127 * math.sin(y / size * 6.283 + 1.0))
            b = int(255 * (x + y) / (2 * size))
            px[x, y] = (r, g, b)
    d = ImageDraw.Draw(img)
    s = size / 256
    d.ellipse([40 * s, 40 * s, 120 * s, 120 * s], fill=(250, 250, 240), outline=(20, 20, 20), width=max(1, int(3 * s)))
    d.rectangle([140 * s, 60 * s, 220 * s, 140 * s], fill=(30, 60, 160), outline=(255, 255, 255), width=max(1, int(2 * s)))
    for i in range(12):
        x = (20 + i * 18) * s
        d.line([x, 170 * s, x + 10 * s, 240 * s], fill=(0, 0, 0), width=1)
    d.text((140 * s, 160 * s), "UPSCAYL hmod", fill=(255, 255, 255))
    d.text((140 * s, 180 * s), "0123456789", fill=(0, 0, 0))
    return img


def main() -> None:
    small = scene(256)
    small.save(HERE / "sample.png", optimize=True)
    scene(512).save(HERE / "sample-512.jpg", quality=90)

    exif = Image.Exif()
    exif[0x013B] = "HollowDeck example"            # Artist
    exif[0x8298] = "CC0 1.0"                       # Copyright
    exif[0x010E] = "copy_metadata acceptance input"  # ImageDescription
    small.save(HERE / "tagged.jpg", quality=90, exif=exif)

    batch = HERE / "batch"
    batch.mkdir(exist_ok=True)
    small.save(batch / "a.png", optimize=True)
    small.transpose(Image.Transpose.FLIP_LEFT_RIGHT).save(batch / "b.jpg", quality=90)
    # A JPEG cut off after its first 400 bytes: the engine cannot read it.
    buf = io.BytesIO()
    small.save(buf, format="JPEG", quality=90)
    (batch / "corrupt.jpg").write_bytes(buf.getvalue()[:400])


if __name__ == "__main__":
    main()
