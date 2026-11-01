"""Build small, versionable copies of the real PDF corpus used by the acceptance tests.

Page tree, content streams, fonts, bookmarks and annotations are kept, so text,
positions, reading order and page structure are unchanged. Each raster image becomes a
1x1 RGB image: it keeps its place and size on the page (they come from the content
stream) and its colour comes from a hash of the original stream, so an image repeated in
the original (even as separate objects) stays repeated and distinct images stay
distinct. The receipt p3.pdf is not copied because it holds billing data.

Requires pikepdf (pip install pikepdf).
Usage: python -I examples/pdf/reduce_corpus.py datos_de_prueba examples/pdf/corpus
"""
from pathlib import Path
import hashlib
import sys

import pikepdf

DOCUMENTS = ["p1.pdf", "p2.pdf", "p4.pdf"]
DROP = ["/Filter", "/DecodeParms", "/SMask", "/Mask", "/Decode", "/Intent", "/ImageMask", "/Interpolate", "/Alternates", "/Metadata"]


def reduce(src, dst):
    with pikepdf.open(src) as pdf:
        for obj in pdf.objects:
            if isinstance(obj, pikepdf.Stream) and obj.get("/Subtype") == "/Image":
                pixel = hashlib.sha256(obj.read_raw_bytes()).digest()[:3]
                for key in DROP:
                    if key in obj:
                        del obj[key]
                obj.write(pixel)
                obj.Width, obj.Height = 1, 1
                obj.BitsPerComponent = 8
                obj.ColorSpace = pikepdf.Name.DeviceRGB
        # Sin metadatos XMP ni fechas variables, para que la copia sea reproducible.
        if "/Metadata" in pdf.Root:
            del pdf.Root["/Metadata"]
        pdf.save(dst, compress_streams=True, object_stream_mode=pikepdf.ObjectStreamMode.generate,
                 deterministic_id=True, linearize=False)


def main():
    src, dst = Path(sys.argv[1]), Path(sys.argv[2])
    dst.mkdir(parents=True, exist_ok=True)
    for name in DOCUMENTS:
        reduce(src / name, dst / name)
        print(f"{name}: {(src / name).stat().st_size} -> {(dst / name).stat().st_size} bytes")


if __name__ == "__main__":
    main()
