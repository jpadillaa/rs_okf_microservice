# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "openpyxl==3.1.5",
#     "pypdfium2==5.14.0",
#     "python-docx==1.2.0",
#     "python-pptx==1.0.2",
# ]
# ///
"""Measure how much of each document's text reaches its OKF bundle, against a reference
extracted with tools that share no code with the service's readers (plan 1, R2).

The bundle comes from the service binary (`okf-microservice convert`, the subcommand the
worker runs). The reference text comes from pdfium for PDF, python-docx for DOCX,
python-pptx for PPTX and openpyxl (cached values) for XLSX. Where those libraries do not
reach, the script reads the part's XML directly:

    DOCX  footnotes, endnotes, comments, text boxes, equations, SmartArt, charts and the
          alternative text of drawings
    PPTX  charts, SmartArt, comments and the alternative text of pictures
    XLSX  charts

Both sides are reduced to words (NFKC, case-folded, runs of letters and digits) and
compared as multisets, in both directions:

    missing  reference words the bundle lacks (losses);
    extra    bundle words the reference lacks (text the reference cannot see, labels the
             converter writes, or duplicated or invented text).

`words%` is the share of reference words present in the bundle. `text%` also counts as
present a missing word of four or more characters found inside the bundle's text once
its spaces are removed: the same text, joined or split differently (pdfium joins words
spaced by position and keeps line-end hyphenation). The bundle side reads
every Markdown concept and `documento.md` (without HTML comments, link targets or the
frontmatter, except the fields that carry text of the original: `kicker`, `section` and
`display_label`) and the CSV resources; navigation indexes and the conversion report are left
out.

With --write-reference the reference word list of each document is saved next to it, in
`examples/<format>/cobertura/<group>/<name>.tsv`, so the CI can check coverage without
Python. Only documents inside `examples/` get a list: the others may be private.

Usage:
    uv run tools/coverage.py [--binary PATH] [--json OUT] [--write-reference] DOC...
"""

import argparse
import collections
import datetime
import io
import json
import posixpath
import re
import subprocess
import sys
import tempfile
import unicodedata
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET

WORD = re.compile(r"[^\W_]+")
NS = {
    "w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    "a": "http://schemas.openxmlformats.org/drawingml/2006/main",
    "c": "http://schemas.openxmlformats.org/drawingml/2006/chart",
    "m": "http://schemas.openxmlformats.org/officeDocument/2006/math",
    "p": "http://schemas.openxmlformats.org/presentationml/2006/main",
    "wp": "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    "mc": "http://schemas.openxmlformats.org/markup-compatibility/2006",
    "p188": "http://schemas.microsoft.com/office/powerpoint/2018/8/main",
    "tc": "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments",
}
# Frontmatter fields that carry text of the original (a slide's kicker or section, its
# visible number); the rest of the frontmatter is generated.
CONTENT_FIELDS = re.compile(r'^\s*(?:kicker|section|display_label): "(.*)"$', re.M)
ROOT = Path(__file__).resolve().parent.parent


def words(text):
    text = unicodedata.normalize("NFKC", text).replace("­", "").casefold()
    return WORD.findall(text)


# --- Reference ------------------------------------------------------------------------


def xml_texts(z, pattern, xpath):
    """Text of the nodes matching `xpath` in every package part matching `pattern`."""
    out = []
    for name in sorted(z.namelist()):
        if re.fullmatch(pattern, name):
            root = ET.fromstring(z.read(name))
            out.extend(n.text or "" for n in root.iterfind(xpath, NS))
    return out


# A 1×1 PNG, standing in for an image the package lacks.
BLANK_PNG = bytes.fromhex(
    "89504e470d0a1a0a0000000d4948445200000001000000010806000000"
    "1f15c4890000000d49444154789c6360000002000154a24f5d0000000049454e44ae426082"
)


def without_dangling_relationships(path):
    """The package as an in-memory file, without relationships to parts it lacks.

    The reduced corpus copies and the samples with declared losses lack some parts (a
    SmartArt layout, an image) that their relationships still name; python-docx,
    python-pptx and openpyxl refuse such packages, while the service, like Office,
    ignores the missing part. A missing image is replaced by a blank one, because the
    parts that point to it may need the relationship; any other missing part loses it.
    """
    rel_ns = "{http://schemas.openxmlformats.org/package/2006/relationships}"
    out = io.BytesIO()
    with zipfile.ZipFile(path) as src, zipfile.ZipFile(out, "w") as dst:
        names = set(src.namelist())
        blanks = set()
        for info in src.infolist():
            data = src.read(info.filename)
            if info.filename.endswith(".rels"):
                root = ET.fromstring(data)
                base = posixpath.dirname(posixpath.dirname(info.filename))
                for rel in list(root):
                    target = rel.get("Target", "")
                    if rel.get("TargetMode") == "External":
                        continue
                    full = target.lstrip("/") if target.startswith("/") else posixpath.normpath(
                        posixpath.join(base, target)
                    )
                    if full in names or full in blanks:
                        continue
                    if full.lower().endswith((".png", ".jpg", ".jpeg", ".gif")):
                        blanks.add(full)
                    else:
                        root.remove(rel)
                ET.register_namespace("", rel_ns[1:-1])
                data = ET.tostring(root, xml_declaration=True, encoding="UTF-8")
            dst.writestr(info, data)
        for name in sorted(blanks):
            dst.writestr(name, BLANK_PNG)
    out.seek(0)
    return out


def chart_texts(z, folder):
    # Cached values and titles; formulas (c:f) and number formats are not content.
    return xml_texts(z, rf"{folder}/charts/chart\d+\.xml", ".//c:v") + xml_texts(
        z, rf"{folder}/charts/chart\d+\.xml", ".//a:t"
    )


def reference_pdf(path):
    import pypdfium2 as pdfium

    pdf = pdfium.PdfDocument(str(path))
    try:
        return [page.get_textpage().get_text_bounded() for page in pdf]
    finally:
        pdf.close()


def docx_table(table):
    out = []
    # A merged cell is repeated along its merge, across columns and rows. The elements are
    # kept so that their lxml proxies, and therefore their ids, stay the same.
    seen = {}
    for row in table.rows:
        for cell in row.cells:
            if id(cell._tc) in seen:
                continue
            seen[id(cell._tc)] = cell._tc
            out.extend(p.text for p in cell.paragraphs)
            for t in cell.tables:
                out.extend(docx_table(t))
    return out


def reference_docx(path):
    import docx

    d = docx.Document(without_dangling_relationships(path))
    out = [p.text for p in d.paragraphs]
    for t in d.tables:
        out.extend(docx_table(t))
    parts = {}
    for s in d.sections:
        for hf in (
            s.header,
            s.footer,
            s.first_page_header,
            s.first_page_footer,
            s.even_page_header,
            s.even_page_footer,
        ):
            if not hf.is_linked_to_previous:
                parts[hf.part.partname] = hf
    for hf in parts.values():
        out.extend(p.text for p in hf.paragraphs)
        for t in hf.tables:
            out.extend(docx_table(t))
    with zipfile.ZipFile(path) as z:
        for part in ("word/footnotes.xml", "word/endnotes.xml", "word/comments.xml"):
            if part in z.namelist():
                out.extend(xml_texts(z, re.escape(part), ".//w:t"))
        body = ET.fromstring(z.read("word/document.xml"))
        # Text boxes: python-docx leaves them out of the paragraph text. The VML fallback
        # repeats the same box, so only the boxes outside mc:Fallback count.
        fallback = {
            id(n)
            for f in body.iterfind(".//mc:Fallback", NS)
            for n in f.iter()
        }
        for box in body.iterfind(".//w:txbxContent", NS):
            if id(box) not in fallback:
                out.extend(t.text or "" for t in box.iterfind(".//w:t", NS))
        out.extend(n.text or "" for n in body.iterfind(".//m:t", NS))
        out.extend(n.get("descr", "") for n in body.iterfind(".//wp:docPr", NS))
        out.extend(xml_texts(z, r"word/diagrams/data\d*\.xml", ".//a:t"))
        out.extend(chart_texts(z, "word"))
    return out


def pptx_shapes(shapes):
    from pptx.enum.shapes import MSO_SHAPE_TYPE

    out = []
    for shape in shapes:
        if shape.shape_type == MSO_SHAPE_TYPE.GROUP:
            out.extend(pptx_shapes(shape.shapes))
            continue
        if shape.has_text_frame:
            out.append(shape.text_frame.text)
        if getattr(shape, "has_table", False) and shape.has_table:
            for row in shape.table.rows:
                for cell in row.cells:
                    if not cell.is_spanned:
                        out.append(cell.text)
        descr = shape._element.find(".//p:cNvPr", NS)
        if descr is not None and shape.shape_type == MSO_SHAPE_TYPE.PICTURE:
            out.append(descr.get("descr", ""))
    return out


def reference_pptx(path):
    from pptx import Presentation

    prs = Presentation(without_dangling_relationships(path))
    out = []
    for slide in prs.slides:
        out.extend(pptx_shapes(slide.shapes))
        if slide.has_notes_slide and slide.notes_slide.notes_text_frame is not None:
            out.append(slide.notes_slide.notes_text_frame.text)
    with zipfile.ZipFile(path) as z:
        out.extend(chart_texts(z, "ppt"))
        out.extend(xml_texts(z, r"ppt/diagrams/data\d*\.xml", ".//a:t"))
        out.extend(xml_texts(z, r"ppt/comments/comment\d+\.xml", ".//p:text"))
        out.extend(xml_texts(z, r"ppt/comments/modernComment[^/]*\.xml", ".//a:t"))
    return out


def cell_text(value):
    """A cell value as the workbook stores it, not as Python prints it."""
    if isinstance(value, datetime.datetime) and value.time() == datetime.time(0):
        return value.date().isoformat()
    if isinstance(value, float) and value.is_integer():
        return str(int(value))
    return str(value)


def reference_xlsx(path):
    import openpyxl

    wb = openpyxl.load_workbook(without_dangling_relationships(path), data_only=True)
    out = []
    for ws in wb.worksheets:
        out.append(ws.title)
        for row in ws.iter_rows():
            for cell in row:
                if cell.value is not None:
                    out.append(cell_text(cell.value))
                # A threaded comment leaves a legacy note with a placeholder text and an
                # author `tc={id}`; its real text is read below.
                c = cell.comment
                if c is not None and not (c.author or "").startswith("tc="):
                    out.append(c.text)
    out.extend(cs.title for cs in wb.chartsheets)
    with zipfile.ZipFile(path) as z:
        out.extend(chart_texts(z, "xl"))
        out.extend(xml_texts(z, r"xl/threadedComments/[^/]*\.xml", ".//tc:text"))
    return out


READERS = {
    ".pdf": reference_pdf,
    ".docx": reference_docx,
    ".pptx": reference_pptx,
    ".xlsx": reference_xlsx,
}


# --- Bundle ---------------------------------------------------------------------------


def markdown_text(md):
    fields = []
    if md.startswith("---\n"):
        end = md.find("\n---\n", 4)
        fields = CONTENT_FIELDS.findall(md[: end if end >= 0 else 0])
        md = md[end + 5 :] if end >= 0 else md
    md = re.sub(r"<!--.*?-->", " ", md, flags=re.S)
    md = re.sub(r"\]\([^)]*\)", "] ", md)
    return "\n".join(fields + [md])


def bundle_texts(zip_path):
    out = []
    with zipfile.ZipFile(zip_path) as z:
        for name in sorted(z.namelist()):
            base = name.rsplit("/", 1)[-1]
            if base in ("index.md", "conversion-report.md"):
                continue
            if name.endswith(".md"):
                out.append(markdown_text(z.read(name).decode("utf-8")))
            elif name.endswith(".csv"):
                out.append(z.read(name).decode("utf-8"))
    return out


def convert(binary, doc, workdir):
    attempt = Path(workdir) / "attempt"
    attempt.mkdir()
    run = subprocess.run(
        [str(binary), "convert", str(doc), str(attempt), doc.name],
        capture_output=True,
        text=True,
    )
    if run.returncode != 0:
        raise RuntimeError(f"{doc}: conversion failed: {run.stderr.strip()}")
    status, bundle = run.stdout.strip().splitlines()[-1].split("\t")
    return status, Path(bundle)


# --- Comparison -----------------------------------------------------------------------


def measure(binary, doc):
    reference = collections.Counter(
        w for t in READERS[doc.suffix.lower()](doc) for w in words(t)
    )
    with tempfile.TemporaryDirectory() as tmp:
        status, bundle = convert(binary, doc, tmp)
        sequence = [w for t in bundle_texts(bundle) for w in words(t)]
    produced = collections.Counter(sequence)
    missing = reference - produced
    extra = produced - reference
    total = sum(reference.values())
    # A missing word of four or more characters that appears inside the bundle's text
    # once the spaces are removed was only joined or split differently: pdfium joins
    # words spaced by position and splits words hyphenated at the end of a line.
    stream = "".join(sequence)
    regrouped = sum(n for w, n in missing.items() if len(w) >= 4 and w in stream)
    return {
        "document": str(doc),
        "status": status,
        "reference_words": total,
        "bundle_words": sum(produced.values()),
        "coverage": round(100 * (1 - sum(missing.values()) / total), 2) if total else None,
        "text_coverage": (
            round(100 * (1 - (sum(missing.values()) - regrouped) / total), 2) if total else None
        ),
        "missing_words": sum(missing.values()),
        "extra_words": sum(extra.values()),
        "missing": missing.most_common(25),
        "extra": extra.most_common(25),
    }, reference


def reference_path(doc):
    """Where the reference list of a versioned document goes, or None if it is not one."""
    try:
        rel = doc.resolve().relative_to(ROOT / "examples")
    except ValueError:
        return None
    fmt, *middle, name = rel.parts
    group = middle[0] if middle else "fixtures"
    return ROOT / "examples" / fmt / "cobertura" / group / (Path(name).stem + ".tsv")


def write_reference(path, counts):
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [f"{w}\t{n}\n" for w, n in sorted(counts.items())]
    path.write_text("".join(lines), encoding="utf-8")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("docs", nargs="+", type=Path)
    ap.add_argument("--binary", type=Path, default=ROOT / "target/release/okf-microservice")
    ap.add_argument("--json", type=Path, help="write the full results here")
    ap.add_argument("--write-reference", action="store_true")
    args = ap.parse_args()
    results = []
    print(
        f"{'document':<52} {'status':<9} {'ref':>7} {'bundle':>7} {'words%':>7} {'text%':>7}"
        f" {'miss':>5} {'extra':>6}"
    )
    for doc in args.docs:
        try:
            r, counts = measure(args.binary, doc)
        except Exception as e:  # noqa: BLE001 - report and go on with the others
            print(f"{str(doc):<52} error: {e}")
            continue
        results.append(r)
        print(
            f"{str(doc):<52} {r['status']:<9} {r['reference_words']:>7} {r['bundle_words']:>7}"
            f" {r['coverage'] if r['coverage'] is not None else '-':>7}"
            f" {r['text_coverage'] if r['text_coverage'] is not None else '-':>7}"
            f" {r['missing_words']:>5} {r['extra_words']:>6}"
        )
        if args.write_reference:
            target = reference_path(doc)
            if target is None:
                print(f"  (no reference list: {doc} is outside examples/)", file=sys.stderr)
            else:
                write_reference(target, counts)
    if args.json:
        args.json.write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
