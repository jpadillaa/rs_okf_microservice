# Microservicio OKF

**Tus documentos, organizados en un paquete que puedes leer, revisar y reutilizar.**

El servicio convierte archivos PDF, Word, Excel y PowerPoint en un **ZIP con texto organizado, recursos, el documento original y un informe de conversión**. Las referencias de cada parte permiten volver al documento original.

```mermaid
flowchart LR
    accTitle: De un documento a un paquete OKF
    accDescr: Un archivo PDF, DOCX, XLSX o PPTX entra al microservicio, que extrae y organiza su contenido y entrega un ZIP descargable.
    d1_input["📄 PDF · DOCX<br/>📊 XLSX · PPTX"] --> d1_service("<b>Microservicio OKF</b><br/>Extrae y organiza")
    d1_service --> d1_output["📦 <b>ZIP descargable</b><br/>Texto + recursos + original"]
    classDef input fill:#DBEAFE,stroke:#2563EB,color:#1E3A8A,stroke-width:2px;
    classDef process fill:#EDE9FE,stroke:#7C3AED,color:#4C1D95,stroke-width:2px;
    classDef output fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:2px;
    class d1_input input;
    class d1_service process;
    class d1_output output;
    linkStyle default stroke:#64748B,stroke-width:2px;
```

**OKF** significa *Open Knowledge Format*. Aquí llamamos **bundle** al paquete de archivos que sigue ese formato. El texto se guarda en **Markdown** (`.md`): archivos de texto con títulos, listas, tablas y enlaces.

Puedes leer el resultado directamente o entregarlo a un sistema de búsqueda o a un asistente de IA. Ese sistema se encarga de indexar el contenido y responder preguntas.

**Elige por dónde empezar:** [ver el resultado](#qué-vas-a-recibir) · [ponerlo en marcha](#servicio-completo-con-docker) · [convertir tu primer archivo](#probar-los-endpoints-con-swagger-ui) · [consultar la documentación técnica](docs/README.md).

## Qué vas a recibir

Piensa en el ZIP como una carpeta de lectura con su material de respaldo. **Empieza por `index.md` para navegar y por `conversion-report.md` para conocer las advertencias.**

```mermaid
flowchart LR
    accTitle: Qué hay dentro del ZIP
    accDescr: El paquete incluye un índice, una ficha del documento, el contenido convertido, los recursos cuando los hay, el original y un informe de conversión.
    subgraph d2_zip["📦 Tu paquete OKF"]
        direction TB
        d2_index["<b>index.md</b><br/>La puerta de entrada"] ~~~ d2_doc["<b>documento.md</b><br/>Ficha y estructura"]
        d2_doc ~~~ d2_content["<b>contenido/</b><br/>Texto organizado"]
        d2_assets["<b>assets/</b><br/>Imágenes y datos, si los hay"] ~~~ d2_original["<b>references/</b><br/>El archivo original"]
        d2_original ~~~ d2_report["<b>conversion-report.md</b><br/>Resultado y advertencias"]
    end
    classDef output fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:2px;
    classDef input fill:#DBEAFE,stroke:#2563EB,color:#1E3A8A,stroke-width:2px;
    classDef review fill:#FEF3C7,stroke:#B45309,color:#78350F,stroke-width:2px;
    class d2_index,d2_doc,d2_content,d2_assets output;
    class d2_original input;
    class d2_report review;
    style d2_zip fill:#F5F3FF,stroke:#7C3AED,color:#4C1D95,stroke-width:2px;
```

La organización depende del formato del archivo que envías:

| Si subes… | Encontrarás… | Ten en cuenta… |
| --- | --- | --- |
| 📄 **PDF** | Texto por secciones, referencias a páginas y recursos extraídos. | Para transcribir escaneos sin capa de texto, activa el OCR. |
| 📝 **Word · DOCX** | Secciones con títulos, listas, tablas, imágenes y notas. | Las referencias señalan bloques del documento, no páginas de Word. |
| 📊 **Excel · XLSX** | Hojas y regiones de datos; las regiones grandes incluyen archivos CSV y, cuando corresponde, detalle por celda. | Conserva las fórmulas y sus valores guardados; no recalcula el libro. |
| 🖼️ **PowerPoint · PPTX** | Una parte por diapositiva, con texto, notas y una vista PNG. | Las vistas son estáticas; las animaciones no se reproducen. |

El conversor extrae y organiza el contenido sin resumirlo. Sus notas se distinguen del texto original. Si detecta pérdidas o incertidumbre, las declara en el informe.

Puedes explorar [los documentos de ejemplo](examples/) y sus carpetas `esperado/` para ver resultados de referencia. Las reglas completas están en la [especificación del bundle](docs/especificacion/ESPECIFICACION_OKF.md).

## Cómo funciona

Hay dos protagonistas: la **API**, que recibe tus peticiones, y el **worker**, que hace la conversión en segundo plano. Esto permite subir un archivo y consultar su avance sin mantener abierta la petición inicial.

```mermaid
flowchart LR
    accTitle: El recorrido de una conversión
    accDescr: La API recibe y guarda el archivo, registra un trabajo en la cola, el worker lo convierte y valida, y finalmente publica el ZIP si supera las comprobaciones.
    d3_api("<b>1. Recibir</b><br/>Guardar el archivo") --> d3_queue[("<b>2. Esperar turno</b><br/>Trabajo en cola")]
    d3_queue --> d3_worker("<b>3. Convertir</b><br/>Extraer y comprobar")
    d3_worker -->|Si es válido| d3_publish("<b>4. Publicar</b><br/>ZIP listo")
    classDef input fill:#DBEAFE,stroke:#2563EB,color:#1E3A8A,stroke-width:2px;
    classDef process fill:#EDE9FE,stroke:#7C3AED,color:#4C1D95,stroke-width:2px;
    classDef output fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:2px;
    class d3_api input;
    class d3_queue,d3_worker process;
    class d3_publish output;
    linkStyle default stroke:#64748B,stroke-width:2px;
```

Al aceptar el archivo, la API entrega un **`job_id`**, el código de seguimiento de esa conversión. Con él consultas el estado y descargas el resultado.

Cada worker procesa un trabajo a la vez. El trabajo y sus archivos se conservan entre reinicios.

### Cómo interpretar el estado

```mermaid
flowchart TD
    accTitle: Estados de un trabajo y qué hacer en cada caso
    accDescr: Un trabajo pasa de queued a processing. Puede terminar complete, partial o failed. Un fallo temporal puede devolverlo a la cola para un reintento limitado.
    d4_queue("<b>queued</b><br/>Esperando turno") --> d4_work("<b>processing</b><br/>Convirtiendo y comprobando")
    d4_work --> d4_complete["✅ <b>complete</b><br/>Sin pérdidas detectadas<br/>Puedes descargar"]
    d4_work --> d4_partial["⚠️ <b>partial</b><br/>Hay pérdidas o incertidumbre<br/>Descarga y revisa el informe"]
    d4_work --> d4_failed["⛔ <b>failed</b><br/>La conversión falló<br/>Consulta el error"]
    d4_work -.->|Fallo temporal: reintento limitado| d4_queue
    classDef input fill:#DBEAFE,stroke:#2563EB,color:#1E3A8A,stroke-width:2px;
    classDef process fill:#EDE9FE,stroke:#7C3AED,color:#4C1D95,stroke-width:2px;
    classDef output fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:2px;
    classDef review fill:#FEF3C7,stroke:#B45309,color:#78350F,stroke-width:2px;
    classDef error fill:#FFE4E6,stroke:#BE123C,color:#881337,stroke-width:2px;
    class d4_queue input;
    class d4_work process;
    class d4_complete output;
    class d4_partial review;
    class d4_failed error;
    linkStyle default stroke:#64748B,stroke-width:2px;
```

**`partial` sí permite descargar el ZIP.** Por ejemplo, un PDF puede conservar una página escaneada como imagen aunque no haya podido transcribirla. El informe explica esa pérdida. En `failed` no hay un bundle publicado.

Los archivos se conservan indefinidamente por defecto. Si activas la retención con `OKF_RETENTION_DAYS`, los trabajos antiguos pasan a `expired` y su descarga responde `410`: el archivo ya se eliminó.

## Servicio completo con Docker

**Necesitas Docker con Compose, acceso a Internet para la primera construcción y una copia de este repositorio.** Docker prepara el entorno; no necesitas instalar Rust para esta ruta.

Desde la carpeta del proyecto:

```bash
# Crea la configuración solo si todavía no existe.
test -f .env || cp .env.example .env
chmod 600 .env

# Construye la imagen que comparten API y worker.
docker compose build api

# Inicia PostgreSQL, API y worker.
docker compose up -d --no-build
```

La primera construcción puede tardar porque compila el servicio y descarga los modelos de OCR. Las siguientes aprovechan la caché.

### Qué acabas de poner en marcha

```mermaid
flowchart LR
    accTitle: Despliegue local con Docker Compose
    accDescr: El navegador habla con la API en localhost 8080. API y worker comparten PostgreSQL para los trabajos y un volumen de archivos para originales y resultados. Compose ejecuta tres servicios en el mismo equipo.
    d5_browser["🌐 <b>Tu navegador</b><br/>localhost:8080"] <--> d5_api
    subgraph d5_local["En tu equipo · Docker Compose"]
        d5_api("<b>API</b><br/>Recibe y entrega")
        d5_worker("<b>Worker</b><br/>Convierte en segundo plano")
        d5_db[("<b>PostgreSQL</b><br/>Trabajos y métricas")]
        d5_files[("<b>Archivos compartidos</b><br/>Originales, intentos y ZIP")]
        d5_api <-->|Registra y consulta| d5_db
        d5_worker <-->|Reclama y actualiza| d5_db
        d5_api <-->|Guarda y descarga| d5_files
        d5_worker <-->|Lee y publica| d5_files
    end
    classDef input fill:#DBEAFE,stroke:#2563EB,color:#1E3A8A,stroke-width:2px;
    classDef process fill:#EDE9FE,stroke:#7C3AED,color:#4C1D95,stroke-width:2px;
    classDef output fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:2px;
    class d5_browser,d5_api input;
    class d5_worker,d5_db process;
    class d5_files output;
    style d5_local fill:#F8FAFC,stroke:#94A3B8,color:#334155,stroke-width:1px;
    linkStyle default stroke:#64748B,stroke-width:2px;
```

**Tres servicios, dos lugares para guardar datos:** PostgreSQL conserva los trabajos y las métricas en `okf-postgres`; el volumen `okf-data` conserva los documentos. API y worker deben ver los mismos archivos. Al arrancar, ambos coordinan la actualización de las tablas de la base de datos.

Comprueba que están activos con `docker compose ps`. Después abre [la salud del servicio](http://127.0.0.1:8080/health): `status: "ok"` indica que la base, el almacenamiento y el worker responden.

El acceso al servicio se limita a `127.0.0.1` en esta configuración, pensada para un solo equipo. Para dar acceso desde otra red, necesitas una puerta de entrada que autentique a los usuarios: el servicio no incluye autenticación propia.

<details>
<summary><strong>Detalles del despliegue: imagen, aislamiento y actualización</strong></summary>

API y worker usan la misma imagen `okf-microservice:local`; basta construirla una vez. BuildKit y `cargo-chef` reutilizan las dependencias cuando solo cambia el código.

La imagen final usa Distroless Debian 13, con fuentes, certificados y modelos OCR incluidos. Ejecuta el binario sin símbolos como UID 10001 y no contiene shell ni gestor de paquetes. Las imágenes base están fijadas por digest, que identifica su contenido exacto.

El worker accede a PostgreSQL por una red interna. Cada conversión se ejecuta en un proceso separado, con límites de memoria y tiempo. En Linux, seccomp bloquea las conexiones de red del proceso de conversión y Landlock restringe las rutas a las que este puede acceder.

Para registrar qué revisión de Git produjo la imagen, construye una revisión ya confirmada con:

```bash
docker compose build --build-arg OKF_BUILD_COMMIT="$(git rev-parse HEAD)" api
```

Sin ese argumento, la imagen registra `unknown`. Después de cambiar el código, reconstruye y recrea API y worker para utilizar la versión nueva:

```bash
docker compose build api
docker compose up -d --no-build --force-recreate api worker
```

Consulta [Compilación y empaquetado Docker](docs/decisiones/COMPILACION_DOCKER.md) para las capas, cachés y mediciones.

</details>

## Probar los endpoints con Swagger UI

**Swagger UI es la pantalla de pruebas de la API.** Abre [http://127.0.0.1:8080/docs/](http://127.0.0.1:8080/docs/) con el servicio iniciado. Cada *endpoint* es una dirección de la API que permite realizar una operación, como subir o descargar un archivo.

```mermaid
flowchart LR
    accTitle: Tu primera conversión desde el navegador
    accDescr: Sube un documento mediante POST jobs, copia el job_id, consulta GET jobs id hasta complete o partial y descarga el ZIP mediante GET jobs id bundle.
    d6_upload("<b>1. Sube</b><br/>POST /jobs<br/>Recibes un job_id") --> d6_check("<b>2. Consulta</b><br/>GET /jobs/{id}<br/>Espera complete o partial")
    d6_check --> d6_download("<b>3. Descarga</b><br/>GET /jobs/{id}/bundle<br/>Abre el ZIP y su informe")
    classDef input fill:#DBEAFE,stroke:#2563EB,color:#1E3A8A,stroke-width:2px;
    classDef process fill:#EDE9FE,stroke:#7C3AED,color:#4C1D95,stroke-width:2px;
    classDef output fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:2px;
    class d6_upload input;
    class d6_check process;
    class d6_download output;
    linkStyle default stroke:#64748B,stroke-width:2px;
```

1. **Sube un ejemplo.** Expande `POST /jobs`, pulsa **Try it out** si aparece, elige `examples/docx/breve.docx` en el campo `file` y pulsa **Execute**. Puedes dejar vacíos los campos opcionales. La respuesta `202` significa «aceptado» e incluye el `job_id`.
2. **Consulta el avance.** Copia ese identificador en `id` de `GET /jobs/{id}` y pulsa **Execute**. Repite mientras veas `queued` o `processing`. Si aparece `failed`, consulta el campo `error`.
3. **Descarga el resultado.** Cuando veas `complete` o `partial`, usa el mismo identificador en `GET /jobs/{id}/bundle`. Pulsa **Execute** y guarda el ZIP desde **Download file**. Descomprímelo y abre `index.md` y `conversion-report.md`.

<details>
<summary><strong>Las seis operaciones y los campos opcionales</strong></summary>

| Quiero… | Operación |
| --- | --- |
| Enviar un documento | `POST /jobs` |
| Ver las versiones de un mismo documento | `GET /jobs?document_key=…` |
| Consultar una conversión | `GET /jobs/{id}` |
| Descargar su resultado | `GET /jobs/{id}/bundle` |
| Comprobar que el servicio funciona | `GET /health` |
| Consultar métricas de operación | `GET /metrics` |

En la subida puedes añadir:

- **`document_key`:** una clave que agrupa versiones del mismo documento, por ejemplo `manual-equipo`. El servicio lista sus trabajos en orden de llegada; no elige cuál es la versión vigente.
- **`access`:** permisos procedentes de otro sistema, escritos como JSON. El servicio los conserva en el resultado; no los aplica como control de acceso a la API.

`GET /health` devuelve `503` si alguna comprobación falla, por ejemplo, si el worker está inactivo. `GET /metrics` entrega métricas en el formato de Prometheus, una herramienta de monitorización.

El contrato [OpenAPI JSON](http://127.0.0.1:8080/api-docs/openapi.json) se genera desde el código. Swagger sirve todos sus recursos desde el propio binario, sin CDN ni validador externo; las peticiones van al mismo servicio que muestra la interfaz.

</details>

<details>
<summary><strong>Mostrar u ocultar Swagger UI</strong></summary>

En `.env`, `OKF_SWAGGER_ENABLED=true` habilita la interfaz y `false` la apaga. Si no defines la variable, queda habilitada. Solo se admiten esos dos valores.

Con `false`, `/docs`, `/docs/`, sus recursos y `/api-docs/openapi.json` responden `404`. Las seis operaciones del servicio siguen disponibles.

Después de cambiar la variable, recrea la API:

```bash
docker compose up -d --no-deps --force-recreate api
```

`docker compose restart` no recarga las variables de `.env`. Si ejecutas el binario directamente, detén y vuelve a iniciar `cargo run -- api`. Una variable ya exportada en el entorno del binario tiene prioridad sobre el archivo.

</details>

## OCR de PDF escaneados

Un escaneo contiene una imagen de las letras. El **OCR** intenta reconocerlas y convertirlas en texto. Se ejecuta localmente y está **apagado por defecto**.

```mermaid
flowchart TD
    accTitle: Qué ocurre con una página escaneada
    accDescr: Una página escaneada sin capa de texto se conserva como vista si el OCR está apagado. Con OCR activo se intenta reconocer el texto; si no se consigue o se agota el presupuesto, se conserva la vista y se informa de la pérdida.
    d7_scan["📄 <b>Página escaneada</b><br/>Sin capa de texto"] -->|OCR activo| d7_ocr("<b>Reconocimiento local</b><br/>Intenta leer las letras")
    d7_scan -->|OCR apagado| d7_view["🖼️ <b>Vista de la página</b><br/>Aviso en el informe"]
    d7_ocr -->|Texto reconocido| d7_text["📝 <b>Texto transcrito</b><br/>Con procedencia y confianza"]
    d7_ocr -->|Sin texto o sin presupuesto| d7_view
    classDef input fill:#DBEAFE,stroke:#2563EB,color:#1E3A8A,stroke-width:2px;
    classDef process fill:#EDE9FE,stroke:#7C3AED,color:#4C1D95,stroke-width:2px;
    classDef output fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:2px;
    classDef review fill:#FEF3C7,stroke:#B45309,color:#78350F,stroke-width:2px;
    class d7_scan input;
    class d7_ocr process;
    class d7_text output;
    class d7_view review;
    linkStyle default stroke:#64748B,stroke-width:2px;
```

Para activarlo, cambia esta línea en `.env`:

```dotenv
OKF_OCR_ENABLED=true
```

Después recrea el worker; la imagen Docker ya incluye los modelos que necesita:

```bash
docker compose up -d --no-deps --force-recreate worker
```

**Subes el PDF de la misma manera.** El servicio selecciona las páginas sin capa de texto que contienen una imagen grande; no aplica OCR a las páginas mixtas con texto digital. El resultado identifica el texto obtenido por OCR e indica la confianza del reconocimiento. Si el servicio no logra transcribir una página, el resultado queda en `partial`; una confianza baja también puede producir ese estado.

<details>
<summary><strong>Límites del OCR y ejecución fuera de Docker</strong></summary>

| Ajuste | Valor inicial | Para qué sirve |
| --- | ---: | --- |
| `OKF_OCR_MAX_PAGES` | 40 | Máximo de páginas que se intentan transcribir por documento. |
| `OKF_CONVERSION_TIMEOUT_S` | 300 segundos | Plazo de la conversión. El OCR deja de iniciar páginas al consumir dos tercios de ese plazo. |
| `OKF_OCR_MEMORY_MB` | 256 MB | Memoria adicional al límite general cuando se convierte un PDF con OCR activo. |

Con el plazo inicial, el rendimiento medido permite unas 12–40 páginas, según su densidad. Para escaneos largos puede ser necesario ampliar el plazo. Si se agota el presupuesto, las páginas restantes quedan como vistas y el bundle sale `partial`.

Para usar OCR al ejecutar el binario en tu equipo, descarga antes los modelos:

```bash
tools/ocr_models.sh
```

Se guardan en `models/ocr`, la ruta de `OKF_OCR_MODELS_DIR` en `.env.example`. El script verifica sus huellas SHA-256 para comprobar que sean los archivos esperados. Con el OCR activo y sin modelos, el worker no arranca.

El OCR está orientado a texto impreso en alfabeto latino. Sus mediciones, ajustes y límites están en [OCR local](docs/decisiones/OCR_LOCAL.md).

</details>

## Si algo no sale como esperabas

| Lo que ves | Qué revisar |
| --- | --- |
| El trabajo sigue en `queued` | Comprueba que el worker esté activo: `docker compose ps` y `/health`. |
| El resultado es `partial` | Descarga el ZIP y lee las incidencias de `conversion-report.md`. |
| El resultado es `failed` | Consulta `error` en `GET /jobs/{id}` y los registros con `docker compose logs worker`. |
| Swagger responde `404` | Revisa `OKF_SWAGGER_ENABLED` y recrea la API tras cambiar `.env`. |
| Falta una operación nueva en Swagger | Reconstruye la imagen y recrea API y worker; el contenedor puede ejecutar una versión anterior. |
| La descarga responde `410` | El trabajo expiró por la política de retención. |

## PostgreSQL de prueba y configuración

La instalación con Docker usa la configuración de desarrollo incluida en `.env.example`. Abre los siguientes apartados si necesitas cambiar la conexión o trabajar con Rust directamente.

<details>
<summary><strong>Conexiones, credenciales de desarrollo y archivo .env</strong></summary>

PostgreSQL escucha en `127.0.0.1:5432`, con base y usuario `okf` y contraseña `okf_test_password`. Son valores genéricos para pruebas. Sus datos persisten en `okf-postgres`; la inicialización crea también `okf_test`, separada del servicio. La imagen está fijada por digest.

En `.env` hay tres conexiones porque cada una se usa desde un lugar distinto:

```dotenv
OKF_DATABASE_URL=postgresql://okf:okf_test_password@127.0.0.1:5432/okf?sslmode=disable
OKF_DATABASE_URL_DOCKER=postgresql://okf:okf_test_password@postgres:5432/okf?sslmode=disable
OKF_TEST_DATABASE_URL=postgresql://okf:okf_test_password@127.0.0.1:5432/okf_test?sslmode=disable
```

| Variable | Quién la utiliza |
| --- | --- |
| `OKF_DATABASE_URL` | El binario ejecutado directamente en tu equipo. |
| `OKF_DATABASE_URL_DOCKER` | Compose la pasa a API y worker como `OKF_DATABASE_URL`; `postgres` es el nombre interno de la base. |
| `OKF_TEST_DATABASE_URL` | Las pruebas de persistencia, en su propia base desechable. |

Para una base externa, cambia las conexiones correspondientes y configura TLS según su certificado; por ejemplo, `sslmode=verify-full`. Los caracteres reservados del usuario o contraseña deben codificarse en la URL.

Los valores `POSTGRES_*` inicializan un volumen nuevo. Cambiarlos después no modifica los usuarios ni las contraseñas de una base ya creada.

El binario lee `.env` del directorio actual antes de crear hilos. Las variables ya presentes en el entorno tienen prioridad. `.env` está excluido de Git. La [arquitectura técnica, §5](docs/arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#5-implementación-en-las-entregas-pptx-docx-xlsx-y-pdf) describe todas las variables `OKF_*`.

</details>

<details>
<summary><strong>Ejecutar API y worker con Rust en tu equipo</strong></summary>

Necesitas Rust 1.99 o posterior y una fuente TrueType para las vistas PPTX. Puedes indicar la fuente con `OKF_FONT_PATH`. Prepara `.env` como en la instalación con Docker e inicia solo la base:

```bash
docker compose up -d --wait postgres
```

Si ya tienes API y worker de Compose activos, detenlos antes de iniciar los procesos locales para evitar conflictos:

```bash
docker compose stop api worker
```

En una terminal inicia la API y, en otra, el worker:

```bash
# Terminal 1
cargo run --locked -- api
```

```bash
# Terminal 2
cargo run --locked -- worker
```

Swagger UI usa la misma dirección: [http://127.0.0.1:8080/docs/](http://127.0.0.1:8080/docs/). Si activas OCR, descarga primero sus modelos con `tools/ocr_models.sh`.

También puedes elegir un archivo de configuración distinto:

```bash
cargo run --locked -- --env-file /ruta/conexion.env api
cargo run --locked -- --env-file /ruta/conexion.env worker
```

</details>

## Verificación

El proyecto compara sus conversiones con resultados de referencia y mide cuánto texto conserva frente a lectores independientes. También prueba el servicio, la recuperación de trabajos y el rechazo de bundles alterados. El [estado del proyecto](docs/estado/ESTADO_DEL_PROYECTO.md) reúne la evidencia y las limitaciones conocidas.

<details>
<summary><strong>Ejecutar las comprobaciones de desarrollo</strong></summary>

Con Rust instalado, PostgreSQL en ejecución y `OKF_TEST_DATABASE_URL` configurada:

```bash
cargo fmt
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo deny check
```

`cargo deny` es una herramienta adicional de auditoría de dependencias; debe estar instalada para ejecutar el último comando.

Las pruebas del OCR usan los modelos de `models/ocr` o de `OKF_OCR_MODELS_DIR`. Estas pruebas se omiten si faltan los modelos; `tools/ocr_models.sh` los descarga. Para exigirlas también en tu equipo:

```bash
OKF_REQUIRE_OCR_MODELS=1 cargo test --locked
```

En la CI son obligatorias. Las pruebas de persistencia crean esquemas aislados dentro de `okf_test`; nunca usan la base del servicio ni requieren SQLite. La CI levanta PostgreSQL antes de ejecutarlas. Esos esquemas son desechables: se pueden eliminar recreando exclusivamente `okf_test` cuando no haya pruebas en curso.

</details>

## Para seguir explorando

| Si quieres entender… | Abre… |
| --- | --- |
| Qué está hecho y qué falta | [Estado del proyecto](docs/estado/ESTADO_DEL_PROYECTO.md) |
| Las reglas exactas del resultado | [Especificación OKF](docs/especificacion/ESPECIFICACION_OKF.md) |
| Los componentes y las decisiones de operación | [Arquitectura técnica](docs/arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md) |
| Cómo se mide la conservación del texto | [Cobertura independiente](docs/decisiones/COBERTURA_INDEPENDIENTE.md) |
| El mapa completo de documentación | [Índice de documentación](docs/README.md) |
