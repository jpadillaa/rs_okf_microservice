"""Build small, versionable copies of the real PPTX corpus used by the acceptance tests.

Every XML part is kept byte for byte, so text, notes, geometry, relationships and
animations are unchanged. Raster and SVG media become 1x1 images of the same format
and embedded fonts become empty parts: the converter reads neither, and the tests
depend on declared geometry rather than pixels. Entries keep their order and get a
fixed timestamp, so the output is deterministic.

Usage: python3 -I examples/pptx/reduce_corpus.py datos_de_prueba examples/pptx/corpus
"""
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo
import struct
import sys
import zlib

DECKS = ["s1.pptx", "s2.pptx", "s3.pptx"]


def tiny_png():
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"\x00\x80\x80\x80")) + chunk(b"IEND", b""))


PNG = tiny_png()
SVG = b'<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>'


def replacement(name):
    lower = name.lower()
    if lower.startswith("ppt/media/"):
        if lower.endswith(".png"):
            return PNG
        if lower.endswith(".svg"):
            return SVG
        raise SystemExit(f"tipo de recurso sin reemplazo definido: {name}")
    if lower.startswith("ppt/fonts/"):
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
    for deck in DECKS:
        reduce(source_dir / deck, target_dir / deck)
        print(f"{deck}: {(source_dir / deck).stat().st_size} -> {(target_dir / deck).stat().st_size} bytes")


if __name__ == "__main__":
    main()
