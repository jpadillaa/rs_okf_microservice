"""Build four artificial PDF documents for the PDF adapter. Each file is written object by
object with the standard Type 1 fonts (Helvetica, Helvetica-Bold, Helvetica-Oblique and
Courier in WinAnsiEncoding), raster images as FlateDecode XObjects, bookmarks and page
labels, the way word processors and layout programs write them, so the adapter is
exercised as with a real file. The output is deterministic.

muestras/breve.pdf                  one page: title, two paragraphs and a bullet list.
                                    Expected: a single text concept, state complete.
muestras/articulo_columnas.pdf      a four-page article: title and abstract across the
                                    page, then two columns; numbered section headings
                                    with matching bookmarks; a running header and printed
                                    page numbers that start on the second page; a
                                    paragraph that continues in the next column and on
                                    the next page; words split at the end of a line (one
                                    of them a compound written whole elsewhere); a
                                    footnote; a table and a figure with its caption.
muestras/informe_tecnico.pdf        a report of several pages in one column without
                                    bookmarks: headings recognised only by their size, a
                                    header «Página N de M» and a repeated footer, numbered and
                                    bulleted lists with hanging indents, a code block in
                                    Courier, bold and italic words and a paragraph that
                                    crosses a page.
muestras/indice_columnas.pdf        a back-of-book index in two columns (plan 4, Q3):
                                    bold section letters, entries with their page
                                    numbers and indented subentries. The lines of both
                                    columns are aligned row by row, but each column is
                                    read whole, left then right, and is not a table.
muestras/escaneado_mixto.pdf        page classes: digital text with a rotated margin
                                    note, a scanned page without text, a page with only
                                    its number, a vector drawing without text and a page
                                    with text and a large photograph; internal page
                                    labels. Expected state: partial (the scanned page).

The text of each page is also written, in reading order, to exactitud/muestras/<name>.json:
the exact reference of tools/exactitud.py and of the exactness check of the tests. Running
headers, footers and page numbers are left out (the bundle records them once in
documento.md); rotated text goes at the end of its page, where the bundle writes it; a
line-end hyphen is kept only if the compound appears whole elsewhere in the document, as
the specification requires. muestras/escaneado_ocr.pdf (create_scans.py) is the image of
informe_tecnico.pdf and shares its reference.

Usage: python3 -I examples/pdf/create_samples.py
"""
from pathlib import Path
import json
import re
import zlib

OUT = Path(__file__).parent / "muestras"
TRUTH = Path(__file__).parent / "exactitud" / "muestras"

# Widths of the standard fonts (AFM, thousandths of the font size) for ASCII; other
# characters take the width of a similar one.
HELVETICA = dict(zip(
    [chr(c) for c in range(32, 127)],
    [278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
     556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
     1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
     667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
     333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
     556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584]))
HELVETICA_BOLD = dict(zip(
    [chr(c) for c in range(32, 127)],
    [278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
     556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611,
     975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
     667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556,
     333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
     611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584]))
SIMILAR = {"á": "a", "é": "e", "í": "i", "ó": "o", "ú": "u", "ñ": "n", "ü": "u", "Á": "A",
           "É": "E", "Í": "I", "Ó": "O", "Ú": "U", "Ñ": "N", "¿": "?", "¡": "!", "«": "<",
           "»": ">", "•": "-", "–": "n", "—": "M", "·": ".", "¹": "1", "²": "2", "º": "o",
           "ª": "a", "€": "C", "×": "x"}
FONTS = {"F1": ("Helvetica", HELVETICA), "F2": ("Helvetica-Bold", HELVETICA_BOLD),
         "F3": ("Helvetica-Oblique", HELVETICA), "F4": ("Courier", None)}


def width(text, font, size):
    table = FONTS[font][1]
    if table is None:
        return 0.6 * size * len(text)
    return sum(table.get(SIMILAR.get(c, c), 556) for c in text) * size / 1000


def pdf_string(text):
    raw = text.encode("cp1252")
    return b"(" + raw.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)") + b")"


def wrap(text, font, size, max_width):
    """Lines of `text` that fit `max_width`; a word ending in «-» marks a forced split."""
    lines, current = [], ""
    for word in text.split(" "):
        candidate = word if not current else current + " " + word
        if current and width(candidate, font, size) > max_width:
            lines.append(current)
            current = word
        else:
            current = candidate
        if current.endswith("-|"):
            lines.append(current[:-1])
            current = ""
    if current:
        lines.append(current)
    return lines


class Page:
    def __init__(self, width=612, height=792):
        self.width, self.height = width, height
        self.ops = []
        self.images = []
        # Text of the page in reading order (the order of the calls), and rotated text.
        self.truth = []
        self.rotated_truth = []

    def text(self, x, top, text, font="F1", size=10, furniture=False, truth_at=None):
        """Text whose top is `top` points below the upper edge. `furniture` marks a
        running header, footer or page number, which the reference leaves out; `truth_at`
        places the text elsewhere in the reference (a footnote after the paragraph that
        calls it, where the specification puts it)."""
        base = self.height - top - 0.8 * size
        self.ops.append(b"BT /%s %g Tf %.2f %.2f Td %s Tj ET" % (font.encode(), size, x, base, pdf_string(text)))
        if furniture:
            return
        if truth_at is None:
            self.truth.append(text)
        else:
            self.truth.insert(truth_at, text)

    def runs(self, x, top, runs, size=10):
        """A line written in several fonts: [(font, text), ...]."""
        base = self.height - top - 0.8 * size
        parts = [b"BT %.2f %.2f Td" % (x, base)]
        for font, text in runs:
            parts.append(b"/%s %g Tf %s Tj" % (font.encode(), size, pdf_string(text)))
        parts.append(b"ET")
        self.ops.append(b" ".join(parts))
        self.truth.append("".join(text for _, text in runs))

    def paragraph(self, x, top, text, max_width, font="F1", size=10, leading=1.25, indent=0):
        lines = wrap(text, font, size, max_width - indent)
        for k, line in enumerate(lines):
            self.text(x + (indent if k == 0 else 0), top + k * size * leading, line, font, size)
        return top + len(lines) * size * leading

    def rotated(self, x, y, text, font="F1", size=8):
        self.ops.append(b"BT /%s %g Tf 0 1 -1 0 %.2f %.2f Tm %s Tj ET" % (font.encode(), size, x, y, pdf_string(text)))
        self.rotated_truth.append(text)

    def rect(self, x, top, w, h, gray):
        self.ops.append(b"%.2f g %.2f %.2f %.2f %.2f re f 0 g" % (gray, x, self.height - top - h, w, h))

    def line(self, x0, top0, x1, top1, w=1.0):
        self.ops.append(b"%.2f w %.2f %.2f m %.2f %.2f l S" % (w, x0, self.height - top0, x1, self.height - top1))

    def image(self, x, top, w, h, pixels, pw, ph):
        name = b"Im%d" % (len(self.images) + 1)
        self.images.append((name, pixels, pw, ph))
        self.ops.append(b"q %.2f 0 0 %.2f %.2f %.2f cm /%s Do Q" % (w, h, x, self.height - top - h, name))


def write_truth(names, pages, blocks=()):
    """Reference text of each page with text, in reading order, for every name in `names`,
    and the structural blocks of the document in order: {"page", "type": "title" |
    "heading" (with "level", 1 for the highest below the title) | "list" ("ordered",
    "items" with "label" and "text") | "table" ("rows" of cells) | "code" ("lines")}."""
    lines = [page.truth + page.rotated_truth for page in pages]
    whole = "\n".join(line for page in lines for line in page)

    def join(match):
        left, right = match.group(1), match.group(2)
        return f"{left}-{right}" if f"{left}-{right}" in whole.replace(match.group(0), "") else left + right

    texts = {}
    for k, page in enumerate(lines, 1):
        text = re.sub(r"(\w+)-\n(\w+)", join, "\n".join(page))
        if text.strip():
            texts[str(k)] = text
    TRUTH.mkdir(parents=True, exist_ok=True)
    for name in names:
        data = {
            "document": f"examples/pdf/muestras/{name}.pdf",
            "source": "examples/pdf/create_samples.py",
            "pages": texts,
            "blocks": list(blocks),
        }
        (TRUTH / f"{name}.json").write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def gradient(pw, ph, seed):
    rows = []
    for y in range(ph):
        row = bytearray()
        for x in range(pw):
            row += bytes(((x * 255 // pw + seed) % 256, (y * 255 // ph) % 256, (seed * 3 + x + y) % 256))
        rows.append(bytes(row))
    return b"".join(rows)


def scan(pw, ph):
    """A grey page with dark strokes, as a scanner would produce."""
    data = bytearray()
    for y in range(ph):
        for x in range(pw):
            v = 235
            if y % 14 in (6, 7) and 30 < x < pw - 30 and (x // 9 + y // 14) % 7 != 0:
                v = 60
            data += bytes((v, v, v))
    return bytes(data)


class Pdf:
    def __init__(self):
        self.objects = []

    def reserve(self):
        self.objects.append(None)
        return len(self.objects)

    def set(self, num, body):
        self.objects[num - 1] = body

    def add(self, body):
        num = self.reserve()
        self.set(num, body)
        return num

    def stream(self, entries, data):
        data = zlib.compress(data, 9)
        return self.add(b"<< %s /Filter /FlateDecode /Length %d >>\nstream\n" % (entries, len(data)) + data + b"\nendstream")

    def save(self, path, pages, info, outline=None, labels=None):
        fonts = {}
        for key, (base, _) in FONTS.items():
            fonts[key] = self.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /%s /Encoding /WinAnsiEncoding >>" % base.encode())
        font_dict = b" ".join(b"/%s %d 0 R" % (k.encode(), n) for k, n in fonts.items())
        tree = self.reserve()
        page_nums = []
        for page in pages:
            xobjects = []
            for name, pixels, pw, ph in page.images:
                n = self.stream(b"/Type /XObject /Subtype /Image /Width %d /Height %d /ColorSpace /DeviceRGB /BitsPerComponent 8" % (pw, ph), pixels)
                xobjects.append(b"/%s %d 0 R" % (name, n))
            content = self.stream(b"", b"\n".join(page.ops))
            resources = b"<< /Font << %s >>%s >>" % (font_dict, b" /XObject << %s >>" % b" ".join(xobjects) if xobjects else b"")
            page_nums.append(self.add(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 %g %g] /Resources %s /Contents %d 0 R >>" % (tree, page.width, page.height, resources, content)))
        kids = b" ".join(b"%d 0 R" % n for n in page_nums)
        self.set(tree, b"<< /Type /Pages /Kids [%s] /Count %d >>" % (kids, len(page_nums)))
        catalog = b"<< /Type /Catalog /Pages %d 0 R" % tree
        if outline:
            catalog += b" /Outlines %d 0 R /PageMode /UseOutlines" % self.outline(outline, page_nums, pages)
        if labels:
            catalog += b" /PageLabels << /Nums [%s] >>" % labels
        root = self.add(catalog + b" >>")
        info_entries = b" ".join(b"/%s %s" % (k.encode(), pdf_string(v)) for k, v in info.items())
        info_num = self.add(b"<< %s /CreationDate (D:20261101100000Z) /Producer (create_samples.py) >>" % info_entries)
        out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
        offsets = []
        for num, body in enumerate(self.objects, 1):
            offsets.append(len(out))
            out += b"%d 0 obj\n" % num + body + b"\nendobj\n"
        xref = len(out)
        out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(self.objects) + 1)
        for off in offsets:
            out += b"%010d 00000 n \n" % off
        out += b"trailer\n<< /Size %d /Root %d 0 R /Info %d 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(self.objects) + 1, root, info_num, xref)
        path.write_bytes(bytes(out))

    def outline(self, items, page_nums, pages):
        """`items`: [(title, page_index, top, [children...]), ...]."""
        root = self.reserve()

        def build(entries, parent):
            nums = [self.reserve() for _ in entries]
            for k, (title, page, top, children) in enumerate(entries):
                y = pages[page].height - top
                body = b"<< /Title %s /Parent %d 0 R /Dest [%d 0 R /XYZ 0 %g 0]" % (pdf_string(title), parent, page_nums[page], y)
                if k > 0:
                    body += b" /Prev %d 0 R" % nums[k - 1]
                if k + 1 < len(nums):
                    body += b" /Next %d 0 R" % nums[k + 1]
                if children:
                    first, last = build(children, nums[k])
                    body += b" /First %d 0 R /Last %d 0 R /Count %d" % (first, last, len(children))
                self.set(nums[k], body + b" >>")
            return nums[0], nums[-1]

        first, last = build(items, root)
        self.set(root, b"<< /Type /Outlines /First %d 0 R /Last %d 0 R /Count %d >>" % (first, last, len(items)))
        return root


def breve():
    p = Page()
    p.text(72, 72, "Préstamo de equipos audiovisuales", "F2", 18)
    y = p.paragraph(72, 110, "La biblioteca presta cámaras, micrófonos y trípodes a estudiantes y profesores para actividades académicas. El préstamo dura como máximo tres días hábiles y se renueva una sola vez si nadie más ha reservado el equipo.", 468)
    y = p.paragraph(72, y + 10, "Para solicitar un equipo se presenta el carné vigente en el mostrador de servicios. Antes de entregarlo, el personal revisa con el usuario que el equipo funcione y que tenga todos sus accesorios.", 468)
    for item in ["Las reservas se hacen con dos días de anticipación.",
                 "Los daños se reportan el mismo día de la devolución.",
                 "Las devoluciones tardías suspenden el servicio una semana."]:
        p.text(84, y + 8, "•", "F1", 10)
        y = p.paragraph(98, y + 8, item, 440) - 4
    Pdf().save(OUT / "breve.pdf", [p], {"Title": "Préstamo de equipos", "Author": "Biblioteca de prueba"})
    write_truth(["breve"], [p], [
        {"page": 1, "type": "title", "text": "Préstamo de equipos audiovisuales"},
        {"page": 1, "type": "list", "ordered": False, "items": [
            {"label": "•", "text": t} for t in [
                "Las reservas se hacen con dos días de anticipación.",
                "Los daños se reportan el mismo día de la devolución.",
                "Las devoluciones tardías suspenden el servicio una semana."]]},
    ])


ARTICLE_HEADER = "Revista de Pruebas Documentales · Vol. 3 · 2026"


def articulo():
    W, H = 595, 842
    left, gutter, col = 56, 20, (595 - 112 - 20) / 2
    right = left + col + gutter
    pages = [Page(W, H) for _ in range(4)]
    # Página 1: portada del artículo y comienzo de las dos columnas.
    p = pages[0]
    p.text(left, 70, "Lectura de documentos con varias columnas:", "F2", 20)
    p.text(left, 96, "un estudio con materiales artificiales", "F2", 20)
    p.text(left, 128, "Ana Ruiz¹ y Tomás Peña²", "F1", 11)
    p.text(left, 160, "Resumen", "F2", 11)
    y = p.paragraph(left, 176, "Este artículo describe un experimento ficticio sobre la conversión de documentos maquetados en dos columnas. Se compararon dos procedimientos de extracción con un conjunto de materiales creados para la prueba y se midió cuántos párrafos conservaban su orden de lectura original.", W - 2 * left, size=10)
    top = y + 24
    p.text(left, top, "1. Introducción", "F2", 12)
    intro = ("Los documentos académicos suelen maquetarse en dos columnas para aprovechar mejor el espacio de la página. Esa disposición facilita la lectura en papel, pero complica la conversión automática porque las líneas de una columna y las de la otra comparten la misma altura y un lector ingenuo las mezcla. "
             "En este trabajo se estudia cómo reconstruir el orden de lectura a partir de la posición de cada línea y cómo reunir los párrafos que la maquetación parte entre columnas o entre páginas, de modo que el texto conserve su")
    lines = wrap(intro, "F1", 10, col)
    split = 14
    yy = top + 20
    for line in lines[:split]:
        p.text(left, yy, line, "F1", 10)
        yy += 12.5
    yy = top + 20
    for line in lines[split:]:
        p.text(right, yy, line, "F1", 10)
        yy += 12.5
    # El párrafo sigue en la columna derecha, en minúscula.
    rest = "sentido. El análisis de costo-beneficio de cada procedimiento se resume al final del artículo, junto con las limitaciones del estudio."
    yy = p.paragraph(right, yy, rest, col)
    p.text(right, yy + 14, "2. Método", "F2", 12)
    metodo = "Se prepararon veinte documentos artificiales con columnas, tablas, figuras y notas al pie. Cada documento se convirtió con los dos procedimientos y un revisor comparó el resultado con el original, párrafo por párrafo, anotando cualquier"
    yy = p.paragraph(right, yy + 34, metodo, col)
    p.text(left, H - 60, "¹ Universidad Ficticia del Norte.", "F1", 7)
    p.text(left, H - 51, "² Instituto Imaginario de Documentos.", "F1", 7)
    # Página 2: continúa el método (desde la página anterior) y resultados.
    p = pages[1]
    cont = ("diferencia en el orden o en el contenido. La conver-| sión se consideró correcta cuando todos los párrafos aparecían completos y en su orden. "
            "Para cada documento se anotó también el tiempo de proceso y la memoria utilizada, con el fin de estimar el costo-| beneficio de cada procedimiento.")
    yy = p.paragraph(left, 60, cont, col)
    p.text(left, yy + 14, "3. Resultados", "F2", 12)
    res = "El primer procedimiento conservó el orden de lectura en diecisiete de los veinte documentos; el segundo, en todos ellos. Las diferencias se concentraron en las páginas con notas al pie y en las tablas que ocupaban una sola columna, donde el primer procedimiento leía las celdas como si fueran texto corrido.¹"
    yy = p.paragraph(left, yy + 34, res, col)
    called = len(p.truth)
    yy = p.paragraph(right, 60, "La tabla 1 resume los resultados por procedimiento. Las cifras son artificiales y solo ilustran el formato de una tabla sencilla con encabezado.", col)
    rows = [("Procedimiento", "Correctos", "Tiempo"), ("Primero", "17", "1,2 s"), ("Segundo", "20", "1,9 s"), ("Diferencia", "3", "0,7 s")]
    yy += 14
    p.text(right, yy, "Tabla 1. Resultados por procedimiento", "F2", 9)
    yy += 18
    for r, row in enumerate(rows):
        for c, cell in enumerate(row):
            p.text(right + c * 80, yy, cell, "F2" if r == 0 else "F1", 9)
        yy += 14
    yy += 10
    p.image(right, yy, col, 110, gradient(240, 110, 40), 240, 110)
    yy += 116
    yy = p.paragraph(right, yy, "Figura 1. Distribución artificial de los tiempos de proceso.", col, "F3", 9)
    p.text(left, H - 70, "¹ Los veinte documentos se crearon solo para esta prueba.", "F1", 7, truth_at=called)
    # Página 3: conclusiones.
    p = pages[2]
    p.text(left, 60, "4. Conclusiones", "F2", 12)
    concl = "Reconstruir el orden de lectura a partir de la posición de las líneas permite convertir documentos en dos columnas sin mezclar su contenido. Las notas al pie y las tablas siguen siendo los casos más delicados y conviene declararlos en el informe de conversión."
    yy = p.paragraph(left, 80, concl, col)
    yy = p.paragraph(right, 80, "Como trabajo futuro se propone repetir la prueba con documentos escaneados, que exigen reconocimiento óptico de caracteres antes de cualquier análisis de la disposición.", col)
    # Página 4: referencias.
    p = pages[3]
    p.text(left, 60, "Referencias", "F2", 12)
    yy = 82
    for k, ref in enumerate(["Ruiz A. Documentos de prueba para conversores. Rev Pruebas Doc. 2025;2(1):1–10.",
                             "Peña T. Orden de lectura en páginas con columnas. Cuad Doc. 2024;7:33–41.",
                             "Gómez L, Ruiz A. Tablas sencillas en archivos PDF. Rev Pruebas Doc. 2026;3(1):5–9."], 1):
        p.text(left, yy, f"{k}.", "F1", 9)
        yy = p.paragraph(left + 16, yy, ref, W - 2 * left - 16, size=9) + 4
    for k, page in enumerate(pages[1:], 2):
        page.text(left, 30, ARTICLE_HEADER, "F3", 8, furniture=True)
        page.text(W / 2 - 3, H - 34, str(k - 1), "F1", 9, furniture=True)
    outline = [("Lectura de documentos con varias columnas: un estudio con materiales artificiales", 0, 70, [
        ("Resumen", 0, 160, []), ("1. Introducción", 0, top, []), ("2. Método", 0, 400, []),
        ("3. Resultados", 1, 100, []), ("4. Conclusiones", 2, 60, []), ("Referencias", 3, 60, [])])]
    Pdf().save(OUT / "articulo_columnas.pdf", pages, {"Title": "Lectura de documentos con varias columnas", "Author": "Ana Ruiz; Tomás Peña"}, outline)
    references = ["Ruiz A. Documentos de prueba para conversores. Rev Pruebas Doc. 2025;2(1):1–10.",
                  "Peña T. Orden de lectura en páginas con columnas. Cuad Doc. 2024;7:33–41.",
                  "Gómez L, Ruiz A. Tablas sencillas en archivos PDF. Rev Pruebas Doc. 2026;3(1):5–9."]
    write_truth(["articulo_columnas"], pages, [
        {"page": 1, "type": "title", "text": "Lectura de documentos con varias columnas: un estudio con materiales artificiales"},
        *({"page": page, "type": "heading", "level": 1, "text": text} for page, text in [
            (1, "Resumen"), (1, "1. Introducción"), (1, "2. Método"), (2, "3. Resultados")]),
        {"page": 2, "type": "table", "rows": [list(r) for r in rows]},
        {"page": 3, "type": "heading", "level": 1, "text": "4. Conclusiones"},
        {"page": 4, "type": "heading", "level": 1, "text": "Referencias"},
        {"page": 4, "type": "list", "ordered": True, "items": [
            {"label": str(k), "text": ref} for k, ref in enumerate(references, 1)]},
    ])


def informe():
    pages = [Page() for _ in range(5)]
    x, w = 72, 468
    flow = [
        ("h1", "Informe técnico de migración"),
        ("p", "Este informe describe la migración ficticia de un sistema de inventario a una nueva plataforma. Todos los nombres, cifras y fechas son artificiales y solo sirven para probar la conversión de documentos."),
        ("h2", "1. Contexto"),
        ("p", "El sistema actual funciona desde hace ocho años en un servidor propio. Su base de datos crece un veinte por ciento cada año y las copias de seguridad tardan más de seis horas, lo que obliga a programarlas en fines de semana."),
        ("rich", [("F1", "La migración es "), ("F2", "importante"), ("F1", " porque el contrato de soporte vence en marzo y "), ("F3", "no se renovará"), ("F1", ".")]),
        ("p", "Los usuarios principales son el almacén central, las tres sedes regionales y el área de compras, que consultan el inventario a diario y registran cerca de dos mil movimientos por semana."),
        ("h2", "2. Arquitectura"),
        ("p", "La nueva plataforma separa la aplicación en tres servicios: catálogo, existencias y reportes. Cada servicio tiene su propia base de datos y se comunica con los demás mediante mensajes."),
        ("p", "La separación permite actualizar cada servicio por su cuenta y escalar solo el que recibe más carga. A cambio, exige definir con cuidado los mensajes que intercambian los servicios, porque un cambio en su formato obliga a coordinar las versiones de todos los que los consumen. Para reducir ese riesgo, cada mensaje lleva un número de versión y los servicios aceptan durante un tiempo la versión anterior y la nueva."),
        ("p", "La plataforma registra cada operación en un historial que no se puede modificar. El historial sirve para auditar los cambios de existencias, para reconstruir el saldo de cualquier día y para detectar diferencias entre el inventario físico y el registrado en el sistema."),
        ("ol", ["El servicio de catálogo guarda la descripción de cada producto, sus códigos y sus proveedores, y publica un mensaje cada vez que cambia un producto.",
                "El servicio de existencias registra entradas y salidas por sede y calcula el saldo disponible en tiempo real.",
                "El servicio de reportes consolida los datos de los otros dos servicios cada noche."]),
        ("p", "La configuración mínima de cada servicio se describe con un archivo como el siguiente:"),
        ("code", ["servicio: existencias", "replicas: 2", "base_de_datos:", "  motor: postgres", "  version: 16"]),
        ("h2", "3. Plan de migración"),
        ("p", "La migración se hará en dos fases para limitar el riesgo. Durante todo el proceso el sistema actual seguirá disponible en modo de solo lectura, de modo que las sedes puedan consultar el inventario aunque no registren movimientos nuevos durante las ventanas de mantenimiento programadas por el equipo de infraestructura con las sedes regionales y con el área de compras, que deberá aprobar cada ventana con al menos una semana de anticipación para reorganizar sus pedidos."),
        ("h3", "3.1 Fase de preparación"),
        ("ul", ["Inventario de datos y limpieza de registros duplicados.",
                "Pruebas de carga con una copia anonimizada de la base de datos.",
                "Capacitación de los responsables de cada sede."]),
        ("h3", "3.2 Fase de ejecución"),
        ("p", "La ejecución empieza un viernes por la noche con la copia final de los datos y termina el domingo con la verificación de saldos en cada sede. Si la verificación falla, se vuelve al sistema anterior siguiendo el procedimiento de reversión."),
        ("h2", "4. Riesgos"),
        ("p", "El principal riesgo es la pérdida de movimientos registrados durante la copia final. Para mitigarlo, el sistema anterior bloqueará los registros nuevos una hora antes de empezar y la nueva plataforma comparará los totales por producto antes de abrir el servicio."),
        ("p", "Un segundo riesgo es la resistencia al cambio en las sedes. El plan de capacitación incluye sesiones prácticas y una guía breve que cada responsable podrá consultar durante las primeras semanas de uso."),
    ]
    sizes = {"h1": 20, "h2": 14, "h3": 12}
    page, y = 0, 72
    blocks = []
    bottom = 792 - 90

    def need(h):
        nonlocal page, y
        if y + h > bottom:
            page += 1
            y = 72

    for kind, value in flow:
        p = pages[page]
        if kind in sizes:
            need(sizes[kind] * 3)
            p = pages[page]
            y += 8 if y > 72 else 0
            p.text(x, y, value, "F2", sizes[kind])
            blocks.append({"page": page + 1, "type": "title", "text": value} if kind == "h1"
                          else {"page": page + 1, "type": "heading", "level": int(kind[1]) - 1, "text": value})
            y += sizes[kind] * 1.6
        elif kind == "p":
            lines = wrap(value, "F1", 10.5, w)
            for line in lines:
                need(14)
                pages[page].text(x, y, line, "F1", 10.5)
                y += 13.5
            y += 6
        elif kind == "rich":
            need(14)
            p.runs(x, y, value, 10.5)
            y += 19.5
        elif kind in ("ol", "ul"):
            blocks.append({"page": page + 1, "type": "list", "ordered": kind == "ol", "items": [
                {"label": f"{k}." if kind == "ol" else "–", "text": item} for k, item in enumerate(value, 1)]})
            for k, item in enumerate(value, 1):
                label = f"{k}." if kind == "ol" else "–"
                lines = wrap(item, "F1", 10.5, w - 24)
                need(14 * len(lines))
                pages[page].text(x + 6, y, label, "F1", 10.5)
                for line in lines:
                    pages[page].text(x + 24, y, line, "F1", 10.5)
                    y += 13.5
                y += 3
            y += 6
        elif kind == "code":
            need(12 * len(value))
            blocks.append({"page": page + 1, "type": "code", "lines": list(value)})
            for line in value:
                pages[page].text(x + 12, y, line, "F4", 9)
                y += 11.5
            y += 8
    used = pages[:page + 1]
    for k, p in enumerate(used, 1):
        label = f"Informe técnico — Página {k} de {len(used)}"
        p.text(612 - 72 - width(label, "F1", 8), 36, label, "F1", 8, furniture=True)
        p.text(72, 792 - 48, "Documento de prueba sin datos reales", "F3", 8, furniture=True)
    Pdf().save(OUT / "informe_tecnico.pdf", used, {"Title": "Informe técnico de migración", "Author": "Equipo de pruebas"})
    # escaneado_ocr.pdf (create_scans.py) es la imagen de estas mismas páginas.
    write_truth(["informe_tecnico", "escaneado_ocr"], used, blocks)


def escaneado():
    pages = [Page() for _ in range(5)]
    p = pages[0]
    p.text(72, 72, "Expediente digitalizado", "F2", 18)
    y = p.paragraph(72, 110, "Este expediente reúne una página escrita directamente en el computador, una página escaneada sin reconocimiento de texto, una página en blanco, un diagrama sin texto y una página con una fotografía. Sirve para comprobar la clasificación de las páginas.", 468)
    p.paragraph(72, y + 10, "La segunda página es la imagen de un documento en papel; su contenido no puede transcribirse sin reconocimiento óptico de caracteres.", 468)
    p.rotated(40, 300, "Copia controlada · no imprimir", "F1", 8)
    # Página 2: escaneo sin capa de texto.
    pages[1].image(30, 30, 552, 732, scan(276, 366), 276, 366)
    # Página 4: diagrama vectorial.
    p = pages[3]
    for k in range(3):
        p.rect(90 + k * 160, 200, 120, 70, 0.8 - 0.2 * k)
        if k < 2:
            p.line(210 + k * 160, 235, 250 + k * 160, 235, 2)
    p.rect(250, 380, 120, 70, 0.5)
    p.line(310, 270, 310, 380, 2)
    # Página 5: texto con una fotografía grande.
    p = pages[4]
    p.text(72, 72, "Registro fotográfico", "F2", 14)
    p.paragraph(72, 100, "La fotografía muestra el estado del archivo antes de la digitalización. Se conserva como imagen adjunta.", 468)
    p.image(72, 150, 468, 360, gradient(234, 180, 90), 234, 180)
    for k in (1, 3, 4, 5):
        pages[k - 1].text(300, 792 - 40, str(k), "F1", 9, furniture=True)
    Pdf().save(OUT / "escaneado_mixto.pdf", pages, {"Title": "Expediente digitalizado"}, labels=b"0 << /S /D /P (Exp-) >>")
    write_truth(["escaneado_mixto"], pages, [
        {"page": 1, "type": "title", "text": "Expediente digitalizado"},
        {"page": 5, "type": "heading", "level": 1, "text": "Registro fotográfico"},
    ])


# Index columns: ("#", letter) opens a section, ("", text) is an entry at the margin and
# ("-", text) an indented subentry. The sections of the two columns start at different
# heights, as in a real index.
INDEX_LEFT = [
    ("#", "A"), ("", "acceso abierto 12"), ("-", "licencias 14"), ("-", "repositorios 15–17"),
    ("", "actas de congreso 21"), ("", "anexos 30"), ("-", "numeración 31"),
    ("-", "tablas en anexos 33"),
    ("#", "B"), ("", "bibliografía 40"), ("-", "estilos de cita 41"), ("-", "orden alfabético 43"),
    ("", "boletines 47"), ("", "búsqueda de texto 52"), ("-", "por palabras 53"),
    ("-", "por frases exactas 55"),
]
INDEX_RIGHT = [
    ("#", "C"), ("", "cabeceras de página 60"), ("-", "folios 61"), ("-", "títulos corrientes 62"),
    ("", "capítulos 64"), ("-", "numeración 65"), ("", "columnas 70"), ("-", "de texto corrido 71"),
    ("-", "de tablas 73"), ("", "cuadros 75"),
    ("#", "D"), ("", "diagramas 80"), ("-", "rótulos 81"), ("", "documentos escaneados 85"),
    ("-", "reconocimiento de caracteres 86"), ("-", "resolución 88"),
]


def indice():
    W, H = 595, 842
    left, gutter = 56, 24
    right = left + (W - 2 * left - gutter) / 2 + gutter
    p = Page(W, H)
    p.text(left, 60, "Índice analítico", "F2", 18)
    for x, column in ((left, INDEX_LEFT), (right, INDEX_RIGHT)):
        y = 110
        for kind, text in column:
            if kind == "#":
                y += 8
                p.text(x, y, text, "F2", 12)
                y += 16
            else:
                p.text(x + (12 if kind == "-" else 0), y, text, "F1", 10)
                y += 13
    Pdf().save(OUT / "indice_columnas.pdf", [p], {"Title": "Índice analítico"})
    write_truth(["indice_columnas"], [p], [
        {"page": 1, "type": "title", "text": "Índice analítico"},
        *({"page": 1, "type": "heading", "level": 1, "text": t}
          for kind, t in INDEX_LEFT + INDEX_RIGHT if kind == "#"),
    ])


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    breve()
    articulo()
    informe()
    indice()
    escaneado()
