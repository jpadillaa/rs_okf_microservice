#!/usr/bin/env bash
# Prueba de humo de la imagen (plan 1, R3): la API recibe una muestra de cada formato,
# el worker los convierte en un contenedor sin red y la API entrega los cuatro bundles.
# Uso: tools/smoke.sh IMAGEN
set -euo pipefail

image=${1:?uso: tools/smoke.sh IMAGEN}
port=18080
data=$(mktemp -d)
# Los ZIP descargados se validan desde un contenedor de solo lectura (UID 10001).
chmod 755 "$data"
# El almacén usa un volumen Linux como Compose: Landlock no puede garantizar
# las reglas de escritura sobre algunos montajes del host de Docker Desktop.
volume=okf-smoke-$RANDOM-$$
api=okf-smoke-api
cleanup() {
    docker rm -f "$api" >/dev/null 2>&1 || true
    docker volume rm "$volume" >/dev/null 2>&1 || true
    rm -rf "$data" 2>/dev/null || true
}
trap cleanup EXIT
docker volume create "$volume" >/dev/null

docker run -d --name "$api" -p "127.0.0.1:$port:8080" -v "$volume:/data" "$image" \
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

docker run --rm --network none -v "$volume:/data" "$image" worker --once

for id in "${ids[@]}"; do
    job=$(curl -sf "http://127.0.0.1:$port/jobs/$id")
    status=$(jq -r .status <<<"$job")
    jq -e '(.converter_version | type == "string") and (.profile_version | type == "string") and (.converter_commit | test("^([0-9a-f]{40}|unknown)$"))' <<<"$job" >/dev/null
    case $status in
        complete | partial) ;;
        *) echo "trabajo $id: $status" >&2; curl -s "http://127.0.0.1:$port/jobs/$id" >&2; exit 1 ;;
    esac
    curl -sf -o "$data/$id.zip" "http://127.0.0.1:$port/jobs/$id/bundle"
    chmod 644 "$data/$id.zip"
    # Sin tubería hacia grep -q: con pipefail, su cierre anticipado haría fallar unzip.
    listing=$(unzip -Z1 "$data/$id.zip")
    grep -qx documento.md <<<"$listing"
    docker run --rm --network none -v "$data:/data:ro" "$image" validate "/data/$id.zip"
    echo "trabajo $id: $status, bundle descargado y validado"
done

# `--once` ya terminó: no hay worker vivo, y la salud debe reflejarlo.
health=$(curl -s -o "$data/health.json" -w '%{http_code}' "http://127.0.0.1:$port/health")
[[ "$health" == 503 ]]
metrics=$(curl -sf "http://127.0.0.1:$port/metrics")
for format in pptx docx xlsx pdf; do
    grep -E "^okf_conversions_total\{format=\"$format\",result=\"(complete|partial)\"\} 1$" <<<"$metrics" >/dev/null
done
