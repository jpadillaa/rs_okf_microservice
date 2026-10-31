"""Build small, versionable copies of the real DOCX corpus used by the acceptance tests.

Every XML part is kept byte for byte, so text, styles, numbering, relationships, fields
and notes are unchanged. Raster media become 1x1 images of the same format and embedded
fonts become empty parts: the converter attaches images without reading their pixels and
never reads fonts. Entries keep their order and get a fixed timestamp, so the output is
deterministic.

Usage: python3 -I examples/docx/reduce_corpus.py datos_de_prueba examples/docx/corpus
"""
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo
import struct
import sys
import zlib

DOCUMENTS = ["d1.docx", "d2.docx", "d3.docx"]


def tiny_png():
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"\x00\x80\x80\x80")) + chunk(b"IEND", b""))


PNG = tiny_png()


def replacement(name):
    lower = name.lower()
    if lower.startswith("word/media/"):
        if lower.endswith(".png"):
            return PNG
        raise SystemExit(f"tipo de recurso sin reemplazo definido: {name}")
    if lower.startswith("word/fonts/"):
        return b""
    return None


def reduce(source, target):
    with ZipFile(source) as src, ZipFile(target, "w", ZIP_DEFLATED) as out:
        for info in src.infolist():
            data = replacement(info.filename)
            if data is None:
                data = src.read(info)
            entry = ZipInfo(info.filename, date_time=(1980, 1, 1, 0, 0, 0))
            entry.compress_type = ZIP_DEFLATED
            out.writestr(entry, data)


def main():
    source_dir, target_dir = map(Path, sys.argv[1:3])
    target_dir.mkdir(parents=True, exist_ok=True)
    for doc in DOCUMENTS:
        reduce(source_dir / doc, target_dir / doc)
        print(f"{doc}: {(source_dir / doc).stat().st_size} -> {(target_dir / doc).stat().st_size} bytes")


if __name__ == "__main__":
    main()
