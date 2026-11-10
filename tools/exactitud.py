# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "openpyxl==3.1.5",
#     "pypdfium2==5.14.0",
#     "python-docx==1.2.0",
#     "python-pptx==1.0.2",
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
    return {
        "document": str(doc),
        "reference": str(reference) if exact else "pdfium (aproximada)",
        "pages": len(rows),
        "words": total["words"],
        **scores(total),
        "worst": [
            {k: r[k] for k in keys + ("divergence",)}
            for r in ranked[:worst]
            if min(r[k] or 100 for k in keys[2:]) < 100
        ],
        "by_page": [{k: r[k] for k in keys} for r in rows],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("docs", nargs="+", type=Path)
    parser.add_argument("--binary", type=Path, default=ROOT / "target" / "release" / "okf-microservice")
    parser.add_argument("--bundle", type=Path, help="bundle ya convertido (un solo DOC)")
    parser.add_argument("--reference", type=Path, help="referencia exacta (un solo DOC)")
    parser.add_argument("--json", type=Path, help="resultados completos, con cada página")
    parser.add_argument("--worst", type=int, default=5, help="páginas peores que se listan")
    args = parser.parse_args()
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
