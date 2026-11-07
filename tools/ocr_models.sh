#!/bin/sh
# Descarga los modelos del OCR local (PaddleOCR en ONNX, Apache-2.0) y verifica su
# SHA-256. Por defecto los deja en models/ocr, el valor de OKF_OCR_MODELS_DIR para el
# binario ejecutado en el host; la imagen los incorpora en su construcción.
#
# Uso: tools/ocr_models.sh [directorio]
set -eu

dir=${1:-models/ocr}
base=https://huggingface.co/monkt/paddleocr-onnx/resolve/7b02d0a30a07ba2b92ad1ff5a8941ae2c633de65

# archivo local, ruta en el repositorio de modelos, SHA-256
models="
det.onnx detection/v3/det.onnx ee40e80071ba3a320d4efda75f3e22047a7d049e9bf7bcaaf9daea23fc21b935
rec.onnx languages/latin/rec.onnx 614ffc2d6d3902d360fad7f1b0dd455ee45e877069d14c4e51a99dc4ef144409
dict.txt languages/latin/dict.txt 3c0a8a79b612653c25f765271714f71281e4e955962c153e272b7b8c1d2b13ff
"

if command -v sha256sum >/dev/null 2>&1; then
    sha() { sha256sum "$1" | cut -d' ' -f1; }
else
    sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
fi

mkdir -p "$dir"
echo "$models" | while read -r file remote expected; do
    [ -n "$file" ] || continue
    target="$dir/$file"
    if [ -f "$target" ] && [ "$(sha "$target")" = "$expected" ]; then
        echo "$target: ya verificado"
        continue
    fi
    curl -fsSL --retry 3 -o "$target.part" "$base/$remote"
    actual=$(sha "$target.part")
    if [ "$actual" != "$expected" ]; then
        rm -f "$target.part"
        echo "$target: SHA-256 $actual, se esperaba $expected" >&2
        exit 1
    fi
    mv "$target.part" "$target"
    echo "$target: descargado y verificado"
done
