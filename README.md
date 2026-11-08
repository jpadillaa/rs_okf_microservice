# Microservicio OKF

Convierte PPTX, DOCX, XLSX y PDF en bundles OKF. PostgreSQL conserva los trabajos y las métricas; los originales y bundles permanecen en archivos.

Consulta el índice en [`docs/README.md`](docs/README.md).

## PostgreSQL de prueba y configuración

```bash
cp .env.example .env
chmod 600 .env
docker compose up -d --wait postgres
```

La instancia de desarrollo escucha en `127.0.0.1:5432`, con base y usuario `okf` y contraseña `okf_test_password`. La imagen está fijada por digest y los datos persisten en `okf-postgres`. Son valores genéricos para pruebas. La inicialización crea también `okf_test`, separada del servicio.

Edita `.env` para cambiar la conexión:

```dotenv
OKF_DATABASE_URL=postgresql://okf:okf_test_password@127.0.0.1:5432/okf?sslmode=disable
OKF_DATABASE_URL_DOCKER=postgresql://okf:okf_test_password@postgres:5432/okf?sslmode=disable
OKF_TEST_DATABASE_URL=postgresql://okf:okf_test_password@127.0.0.1:5432/okf_test?sslmode=disable
```

`OKF_DATABASE_URL` se usa al ejecutar el binario en el host. Compose pasa `OKF_DATABASE_URL_DOCKER` como conexión de API y worker; el nombre `postgres` se resuelve dentro de Docker. Para una base externa, cambia las URLs correspondientes y configura TLS según su certificado, por ejemplo `sslmode=verify-full`. Los caracteres reservados de usuario/contraseña deben codificarse en la URL. Los valores `POSTGRES_*` inicializan un volumen nuevo; cambiarlos después no modifica usuarios ni contraseñas existentes.

El binario lee `.env` del directorio actual antes de crear hilos. Las variables ya presentes en el entorno tienen prioridad. Se puede elegir otro archivo:

```bash
cargo run -- --env-file /ruta/conexion.env api
cargo run -- --env-file /ruta/conexion.env worker
```

El archivo `.env` está excluido de Git. Todas las variables `OKF_*` se describen en la [arquitectura técnica, §5](docs/arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#5-implementación-en-las-entregas-pptx-docx-xlsx-y-pdf).

## Servicio completo con Docker

```bash
docker compose build api
docker compose up -d --no-build
```

La API queda en `http://127.0.0.1:8080` (`POST /jobs`, `GET /jobs?document_key=…`, `GET /jobs/{id}`, `GET /jobs/{id}/bundle`). API y worker aplican las migraciones de esquema al arrancar, con coordinación entre procesos. Ambos comparten el volumen `okf-data`; deben ver los mismos archivos. El worker alcanza PostgreSQL por una red interna y su proceso conversor bloquea sockets con seccomp en Linux, además de los límites y Landlock.

API y worker usan la misma imagen `okf-microservice:local`: basta construirla una vez. BuildKit y `cargo-chef` reutilizan las dependencias cuando solo cambia el código. La imagen final usa Distroless Debian 13, conserva las fuentes y certificados y ejecuta el binario sin símbolos como UID 10001; no incluye shell ni gestor de paquetes. Para una revisión ya confirmada en Git, añade `--build-arg OKF_BUILD_COMMIT="$(git rev-parse HEAD)"` a `docker compose build`; sin argumento registra `unknown`. Consulta [Compilación y empaquetado Docker](docs/decisiones/COMPILACION_DOCKER.md) para las capas, cachés y mediciones.

## Probar los endpoints con Swagger UI

Con el servicio iniciado, abre [Swagger UI](http://127.0.0.1:8080/docs/). La interfaz incluye las seis operaciones, sus parámetros, respuestas y errores. **Try it out** está activado; expande una operación y pulsa **Execute** para enviar la petición.

1. En `POST /jobs`, selecciona un `.pdf`, `.docx`, `.xlsx` o `.pptx` en `file` (por ejemplo, `examples/docx/breve.docx`) y pulsa **Execute**. La respuesta `202` contiene el `job_id`. El campo opcional `document_key` agrupa las versiones de un mismo documento, que `GET /jobs` lista con esa clave.
2. Copia ese identificador en `id` de `GET /jobs/{id}`. Ejecuta de nuevo mientras el estado sea `queued` o `processing`.
3. Cuando sea `complete` o `partial`, ejecuta `GET /jobs/{id}/bundle` con el mismo `id` y guarda el ZIP desde **Download file**.

`GET /health` comprueba PostgreSQL, almacenamiento y worker; devuelve `503` si el worker no está activo. `GET /metrics` muestra las métricas Prometheus.

El contrato [OpenAPI JSON](http://127.0.0.1:8080/api-docs/openapi.json) se genera desde los handlers y tipos Rust. Swagger sirve sus archivos desde el propio binario, sin CDN ni validador externo; las peticiones se envían al mismo origen que la interfaz. También funciona con `cargo run -- api`, pero debes iniciar `cargo run -- worker` en otra terminal para procesar los trabajos.

Para habilitar o apagar Swagger, cambia esta variable en `.env`:

```dotenv
OKF_SWAGGER_ENABLED=true
```

`true` lo habilita y `false` lo apaga. Si la variable no está definida, se mantiene habilitado; solo se admiten `true` y `false`. Con `false`, `/docs`, `/docs/`, sus archivos y `/api-docs/openapi.json` devuelven `404`. Los cinco endpoints del microservicio siguen disponibles.

El valor se lee al arrancar. En Docker aplica los cambios del archivo recreando la API:

```bash
docker compose up -d --no-deps --force-recreate api
```

`docker compose restart` no recarga las variables del `.env`. Si ejecutas el binario directamente, detén y vuelve a iniciar `cargo run -- api`. Una variable ya exportada en el entorno del binario tiene prioridad sobre el archivo.

## OCR de PDF escaneados

El OCR local transcribe las páginas PDF escaneadas que no tienen capa de texto. Está apagado por defecto; para activarlo, cambia esta variable en `.env`:

```dotenv
OKF_OCR_ENABLED=true
```

Quien sube el archivo no hace nada distinto: el servicio decide qué páginas lo necesitan, y el bundle señala junto al texto y en el informe que procede de OCR, con su confianza. `OKF_OCR_MAX_PAGES` (40) limita las páginas transcritas por documento, y el OCR deja de empezar páginas al consumir dos tercios de `OKF_CONVERSION_TIMEOUT_S` (300 s, unas 12–40 páginas según su densidad): las restantes quedan como vista y el bundle sale `partial`. Para documentos escaneados largos, sube ese plazo. `OKF_OCR_MEMORY_MB` (256) se suma al límite de memoria al convertir un PDF. Lo lee el worker al arrancar: en Docker, recrea el worker con `docker compose up -d --no-deps --force-recreate worker`. La imagen incluye los modelos. Para ejecutar el binario en el host, descárgalos antes en `models/ocr` (la ruta de `OKF_OCR_MODELS_DIR` en `.env.example`):

```bash
tools/ocr_models.sh
```

El script verifica el SHA-256 de cada archivo. Con el OCR activo y sin modelos, el worker no arranca. Motor, mediciones y límites: [OCR local](docs/decisiones/OCR_LOCAL.md).

## Verificación

Con PostgreSQL iniciado y `OKF_TEST_DATABASE_URL` configurada:

```bash
cargo fmt
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo deny check
```

Las pruebas del OCR usan los modelos de `models/ocr` (o de `OKF_OCR_MODELS_DIR`) y se omiten si faltan; `tools/ocr_models.sh` los descarga. En la CI son obligatorias (`OKF_REQUIRE_OCR_MODELS`).

Las pruebas de persistencia crean esquemas aislados dentro de `okf_test`, nunca usan la base del servicio y no requieren SQLite. La CI levanta PostgreSQL antes de ejecutarlas. Los esquemas de prueba son desechables; se pueden eliminar recreando exclusivamente `okf_test` cuando no haya pruebas en curso.
