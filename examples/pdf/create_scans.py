# /// script
# requires-python = ">=3.11"
# dependencies = ["pypdfium2==5.14.0", "pillow==11.3.0"]
# ///
"""Build the scanned sample for the OCR of the PDF adapter.

muestras/escaneado_ocr.pdf          the two pages of muestras/informe_tecnico.pdf as a
                                    scanner would deliver them: image only, no text layer,
                                    150 DPI greyscale, tilted 0.8°, with grain, a slight
                                    blur and JPEG compression. Without OCR every page is a
                                    scan without text (state partial); with OCR the text,
                                    the running header and footer and the headings come
                                    back from the image.

The source is a synthetic document, so the sample carries no private text. The noise
comes from a seeded generator and the PDF dates are fixed, so the output is
deterministic for the pinned versions.

Usage: uv run examples/pdf/create_scans.py
"""

import io
import random
from pathlib import Path

import pypdfium2 as pdfium
from PIL import Image, ImageFilter

HERE = Path(__file__).resolve().parent
SOURCE = HERE / "muestras" / "informe_tecnico.pdf"
OUT = HERE / "muestras" / "escaneado_ocr.pdf"
DPI = 150
FIXED_DATE = "D:20261106000000Z"


def scanned(page, rng):
    img = page.render(scale=DPI / 72, grayscale=True).to_pil().convert("L")
    img = img.rotate(0.8, resample=Image.BICUBIC, fillcolor=255)
    # Paper grain: a grey cast with small random variations.
    noise = bytes(min(255, max(0, 232 + int(rng.gauss(0, 6)))) for _ in range(img.width * img.height))
    paper = Image.frombytes("L", img.size, noise)
    img = Image.composite(img, paper, img.point(lambda v: 255 if v < 200 else 0))
    img = img.filter(ImageFilter.GaussianBlur(0.4))
    buf = io.BytesIO()
    img.save(buf, "JPEG", quality=55)
    return Image.open(io.BytesIO(buf.getvalue())).convert("L")


def main():
    rng = random.Random(7)
    doc = pdfium.PdfDocument(SOURCE)
    pages = [scanned(doc[i], rng) for i in range(len(doc))]
    pages[0].save(
        OUT,
        "PDF",
        resolution=DPI,
        save_all=True,
        append_images=pages[1:],
        title="Informe técnico escaneado",
        producer="create_scans.py",
        creationDate=FIXED_DATE,
        modDate=FIXED_DATE,
    )
    print(OUT.relative_to(HERE.parent.parent), len(pages), "páginas")


if __name__ == "__main__":
    main()
