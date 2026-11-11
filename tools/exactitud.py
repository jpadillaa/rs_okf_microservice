# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "openpyxl==3.1.5",
#     "pypdfium2==5.14.0",
#     "python-docx==1.2.0",
#     "python-pptx==1.0.2",
#     "pillow==11.3.0",
#     "pyyaml==6.0.3",
# ]
# ///
"""Exactitud del texto de un bundle PDF, página por página: no solo qué palabras llegan
(completitud), sino si llegan en su orden y sin mezclarse con otras.

Para cada página se compara la secuencia de palabras de la referencia con la de los
bloques del bundle cuyo localizador cubre esa página, en el orden del Markdown (con el
título de su concepto y el del documento):

    completitud     palabras de la referencia presentes en la página del bundle;
    orden           de esas palabras, cuántas están en el orden de la referencia
                    (subsecuencia común más larga): baja si un bloque o una columna sale
                    de su sitio;
    orden interno   lo mismo dentro de cada bloque del bundle, frente a la referencia de
                    las páginas que ocupa y sin contar dónde está el bloque: baja si un
                    párrafo, una celda o una línea mezcla texto de sitios distintos o lo
                    desordena (se cuenta en la primera página del bloque);
    contigüidad     pares de palabras consecutivas de la referencia que el bundle conserva
                    juntos: baja si se intercalan líneas o celdas.

Las notas del conversor (`<!-- okf-generated -->`), sus rótulos, los marcadores y los
destinos de los enlaces no cuentan como texto del original.

Referencia. Si existe `examples/pdf/exactitud/<grupo>/<nombre>.json` (o el archivo de
`--reference`), es exacta: el texto de cada página en orden de lectura, sin encabezados
ni pies; solo se miden sus páginas. Las de las muestras públicas las escribe
`examples/pdf/create_samples.py` al generar cada PDF. Para otro PDF, la referencia es el
texto de pdfium, aproximada: sigue el flujo de contenido (deja, por ejemplo, los rótulos
de una figura al final de la página), por lo que su «orden» no prueba un error; el orden
interno y la contigüidad, que no dependen de dónde está cada bloque, sí son
significativos. Las palabras que pdfium parte al final de línea se reúnen con la regla de
la especificación, y las ausentes que el bundle registra en `documento.md` (encabezados y
pies) se retiran de la referencia, empezando por los bordes de la página.

Formato de una referencia: {"document": ..., "pages": {"1": "texto...", ...}}. Puede
cubrir solo algunas páginas; así se revisan a mano las páginas difíciles de un original
privado, que se guarda fuera del repositorio.

Uso:
    uv run tools/exactitud.py [--binary RUTA] [--bundle ZIP] [--reference JSON]
                              [--json SALIDA] [--worst N] DOC...
"""

import argparse
import collections
import difflib
import json
import re
import sys
import tempfile
import zipfile
from pathlib import Path

import yaml

from coverage import ROOT, convert, markdown_text, words

GENERATED = "<!-- okf-generated -->"
MARKER = re.compile(r"<!-- okf-source-span: ([^ ]+) -->")
# Fin de línea con palabra partida: guion, guion suave o el U+0002 con que pdfium marca
# el guion que inserta al partir una palabra.
HYPHEN = re.compile(r"(\w+)[-­\u0002]\s*\r?\n\s*(\w+)")


# --- Referencia -------------------------------------------------------------------------


def join_hyphens(text, context):
    """Une la palabra partida al final de línea; conserva el guion si la palabra compuesta
    aparece entera en otra parte del documento, `context` (especificación, §5)."""
    whole = HYPHEN.sub(" ", context)

    def join(match):
        left, right = match.group(1), match.group(2)
        compound = f"{left}-{right}"
        return compound if compound in whole else left + right

    return HYPHEN.sub(join, text)


def reference_file(doc):
    try:
        rel = doc.resolve().relative_to(ROOT / "examples")
    except ValueError:
        return None
    fmt, *middle, name = rel.parts
    group = middle[0] if middle else "fixtures"
    path = ROOT / "examples" / fmt / "exactitud" / group / (Path(name).stem + ".json")
    return path if path.exists() else None


def exact_reference(path):
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    return {int(k): words(v) for k, v in data["pages"].items()}


def pdfium_reference(doc):
    import pypdfium2 as pdfium

    pdf = pdfium.PdfDocument(str(doc))
    try:
        texts = [page.get_textpage().get_text_bounded() for page in pdf]
    finally:
        pdf.close()
    whole = "\n".join(texts)
    return {k: words(join_hyphens(t, whole)) for k, t in enumerate(texts, 1)}


# --- Bundle -----------------------------------------------------------------------------


def frontmatter(md):
    if not md.startswith("---\n"):
        return {}, md
    end = md.find("\n---\n", 4)
    return yaml.safe_load(md[4:end]) or {}, md[end + 5 :]


def block_words(segment):
    """Palabras del original en un bloque: sin notas ni rótulos del conversor."""
    segment = re.sub(re.escape(GENERATED) + r"\n(?:.+\n?)+", "\n", segment)
    segment = re.sub(r"!\[[^\]]*\]\([^)]*\)", " ", segment)
    # Rótulo del conversor delante del texto girado de una página.
    segment = segment.replace("_Texto girado:_", " ")
    return words(markdown_text(segment))


def bundle_pages(zip_path):
    """Bloques de cada página (listas de palabras), en el orden del Markdown; cada bloque
    con sus páginas; y las palabras de `documento.md` que no son texto de una página
    (encabezados, pies)."""
    pages = collections.defaultdict(list)
    blocks = []
    furniture = collections.Counter()
    with zipfile.ZipFile(zip_path) as z:
        for name in sorted(z.namelist()):
            base = name.rsplit("/", 1)[-1]
            if not name.endswith(".md") or base in ("index.md", "conversion-report.md"):
                continue
            meta, body = frontmatter(z.read(name).decode("utf-8"))
            spans = list(meta.get("source_spans") or [])
            if meta.get("block_spans"):
                file = json.loads(z.read(meta["block_spans"].lstrip("/")).decode("utf-8"))
                spans += file["source_spans"]
            ranges = {
                s["id"]: (s["from"], s["to"])
                for s in spans
                if s.get("kind") == "pdf_pages" and "from" in s
            }
            parts = MARKER.split(body)
            if name == "documento.md":
                furniture.update(block_words(parts[0]))
            elif "seccion" in ranges:
                # El título del concepto está en la página donde empieza.
                first = ranges["seccion"][0]
                title = block_words(parts[0])
                pages[first].append(title)
                blocks.append((first, first, title))
            for span, segment in zip(parts[1::2], parts[2::2]):
                if name == "documento.md":
                    # El marcador cubre solo el título; lo demás (estructura, encabezados
                    # y pies) no es texto de una página.
                    block, _, rest = segment.lstrip("\n").partition("\n\n")
                    furniture.update(block_words(rest))
                    if span != "titulo" or span not in ranges:
                        furniture.update(block_words(block))
                        continue
                    segment = block
                if span not in ranges:
                    continue
                first, last = ranges[span]
                found = block_words(segment)
                blocks.append((first, last, found))
                for p in range(first, last + 1):
                    pages[p].append(found)
    return pages, blocks, furniture


# --- Métricas ---------------------------------------------------------------------------


def lcs(a, b):
    """Longitud de la subsecuencia común más larga (Hyyrö, por bits)."""
    if not a or not b:
        return 0
    masks = {}
    for i, w in enumerate(a):
        masks[w] = masks.get(w, 0) | (1 << i)
    full = (1 << len(a)) - 1
    v = full
    for w in b:
        u = v & masks.get(w, 0)
        v = ((v + u) | (v - u)) & full
    return len(a) - bin(v).count("1")


def matched(ref, produced):
    have = collections.Counter(produced)
    return sum(min(n, have[w]) for w, n in collections.Counter(ref).items())


def without_furniture(ref, produced, furniture):
    """Retira de la referencia aproximada las palabras ausentes que el bundle registra como
    encabezado o pie, empezando por las más cercanas a los bordes de la página."""
    missing = collections.Counter(ref) - collections.Counter(produced)
    drop = set()
    for w, n in missing.items():
        n = min(n, furniture.get(w, 0))
        if not n:
            continue
        positions = [i for i, x in enumerate(ref) if x == w]
        positions.sort(key=lambda i: min(i, len(ref) - 1 - i))
        drop.update(positions[:n])
    return [w for i, w in enumerate(ref) if i not in drop]


def measure_page(ref, produced, approximate):
    found = matched(ref, produced)
    if approximate:
        # pdfium une palabras espaciadas por posición: una ausente de cuatro letras o más
        # que aparece dentro del texto sin espacios no se pierde.
        stream = "".join(produced)
        missing = collections.Counter(ref) - collections.Counter(produced)
        found += sum(n for w, n in missing.items() if len(w) >= 4 and w in stream)
    pairs = collections.Counter(zip(ref, ref[1:]))
    kept = collections.Counter(zip(produced, produced[1:]))
    return {
        "words": len(ref),
        "matched": found,
        "ordered": min(lcs(ref, produced), found),
        "pairs": sum(pairs.values()),
        "adjacent": sum(min(n, kept[p]) for p, n in pairs.items()),
    }


def ratio(a, b):
    return round(100 * a / b, 2) if b else None


def scores(total):
    return {
        "completeness": ratio(total["matched"], total["words"]),
        "order": ratio(total["ordered"], total["matched"]),
        "inner_order": ratio(total["inner_ordered"], total["inner_matched"]),
        "contiguity": ratio(total["adjacent"], total["pairs"]),
    }


def divergence(ref, produced, width=10):
    """Primer tramo en que el bundle se aparta de la referencia, para revisarlo."""
    matcher = difflib.SequenceMatcher(None, ref, produced, autojunk=False)
    for op, a0, _, b0, _ in matcher.get_opcodes():
        if op != "equal":
            return (
                " ".join(ref[max(0, a0 - 3) : a0 + width]),
                " ".join(produced[max(0, b0 - 3) : b0 + width]),
            )
    return None


# --- Estructura ---------------------------------------------------------------------------

STRUCTURE = {
    "headings": "encabezados",
    "heading_precision": "precisión de encabezados",
    "heading_levels": "niveles",
    "list_items": "elementos de lista",
    "list_numbers": "numeración",
    "table_cells": "celdas",
    "false_tables": "tablas falsas",
    "code_lines": "líneas de código",
    "code_indent": "sangría",
}

HEADING = re.compile(r"^(#{1,6})\s+(.*)$")
LIST_ITEM = re.compile(r"^\s*([-*+]|\d+[.)])\s+(.*)$")
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})")
CELL = re.compile(r"(?<!\\)\|")


def norm(text):
    """Texto comparable de un encabezado, una celda o un elemento: sus palabras."""
    return " ".join(words(markdown_text(text)))


def without_notes(body):
    """El cuerpo sin marcadores ni notas del conversor (el marcador y su bloque)."""
    body = re.sub(re.escape(GENERATED) + r"\n(?:.+\n?)+", "\n", body)
    return MARKER.sub("", body)


def markdown_structure(body):
    """Encabezados (nivel, texto), elementos de lista (rótulo, texto), tablas (filas de
    celdas) y bloques de código (líneas) de un cuerpo Markdown, en orden."""
    out = {"headings": [], "items": [], "tables": [], "code": []}
    table, code = None, None
    for line in without_notes(body).split("\n"):
        if code is not None:
            if FENCE.match(line):
                out["code"].append(code)
                code = None
            else:
                code.append(line)
            continue
        if FENCE.match(line):
            code = []
            continue
        if line.startswith("|"):
            cells = [c.strip() for c in CELL.split(line.strip())[1:-1]]
            if table is None:
                table = []
                out["tables"].append(table)
            if not all(re.fullmatch(r":?-{3,}:?", c) for c in cells):
                table.append(cells)
            continue
        table = None
        if m := HEADING.match(line):
            out["headings"].append((len(m.group(1)), m.group(2)))
        elif m := LIST_ITEM.match(line):
            out["items"].append((m.group(1), m.group(2)))
    return out


def segments(zip_path):
    """Título del documento (texto y página) y bloques Markdown de los conceptos, en orden,
    con su primera y su última página; el título de cada concepto, en la página donde
    empieza."""
    out = []
    with zipfile.ZipFile(zip_path) as z:
        meta, body = frontmatter(z.read("documento.md").decode("utf-8"))
        pages = {s["id"]: s.get("from") for s in meta.get("source_spans") or []}
        part = body.split("<!-- okf-source-span: titulo -->", 1)
        found = HEADING.match(part[1].strip().split("\n", 1)[0]) if len(part) == 2 else None
        title = (found.group(2), pages.get("titulo")) if found else None
        names = sorted(
            n for n in z.namelist()
            if n.startswith("contenido/") and n.endswith(".md") and not n.endswith("index.md")
        )
        for name in names:
            meta, body = frontmatter(z.read(name).decode("utf-8"))
            spans = list(meta.get("source_spans") or [])
            if meta.get("block_spans"):
                spans += json.loads(z.read(meta["block_spans"].lstrip("/")).decode("utf-8"))["source_spans"]
            ranges = {s["id"]: (s.get("from"), s.get("to")) for s in spans}
            parts = MARKER.split(body)
            start = ranges.get("seccion", (None, None))[0]
            out.append((start, start, parts[0]))
            out += [(*ranges.get(span, (None, None)), segment) for span, segment in zip(parts[1::2], parts[2::2])]
    return title, out


def bundle_structure(zip_path, pages=None):
    """Estructura del bundle en el orden del documento: el título de `documento.md`
    (nivel 0) y la de cada concepto, solo de `pages` si se indican. El `#` de un concepto
    que repite el título del documento no es otro encabezado."""
    title, blocks = segments(zip_path)
    wanted = lambda page: pages is None or page in pages
    name = norm(title[0]) if title else None
    total = {"headings": [(0, name)] if title and wanted(title[1]) else [], "items": [], "tables": [], "code": []}
    for page, _, segment in blocks:
        if not wanted(page):
            continue
        s = markdown_structure(segment)
        total["headings"] += [(level, norm(t)) for level, t in s["headings"] if norm(t) != name]
        for key in ("items", "tables", "code"):
            total[key] += s[key]
    return total


def in_order(reference, found, same):
    """Parejas (i, j) de elementos de `reference` y `found` que coinciden, en orden."""
    pairs, start = [], 0
    for i, r in enumerate(reference):
        for j in range(start, len(found)):
            if same(r, found[j]):
                pairs.append((i, j))
                start = j + 1
                break
    return pairs


def sign(x):
    return (x > 0) - (x < 0)


def measure_structure(blocks, bundle, pages=None):
    """Métricas de estructura frente a los bloques de una referencia exacta; con una
    referencia de algunas páginas, solo frente a la estructura del bundle en ellas."""
    got = bundle_structure(bundle, pages)
    out = {}
    # Encabezados: presentes como encabezados, en orden, y con el mismo nivel relativo
    # entre consecutivos (los niveles absolutos dependen del nivel de división).
    ref_heads = [(0 if b["type"] == "title" else b["level"], norm(b["text"]))
                 for b in blocks if b["type"] in ("title", "heading")]
    if ref_heads:
        pairs = in_order(ref_heads, got["headings"], lambda r, f: r[1] == f[1])
        steps = list(zip(pairs, pairs[1:]))
        same_level = sum(
            sign(ref_heads[b[0]][0] - ref_heads[a[0]][0]) == sign(got["headings"][b[1]][0] - got["headings"][a[1]][0])
            for a, b in steps
        )
        out["headings"] = ratio(len(pairs), len(ref_heads))
        out["heading_precision"] = ratio(len(pairs), len(got["headings"]))
        out["heading_levels"] = ratio(same_level, len(steps)) if steps else 100.0
    # Listas: cada elemento como elemento de lista y, si es ordenada, con su número.
    ref_items = [(b["ordered"], i["label"], norm(i["text"])) for b in blocks if b["type"] == "list" for i in b["items"]]
    if ref_items:
        found = [(label, norm(t)) for label, t in got["items"]]
        pairs = in_order(ref_items, found, lambda r, f: r[2] == f[1])
        numbered = [(i, j) for i, j in pairs if ref_items[i][0]]
        digits = lambda s: re.sub(r"\D", "", s)
        out["list_items"] = ratio(len(pairs), len(ref_items))
        out["list_numbers"] = (
            ratio(sum(digits(ref_items[i][1]) == digits(found[j][0]) for i, j in numbered), len(numbered))
            if numbered else 100.0
        )
    # Tablas: cada tabla de la referencia frente a la del bundle con más palabras en
    # común; cuentan las celdas en su fila y columna. Una tabla del bundle que no
    # corresponde a ninguna es una tabla falsa.
    ref_tables = [[[norm(c) for c in row] for row in b["rows"]] for b in blocks if b["type"] == "table"]
    found_tables = [[[norm(c) for c in row] for row in t] for t in got["tables"]]
    table_words = lambda t: [w for row in t for c in row for w in c.split()]
    used, cells, correct = set(), 0, 0
    for ref in ref_tables:
        cells += sum(len(row) for row in ref)
        best = max(
            range(len(found_tables)),
            key=lambda j: matched(table_words(ref), table_words(found_tables[j])),
            default=None,
        )
        if best is None or matched(table_words(ref), table_words(found_tables[best])) * 2 < len(table_words(ref)):
            continue
        used.add(best)
        t = found_tables[best]
        correct += sum(
            r < len(t) and c < len(t[r]) and t[r][c] == cell
            for r, row in enumerate(ref) for c, cell in enumerate(row)
        )
    if ref_tables:
        out["table_cells"] = ratio(correct, cells)
    out["false_tables"] = len(found_tables) - len(used)
    # Código: líneas de cada bloque, en orden, y su sangría relativa al bloque.
    ref_code = [b["lines"] for b in blocks if b["type"] == "code"]
    if ref_code:
        lines = kept = indented = 0
        for ref in ref_code:
            lines += sum(1 for l in ref if l.strip())
            best = max(got["code"], key=lambda c: matched([l.strip() for l in ref], [l.strip() for l in c]), default=[])
            pairs = in_order([l.strip() for l in ref], [l.strip() for l in best], lambda r, f: r == f and r != "")
            kept += len(pairs)
            margin = lambda block: min((len(l) - len(l.lstrip()) for l in block if l.strip()), default=0)
            ref_margin, got_margin = margin(ref), margin(best)
            indented += sum(
                len(ref[i]) - len(ref[i].lstrip()) - ref_margin == len(best[j]) - len(best[j].lstrip()) - got_margin
                for i, j in pairs
            )
        out["code_lines"] = ratio(kept, lines)
        out["code_indent"] = ratio(indented, kept) if kept else None
    return out


def measure(doc, bundle, reference, worst):
    exact = reference is not None
    refs = exact_reference(reference) if exact else pdfium_reference(doc)
    pages, blocks, furniture = bundle_pages(bundle)
    produced = {p: [w for block in pages.get(p, []) for w in block] for p in refs}
    if not exact:
        refs = {p: without_furniture(ref, produced[p], furniture) for p, ref in refs.items()}
    # Orden interno: cada bloque frente a la referencia de sus páginas, juntas.
    inner = collections.defaultdict(collections.Counter)
    for first, last, found in blocks:
        window = [w for p in range(first, last + 1) for w in refs.get(p, [])]
        if window and found:
            inner[first].update({"inner_matched": matched(window, found), "inner_ordered": lcs(window, found)})
    rows = []
    total = collections.Counter()
    for page, ref in sorted(refs.items()):
        if not ref:
            continue
        counts = collections.Counter(measure_page(ref, produced[page], not exact))
        counts.update(inner[page])
        total.update(counts)
        rows.append({"page": page, "words": len(ref), **scores(counts), "divergence": divergence(ref, produced[page])})
    # Peores páginas: primero por la mezcla de texto (orden interno y contigüidad), que
    # una referencia aproximada también mide bien.
    ranked = sorted(
        rows,
        key=lambda r: (
            (r["inner_order"] or 100) + (r["contiguity"] or 100) + (r["completeness"] or 100),
            r["order"] or 100,
        ),
    )
    keys = ("page", "words", "completeness", "order", "inner_order", "contiguity")
    blocks = json.loads(Path(reference).read_text(encoding="utf-8")).get("blocks") if exact else None
    covered = set(refs) if exact else None
    return {
        "document": str(doc),
        "reference": str(reference) if exact else "pdfium (aproximada)",
        "pages": len(rows),
        "words": total["words"],
        **scores(total),
        "structure": measure_structure(blocks, bundle, covered) if blocks else None,
        "worst": [
            {k: r[k] for k in keys + ("divergence",)}
            for r in ranked[:worst]
            if min(r[k] or 100 for k in keys[2:]) < 100
        ],
        "by_page": [{k: r[k] for k in keys} for r in rows],
    }


# --- Borradores de referencia ------------------------------------------------------------


def plain(segment):
    """Texto de un bloque Markdown sin marcado: sin notas ni rótulos del conversor, sin
    marcas de encabezado, lista, tabla ni énfasis, con los enlaces como su texto."""
    segment = without_notes(segment)
    segment = re.sub(r"!\[[^\]]*\]\([^)]*\)", "", segment).replace("_Texto girado:_", "")
    out, code = [], False
    for line in segment.split("\n"):
        if FENCE.match(line):
            code = not code
            continue
        if code:
            out.append(line)
            continue
        if line.startswith("|"):
            cells = [c.strip() for c in CELL.split(line.strip())[1:-1]]
            if all(re.fullmatch(r":?-{3,}:?", c) for c in cells):
                continue
            line = " ".join(cells)
        line = HEADING.sub(r"\2", line)
        line = re.sub(r"^\s*(?:[-*+]|(\d+[.)]))\s+", lambda m: (m.group(1) or "") + " " if m.group(1) else "", line)
        line = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", line)
        line = re.sub(r"(\*\*|\*|__)(?=\S)(.+?)(?<=\S)\1", r"\2", line)
        line = re.sub(r"\\([\\`*_{}\[\]()#+\-.!|>])", r"\1", line)
        out.append(line.rstrip())
    return "\n".join(l for l in out).strip()


def draft_blocks(segment, page):
    """Bloques de estructura de un bloque Markdown, con el formato de las referencias."""
    s = markdown_structure(segment)
    out = [{"page": page, "type": "heading", "level": level, "text": plain(t)} for level, t in s["headings"]]
    if s["items"]:
        out.append({"page": page, "type": "list", "ordered": s["items"][0][0][0].isdigit(),
                    "items": [{"label": label, "text": plain(t)} for label, t in s["items"]]})
    out += [{"page": page, "type": "table", "rows": [[plain(c) for c in row] for row in t]} for t in s["tables"]]
    out += [{"page": page, "type": "code", "lines": c} for c in s["code"]]
    return out


def choose_pages(doc, bundle, count):
    """Páginas difíciles: las de peor orden interno y contigüidad frente a pdfium, las que
    tienen tablas o código en el bundle y, si faltan, las primeras."""
    result = measure(doc, bundle, None, 0)
    ranked = sorted(result["by_page"], key=lambda r: (r["inner_order"] or 100) + (r["contiguity"] or 100))
    _, blocks = segments(bundle)
    structured = [p for p, _, seg in blocks if p and ("|" in seg or "```" in seg)]
    chosen = []
    # Un escaneo no tiene texto en pdfium: entonces cuentan sus primeras páginas.
    total = len(pdfium_reference(doc))
    for p in [r["page"] for r in ranked[: count // 2]] + structured + [r["page"] for r in ranked] + list(range(1, total + 1)):
        if p not in chosen:
            chosen.append(p)
        if len(chosen) == count:
            break
    return sorted(chosen)


def split_by_pages(text, first, last, reference):
    """Partes de `text` en cada página de `first` a `last`: cada corte, en la palabra que
    hace coincidir más palabras con el texto de pdfium de la página a cada lado."""
    if last <= first:
        return [(first, text)]
    tokens = list(re.finditer(r"[^\W_]+", text))
    folded = [words(t.group(0))[:1] or [""] for t in tokens]
    folded = [f[0] for f in folded]
    before = collections.Counter(reference.get(first, []))
    after = collections.Counter(w for p in range(first + 1, last + 1) for w in reference.get(p, []))
    left, right = collections.Counter(), collections.Counter(folded)
    score = lambda: sum(min(n, before[w]) for w, n in left.items()) + sum(min(n, after[w]) for w, n in right.items())
    best, cut = score(), 0
    for k, w in enumerate(folded, 1):
        left[w] += 1
        right[w] -= 1
        if (s := score()) > best:
            best, cut = s, k
    offset = tokens[cut].start() if cut < len(tokens) else len(text)
    head, tail = text[:offset].rstrip(), text[offset:].lstrip()
    return [(first, head)] + split_by_pages(tail, first + 1, last, reference)


def write_draft(doc, bundle, pages, out_dir):
    """Borrador de referencia de `pages` (texto y estructura del bundle, sin revisar), las
    vistas de esas páginas y una hoja de revisión con el texto de pdfium al lado."""
    import pypdfium2 as pdfium

    out_dir.mkdir(parents=True, exist_ok=True)
    title, blocks = segments(bundle)
    texts = collections.defaultdict(list)
    structure = []
    if title and title[1] in pages:
        texts[title[1]].append(plain(title[0]))
        structure.append({"page": title[1], "type": "title", "text": plain(title[0])})
    reference = pdfium_reference(doc)
    for first, last, segment in blocks:
        text = plain(segment)
        if not text or first is None:
            continue
        # Un bloque que cruza páginas se reparte por la palabra que mejor separa su texto
        # entre el de pdfium de cada página.
        for page, part in split_by_pages(text, first, last or first, reference):
            if page in pages and part:
                texts[page].append(part)
        if first in pages:
            structure += [b for b in draft_blocks(segment, first) if b["type"] != "heading" or b["text"] != (plain(title[0]) if title else None)]
    stem = doc.stem
    data = {
        "document": doc.name,
        "source": "borrador de tools/exactitud.py: texto y estructura del bundle; corregir contra la vista",
        "reviewed": False,
        "pages": {str(p): "\n".join(texts[p]) for p in pages},
        "blocks": structure,
    }
    (out_dir / f"{stem}.json").write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    pdf = pdfium.PdfDocument(str(doc))
    sheet = [f"# Revisión de {doc.name}", "",
             f"Corregir `{stem}.json` mirando cada vista: el texto de cada página en orden de lectura,"
             " sin encabezados ni pies repetidos, y sus bloques (títulos con su nivel, listas, tablas"
             " y código). Al terminar, poner `\"reviewed\": true`.", ""]
    try:
        for p in pages:
            image = f"{stem}-pagina-{p:03}.png"
            pdf[p - 1].render(scale=1.5).to_pil().save(out_dir / image)
            reference = pdf[p - 1].get_textpage().get_text_bounded().replace("\r", "")
            sheet += [f"## Página {p}", "", f"![Vista de la página {p}]({image})", "",
                      "### Borrador (bundle)", "", "```text", "\n".join(texts[p]), "```", "",
                      "### pdfium (orden del flujo de contenido, solo como ayuda)", "", "```text", reference.strip(), "```", ""]
    finally:
        pdf.close()
    (out_dir / f"{stem}-revision.md").write_text("\n".join(sheet), encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("docs", nargs="+", type=Path)
    parser.add_argument("--binary", type=Path, default=ROOT / "target" / "release" / "okf-microservice")
    parser.add_argument("--bundle", type=Path, help="bundle ya convertido (un solo DOC)")
    parser.add_argument("--reference", type=Path, help="referencia exacta (un solo DOC)")
    parser.add_argument("--json", type=Path, help="resultados completos, con cada página")
    parser.add_argument("--worst", type=int, default=5, help="páginas peores que se listan")
    parser.add_argument("--draft", type=Path, help="escribe aquí un borrador de referencia y su hoja de revisión")
    parser.add_argument("--pages", help="páginas del borrador, separadas por comas (por defecto, las difíciles)")
    parser.add_argument("--count", type=int, default=15, help="páginas difíciles del borrador")
    args = parser.parse_args()
    if args.draft:
        for doc in args.docs:
            with tempfile.TemporaryDirectory() as tmp:
                bundle = args.bundle or convert(args.binary, doc, tmp)[1]
                pages = [int(p) for p in args.pages.split(",")] if args.pages else choose_pages(doc, bundle, args.count)
                write_draft(doc, bundle, pages, args.draft)
            print(f"{doc.name}: borrador de las páginas {', '.join(map(str, pages))} en {args.draft}")
        return 0
    if (args.bundle or args.reference) and len(args.docs) != 1:
        parser.error("--bundle y --reference admiten un solo documento")
    fmt = lambda v: "-" if v is None else f"{v:.2f}"
    results = []
    print(
        f"{'documento':46} {'referencia':10} {'págs':>5} {'palabras':>8} {'complet.':>8}"
        f" {'orden':>7} {'interno':>7} {'contig.':>7}"
    )
    for doc in args.docs:
        reference = args.reference or reference_file(doc)
        if reference and json.loads(Path(reference).read_text(encoding="utf-8")).get("reviewed") is False:
            # Un borrador es el propio bundle: medir con él daría siempre el 100 %.
            print(f"{str(doc)[-46:]:46} borrador sin revisar ({reference}): no se mide")
            continue
        with tempfile.TemporaryDirectory() as tmp:
            bundle = args.bundle
            if bundle is None:
                _, bundle = convert(args.binary, doc, tmp)
            result = measure(doc, bundle, reference, args.worst)
        results.append(result)
        kind = "exacta" if reference else "pdfium"
        print(
            f"{str(doc)[-46:]:46} {kind:10} {result['pages']:>5} {result['words']:>8}"
            f" {fmt(result['completeness']):>8} {fmt(result['order']):>7}"
            f" {fmt(result['inner_order']):>7} {fmt(result['contiguity']):>7}"
        )
        if result["structure"]:
            print("    estructura: " + ", ".join(f"{STRUCTURE[k]} {fmt(v) if k != 'false_tables' else v}"
                                               for k, v in result["structure"].items()))
        for w in result["worst"]:
            print(
                f"    pág. {w['page']:>4}: completitud {fmt(w['completeness'])}, orden {fmt(w['order'])},"
                f" interno {fmt(w['inner_order'])}, contigüidad {fmt(w['contiguity'])} ({w['words']} palabras)"
            )
            if w["divergence"]:
                print(f"        referencia: …{w['divergence'][0]}…")
                print(f"        bundle:     …{w['divergence'][1]}…")
    if args.json:
        args.json.write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
