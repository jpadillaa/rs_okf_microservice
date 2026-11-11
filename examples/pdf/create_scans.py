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
muestras/imagenes_con_texto.pdf     a digital page with images: a capture of the table
                                    of articulo_columnas.pdf, a capture of the code block
                                    of informe_tecnico.pdf and a photograph without text.
                                    Its exact reference (exactitud/muestras/) includes
                                    the text of the captures, which only the OCR of
                                    images recovers.

The source is a synthetic document, so the sample carries no private text. The noise
comes from a seeded generator and the PDF dates are fixed, so the output is
deterministic for the pinned versions.

Usage: uv run examples/pdf/create_scans.py
"""

import io
import random
import sys
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


def capture(path, page, first, last, scale=2.0, margin=8):
    """RGB pixels of the region of `page` (from 0) of `path` that spans from the first
    occurrence of `first` to the end of `last`, rendered at `scale`."""
    doc = pdfium.PdfDocument(path)
    try:
        p = doc[page]
        text = p.get_textpage()
        height = p.get_height()
        boxes = []
        for needle in (first, last):
            found = text.search(needle).get_next()
            assert found, f"{needle} no está en {path}"
            start, count = found
            boxes += [text.get_charbox(i) for i in range(start, start + count)]
        # La región va de la primera palabra a la última y abarca todo lo que hay entre
        # ellas a lo ancho de la columna de ambas.
        left = min(b[0] for b in boxes) - 4
        right = max(b[2] for b in boxes) + 60
        top = height - max(b[3] for b in boxes) - 4
        bottom = height - min(b[1] for b in boxes) + 4
        image = p.render(scale=scale).to_pil().convert("RGB")
        crop = (int(left * scale) - margin, int(top * scale) - margin,
                int(right * scale) + margin, int(bottom * scale) + margin)
        return image.crop(crop)
    finally:
        doc.close()


def images_with_text():
    """A digital page with two captures with text and a photograph without text."""
    sys.path.insert(0, str(HERE))
    import create_samples as cs

    table = capture(HERE / "muestras" / "articulo_columnas.pdf", 1, "Tabla 1.", "0,7 s")
    code = capture(HERE / "muestras" / "informe_tecnico.pdf", 0, "servicio: existencias", "version: 16")
    p = cs.Page()
    p.text(72, 72, "Anexo de capturas", "F2", 18)
    y = p.paragraph(72, 110, "Este anexo reúne capturas de pantalla de otros documentos de prueba. Su texto solo está en las imágenes.", 468)
    for image, lines in [
        (table, ["Tabla 1. Resultados por procedimiento", "Procedimiento Correctos Tiempo",
                 "Primero 17 1,2 s", "Segundo 20 1,9 s", "Diferencia 3 0,7 s"]),
        (code, ["servicio: existencias", "replicas: 2", "base_de_datos:", "motor: postgres", "version: 16"]),
    ]:
        w = 300
        h = w * image.height / image.width
        p.image(72, y + 12, w, h, image.tobytes(), image.width, image.height)
        p.truth += lines
        y += 12 + h
    y = p.paragraph(72, y + 16, "La fotografía siguiente no contiene texto.", 468)
    p.image(72, y + 12, 240, 120, cs.gradient(240, 120, 70), 240, 120)
    out = HERE / "muestras" / "imagenes_con_texto.pdf"
    cs.Pdf().save(out, [p], {"Title": "Anexo de capturas"})
    cs.write_truth(["imagenes_con_texto"], [p], [
        {"page": 1, "type": "title", "text": "Anexo de capturas"},
    ])
    print(out.relative_to(HERE.parent.parent), "1 página")


if __name__ == "__main__":
    main()
    images_with_text()
