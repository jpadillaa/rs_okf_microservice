#!/usr/bin/env bash
# Prueba de humo de la imagen (plan 1, R3): la API recibe una muestra de cada formato,
# el worker usa PostgreSQL y el hijo convierte sin sockets y la API entrega los cuatro bundles.
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
api=okf-smoke-api-$RANDOM-$$
postgres=okf-smoke-postgres-$RANDOM-$$
network=okf-smoke-net-$RANDOM-$$
postgres_image=postgres:17-bookworm@sha256:839fbb5a4942a7758dd93e18b42ea5649807491a0545b5eeeb8f0b8bb11857a5
cleanup() {
    docker rm -f "$api" >/dev/null 2>&1 || true
    docker rm -f "$postgres" >/dev/null 2>&1 || true
    docker network rm "$network" >/dev/null 2>&1 || true
    docker volume rm "$volume" >/dev/null 2>&1 || true
    rm -rf "$data" 2>/dev/null || true
}
trap cleanup EXIT
docker volume create "$volume" >/dev/null
docker network create "$network" >/dev/null
docker run -d --name "$postgres" --network "$network" --network-alias postgres \
    -e POSTGRES_USER=okf -e POSTGRES_PASSWORD=smoke_password -e POSTGRES_DB=okf "$postgres_image" >/dev/null
for _ in $(seq 60); do
    docker exec "$postgres" pg_isready -U okf -d okf >/dev/null 2>&1 && break
    sleep 0.5
done
printf '%s\n' 'OKF_DATABASE_URL=postgresql://okf:smoke_password@postgres:5432/okf?sslmode=disable' > "$data/service.env"
chmod 600 "$data/service.env"

docker run -d --name "$api" --network "$network" --env-file "$data/service.env" -p "127.0.0.1:$port:8080" -v "$volume:/data" "$image" \
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

docker run --rm --network "$network" --env-file "$data/service.env" -v "$volume:/data" "$image" worker --once

for id in "${ids[@]}"; do
    job=$(curl -sf "http://127.0.0.1:$port/jobs/$id")
    status=$(jq -r .status <<<"$job")
    jq -e '(.converter_version | type == "string") and (.profile_version | type == "string") and (.converter_commit | test("^([0-9a-f]{40}|unknown)$")) and (.conversion_id | test("^[0-9a-f]{64}$"))' <<<"$job" >/dev/null
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

# OCR local: con OKF_OCR_ENABLED=true, un PDF escaneado sale con texto, dentro de las
# restricciones del hijo (Landlock, seccomp y límite de memoria).
# Se sube con una clave de documento, que se lista y llega al bundle (plan 3, I2).
id=$(curl -sf -F "document_key=smoke:ocr/1" -F "file=@examples/pdf/muestras/escaneado_ocr.pdf" \
    "http://127.0.0.1:$port/jobs" | jq -r .job_id)
docker run --rm --network "$network" --env-file "$data/service.env" -e OKF_OCR_ENABLED=true \
    -v "$volume:/data" "$image" worker --once
job=$(curl -sf "http://127.0.0.1:$port/jobs/$id")
[[ $(jq -r .status <<<"$job") == complete ]] || { echo "OCR: $job" >&2; exit 1; }
curl -sf -o "$data/$id.zip" "http://127.0.0.1:$port/jobs/$id/bundle"
chmod 644 "$data/$id.zip"
docker run --rm --network none -v "$data:/data:ro" "$image" validate "/data/$id.zip"
report=$(unzip -p "$data/$id.zip" conversion-report.md)
grep -q "escaneadas con OCR del servicio: 2" <<<"$report"
content=$(unzip -p "$data/$id.zip" 'contenido/*.md')
grep -q "anonimizada" <<<"$content"
# El hijo lee los modelos para declarar su huella dentro de Landlock (plan 3, I3): cada
# una es la fijada en tools/ocr_models.sh, la misma que el Dockerfile, y no «ausente».
document=$(unzip -p "$data/$id.zip" documento.md)
for file in det.onnx rec.onnx dict.txt; do
    pinned=$(awk -v f="$file" '$1 == f { print $3 }' tools/ocr_models.sh)
    grep -qx "    ${file%%.*}_sha256: \"$pinned\"" <<<"$document" ||
        { echo "OCR: la huella de $file en documento.md no es la fijada ($pinned)" >&2; exit 1; }
done
versions=$(curl -sf "http://127.0.0.1:$port/jobs?document_key=smoke%3Aocr%2F1")
jq -e --arg id "$id" 'length == 1 and .[0].job_id == $id and (.[0].conversion_id | test("^[0-9a-f]{64}$"))' \
    <<<"$versions" >/dev/null || { echo "versiones: $versions" >&2; exit 1; }
grep -qx 'document_key: "smoke:ocr/1"' <<<"$document"
echo "trabajo $id: OCR complete, bundle descargado y validado"
