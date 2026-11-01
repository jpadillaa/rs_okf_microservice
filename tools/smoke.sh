#!/usr/bin/env bash
# Prueba de humo de la imagen (plan 1, R3): la API recibe una muestra de cada formato,
# el worker los convierte en un contenedor sin red y la API entrega los cuatro bundles.
# Uso: tools/smoke.sh IMAGEN
set -euo pipefail

image=${1:?uso: tools/smoke.sh IMAGEN}
port=18080
data=$(mktemp -d)
# El contenedor escribe como el usuario 10001.
chmod 777 "$data"
api=okf-smoke-api
cleanup() {
    docker rm -f "$api" >/dev/null 2>&1 || true
    rm -rf "$data" 2>/dev/null || true
}
trap cleanup EXIT

docker run -d --name "$api" -p "127.0.0.1:$port:8080" -v "$data:/data" "$image" \
    api --bind 0.0.0.0:8080 >/dev/null
for _ in $(seq 60); do
    curl -s -o /dev/null "http://127.0.0.1:$port/jobs/ninguno" && break
    sleep 0.5
done

ids=()
for sample in examples/pptx/simple.pptx examples/docx/breve.docx \
    examples/xlsx/muestras/ventas_trimestrales.xlsx examples/pdf/muestras/breve.pdf; do
    id=$(curl -sf -F "file=@$sample" "http://127.0.0.1:$port/jobs" | jq -r .job_id)
    echo "$sample → $id"
    ids+=("$id")
done

docker run --rm --network none -v "$data:/data" "$image" worker --once

for id in "${ids[@]}"; do
    status=$(curl -sf "http://127.0.0.1:$port/jobs/$id" | jq -r .status)
    case $status in
        complete | partial) ;;
        *) echo "trabajo $id: $status" >&2; curl -s "http://127.0.0.1:$port/jobs/$id" >&2; exit 1 ;;
    esac
    curl -sf -o "$data/$id.zip" "http://127.0.0.1:$port/jobs/$id/bundle"
    # Sin tubería hacia grep -q: con pipefail, su cierre anticipado haría fallar unzip.
    listing=$(unzip -Z1 "$data/$id.zip")
    grep -qx documento.md <<<"$listing"
    echo "trabajo $id: $status, bundle descargado"
done
