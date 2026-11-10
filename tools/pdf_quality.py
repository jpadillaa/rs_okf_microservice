# /// script
# requires-python = ">=3.11"
# dependencies = ["pypdfium2==5.14.0", "PyYAML==6.0.3"]
# ///
"""Audita un PDF y su bundle sin copiar el texto del original al informe.

Ejemplo (el ZIP debe proceder del mismo PDF):
    uv run tools/pdf_quality.py datos_de_prueba/documento.pdf output/intento/bundle.zip \
        --json output/calidad.json

La cobertura global sigue tools/coverage.py. La cobertura por página es conservadora:
un bloque que continúa en varias páginas se compara con ese tramo completo. No mide
orden de lectura ni exactitud de tablas, código, figuras o texto dentro de imágenes;
esas propiedades requieren inspección visual. El estado `complete` no las garantiza.
"""

import argparse
import collections
import hashlib
import json
import re
import zipfile
from pathlib import Path

import pypdfium2 as pdfium
import yaml

from coverage import bundle_texts, markdown_text, words


def split_markdown(text):
    if not text.startswith("---\n"):
        raise ValueError("concepto sin frontmatter")
    metadata, body = text[4:].split("\n---\n", 1)
    return yaml.safe_load(metadata), body


def compare(reference, produced):
    expected = collections.Counter(reference)
    actual = collections.Counter(produced)
    missing = expected - actual
    total = sum(expected.values())
    stream = "".join(produced)
    regrouped = sum(n for w, n in missing.items() if len(w) >= 4 and w in stream)
    return {
        "reference_words": total,
        "bundle_words": sum(actual.values()),
        "missing_words": sum(missing.values()),
        "extra_words": sum((actual - expected).values()),
        "words_pct": round(100 * (1 - sum(missing.values()) / total), 2) if total else None,
        "text_pct": (
            round(100 * (1 - (sum(missing.values()) - regrouped) / total), 2)
            if total else None
        ),
    }


def audit(original, bundle):
    with original.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    references = []
    with pdfium.PdfDocument(original) as pdf:
        for page in pdf:
            text = page.get_textpage()
            try:
                references.append(words(text.get_text_bounded()))
            finally:
                text.close()
                page.close()

    page_words = collections.defaultdict(list)
    ranges = []
    multi_page_blocks = 0
    with zipfile.ZipFile(bundle) as archive:
        doc, _ = split_markdown(archive.read("documento.md").decode())
        if doc["source_file"]["sha256"] != digest:
            raise ValueError("el bundle no procede del PDF indicado")
        stored = hashlib.sha256(archive.read("references/original.pdf")).hexdigest()
        if stored != digest:
            raise ValueError("el original incluido no coincide con el PDF indicado")
        report, report_body = split_markdown(archive.read("conversion-report.md").decode())
        concepts = sorted(
            n for n in archive.namelist()
            if n.startswith("contenido/") and n.endswith(".md") and n != "contenido/index.md"
        )
        for name in concepts:
            fm, body = split_markdown(archive.read(name).decode())
            spans = {s["id"]: s for s in fm["source_spans"]}
            # Desde el perfil 1.17.0, los localizadores de bloque están en su archivo.
            if fm.get("block_spans"):
                blocks = json.loads(archive.read(fm["block_spans"].lstrip("/")).decode())
                spans.update({s["id"]: s for s in blocks["source_spans"]})
            # El título también pertenece al contenido localizado del concepto.
            for span in spans.values():
                if span["id"] in ("seccion", "texto") and span["kind"] == "pdf_pages":
                    ranges.append({"concept": name, "start": span["from"], "end": span["to"]})
            markers = list(re.finditer(r"<!-- okf-source-span: ([^ ]+) -->", body))
            title = body[:markers[0].start()] if markers else body
            if ranges and ranges[-1]["concept"] == name:
                page_words[ranges[-1]["start"]].extend(words(markdown_text(title)))
            for i, marker in enumerate(markers):
                span = spans[marker.group(1)]
                if span["kind"] != "pdf_pages":
                    continue
                start, end = span["from"], span["to"]
                multi_page_blocks += end > start
                next_start = markers[i + 1].start() if i + 1 < len(markers) else len(body)
                block = words(markdown_text(body[marker.end():next_start]))
                for page in range(start, end + 1):
                    page_words[page].extend(block)
        assets = [n for n in archive.namelist() if n.startswith("assets/") and not n.endswith("/")]
        result = {
            "source_sha256": digest,
            "source_bytes": original.stat().st_size,
            "bundle_bytes": bundle.stat().st_size,
            "pages": len(references),
            "pages_without_reference_text": [i + 1 for i, ws in enumerate(references) if not ws],
            "profile_version": doc.get("profile_version"),
            "converter_commit": doc.get("converter_commit"),
            "concepts": len(concepts),
            "assets": len(assets),
            "asset_bytes": sum(archive.getinfo(n).file_size for n in assets),
            "multi_page_blocks": multi_page_blocks,
            "report_metadata": report,
            # Solo datos del conversor; el texto del informe puede contener texto privado.
            "page_classes": dict(collections.Counter(
                label.strip()
                for start, end, label in re.findall(
                    r"^\| (\d+)(?:–(\d+))? \| ([^|]+?) \|", report_body, re.M
                )
                for _ in range(int(start), int(end or start) + 1)
            )),
            "global": compare(
                [w for ws in references for w in ws],
                [w for text in bundle_texts(bundle) for w in words(text)],
            ),
            "concept_ranges": ranges,
            "per_page": [
                {"page": i + 1, **compare(ws, page_words[i + 1])}
                for i, ws in enumerate(references)
            ],
        }
        # No publicar metadatos libres que puedan reproducir contenido privado.
        result["report_metadata"] = {
            k: report[k] for k in ("status", "profile_version") if k in report
        }
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--json", required=True, type=Path)
    args = parser.parse_args()
    result = audit(args.original, args.bundle)
    args.json.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({k: result[k] for k in ("pages", "concepts", "assets", "global")}, ensure_ascii=False))


if __name__ == "__main__":
    main()
