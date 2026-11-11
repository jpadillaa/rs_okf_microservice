# Microservicio OKF

**Documentos organizados en un paquete para leer, revisar y reutilizar.**

El servicio convierte archivos PDF, Word, Excel y PowerPoint en un **ZIP con texto organizado, recursos, el documento original y un informe de conversión**. Las referencias de cada parte permiten volver al documento original.

**OKF** significa *Open Knowledge Format*. Aquí se llama **bundle** al paquete de archivos que sigue ese formato. El texto se guarda en **Markdown** (`.md`): archivos de texto con títulos, listas, tablas y enlaces.

## Un contrato común de salida: las mismas reglas para todos los formatos

El **contrato común de salida** es el acuerdo que dice: **«sin importar si recibimos un PDF, un Word, un Excel o un PowerPoint, entregaremos el resultado siguiendo las mismas reglas»**.

Cuatro proveedores pueden enviar productos distintos y usar el mismo sistema de cajas, etiquetas e inventario. Así, quien recibe el envío sabe dónde encontrar cada cosa. En este proyecto, esa “caja” es el bundle OKF.

El contrato establece cómo indicar:

- **Qué contiene cada parte:** su título, texto, tablas o recursos.
- **De dónde salió:** la página del PDF, el bloque de Word, las celdas de Excel o la diapositiva.
- **Cómo identificarla:** a qué conversión y versión del archivo pertenece.
- **Cómo se relaciona con las demás:** qué parte la contiene y cuáles vienen antes o después.
- **Qué problemas hubo:** contenido incompleto, referencias sin destino o elementos que no se pudieron extraer.

Por ejemplo, el origen de una tabla de Excel puede indicarse como «hoja Ventas, celdas A1:D20» y el de un párrafo de PDF como «página 7». La ubicación se expresa según cada formato, pero ambos contenidos se entregan dentro de una organización común.

**Esto permite que un sistema RAG —que busca información en los documentos para responder preguntas— lea los resultados con las mismas reglas, conserve el contexto y pueda citar el original.** Con esa información, el sistema también puede distinguir versiones y conocer las limitaciones de la conversión antes de utilizar el contenido. Las reglas completas están en la [especificación del bundle](docs/especificacion/ESPECIFICACION_OKF.md).

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

El resultado se puede leer directamente o entregar a un sistema de búsqueda o a un asistente de IA. Ese sistema se encarga de indexar el contenido y responder preguntas.

**Por dónde empezar:** [entender el contrato común](#un-contrato-común-de-salida-las-mismas-reglas-para-todos-los-formatos) · [ver el resultado](#qué-contiene-el-resultado) · [leer los metadatos YAML](#cómo-leer-los-metadatos-yaml) · [ponerlo en marcha](#servicio-completo-con-docker) · [convertir el primer archivo](#probar-los-endpoints-con-swagger-ui) · [consultar la documentación técnica](docs/README.md).

## Qué contiene el resultado

El ZIP funciona como una carpeta de lectura con su material de respaldo. **`index.md` permite navegar por el contenido y `conversion-report.md` recoge las advertencias.**

```mermaid
flowchart LR
    accTitle: Qué hay dentro del ZIP
    accDescr: El paquete incluye un índice, una ficha del documento, el contenido convertido, los recursos cuando los hay, el original y un informe de conversión.
    subgraph d2_zip["📦 Paquete OKF"]
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

Dentro de esa estructura común, el contenido se organiza según el formato del archivo de entrada:

| Formato de entrada | Contenido del resultado | Consideraciones |
| --- | --- | --- |
| 📄 **PDF** | Texto por secciones, referencias a páginas y recursos extraídos. | Para transcribir escaneos sin capa de texto, se debe activar el OCR. |
| 📝 **Word · DOCX** | Secciones con títulos, listas, tablas, imágenes y notas. | Las referencias señalan bloques del documento, no páginas de Word. |
| 📊 **Excel · XLSX** | Hojas y regiones de datos; las regiones grandes incluyen archivos CSV y, cuando corresponde, detalle por celda. | Conserva las fórmulas y sus valores guardados; no recalcula el libro. |
| 🖼️ **PowerPoint · PPTX** | Una parte por diapositiva, con texto, notas y una vista PNG. | Las vistas son estáticas; las animaciones no se reproducen. |

El conversor extrae y organiza el contenido sin resumirlo. Sus notas se distinguen del texto original. Si detecta pérdidas o incertidumbre, las declara en el informe.

En [los documentos de ejemplo](examples/) y sus carpetas `esperado/` se pueden consultar resultados de referencia. Las reglas completas están en la [especificación del bundle](docs/especificacion/ESPECIFICACION_OKF.md).

## Cómo leer los metadatos YAML

Al comienzo de `documento.md`, de cada archivo de `contenido/` y de `conversion-report.md` hay un encabezado entre dos líneas `---`. Se llama **frontmatter** y está escrito en **YAML**, un formato que organiza datos mediante nombres de campo, valores, listas y sangría. Después del segundo `---` comienza el cuerpo del documento en Markdown. Estos archivos se denominan **conceptos** en OKF.

**Estos metadatos hacen explícito el contrato común de salida:** permiten identificar cada unidad, localizar su origen, recuperar su contexto y conocer cómo se produjo. Los campos dependen del archivo: `documento.md` describe el conjunto, los archivos de `contenido/` describen sus unidades y el informe registra el resultado de la conversión.

Este ejemplo procede del [texto DOCX breve de referencia](examples/docx/esperado/fixtures/breve/contenido/texto.md). Se muestra solo el localizador general y se usa una fecha ilustrativa:

```yaml
---
type: "Source Text"
unit_id: "b50a3bf11592a55cec4caa203d549d21ac966cb70336d8236a18e6ec03835cb7/contenido/texto.md"
parent: "documento.md"
title: "breve"
description: "Texto completo de breve.docx. Nota breve"
sources:
  - id: "original"
    resource: "/references/original.docx"
generated:
  by: "process:document-conversion"
  at: "2026-11-09T15:00:00Z"
source_spans:
  - id: "seccion"
    source_id: "original"
    kind: "docx_blocks"
    from: 1
    to: 5
---
```

El ejemplo describe una unidad que depende de `documento.md` y procede de los bloques 1 a 5 del Word original. `source_id: "original"` la vincula con la fuente declarada en `sources`. La fecha de `generated.at` corresponde a la conversión.

### Contenido, procedencia y relaciones

En las tablas siguientes, el punto indica un campo anidado y `[]` indica cada elemento de una lista. Por ejemplo, `sources[].resource` es el campo `resource` de cada fuente.

| Campo | Significado |
| --- | --- |
| `type` | Clase de unidad: `Source Document` para el documento completo, `Source Text` o `Source Section` para texto, `Slide` para una diapositiva, `Sheet` o `Sheet Region` para Excel y `Conversion Report` para el informe. |
| `title` | Título de la unidad. Su procedencia depende del formato; puede tomarse del original, de sus propiedades o del nombre del archivo, o inferirse según las reglas del perfil. |
| `description` | Descripción breve escrita por el conversor, que puede incorporar el título o el comienzo del texto extraído. Debe interpretarse como una descripción de la unidad, no como una cita literal del original. |
| `unit_id` | Identificador de la unidad, formado por el identificador de conversión y la ruta del archivo dentro del bundle. También aparece en `documento.md` y en el informe. |
| `sources[].id` y `sources[].resource` | Identificador local de la fuente y ruta de la copia del original incluida en el bundle. |
| `source_spans[]` | Lugares del original que corresponden a la unidad o a sus pasajes. Cada entrada tiene un `id`, un `source_id` que enlaza con `sources`, un `kind` que indica el tipo de ubicación y los datos para localizarla. |
| `generated.by` y `generated.at` | Proceso que produjo el archivo Markdown y fecha de esa conversión, con zona horaria. |
| `parent`, `previous` y `next` | Unidad que contiene el concepto y, cuando existen, unidades anterior y siguiente con el mismo `parent`, en orden de lectura. Son rutas relativas a la raíz del bundle y se declaran en los conceptos de `contenido/`. |

El significado de la ubicación cambia según `source_spans[].kind`:

| Valor de `kind` | Cómo se localiza el contenido |
| --- | --- |
| `pdf_pages` | `from` y `to` indican páginas físicas, desde 1 e incluidas ambas. Los localizadores de bloques añaden `boxes`, con la página y las coordenadas del área ocupada, y van en el archivo `contenido/<concepto>.spans.json` que el concepto declara en `block_spans`, para que el frontmatter no quede dominado por ellos. |
| `docx_blocks` | `from` y `to` indican bloques en el orden estructural del Word, desde 1 e incluidos ambos. Son bloques, no páginas renderizadas. |
| `xlsx_range` | `sheet` identifica la hoja y `range` el rango de celdas, por ejemplo `A1:D20`. |
| `pptx_slide` | `slide_index` indica la posición de la diapositiva, desde 1 e incluidas las ocultas. `slide_id` conserva su identificador interno cuando existe. |
| `pptx_shape` | `slide_index` y `shape_id` identifican un elemento dentro de una diapositiva; se añaden su posición y tamaño cuando están disponibles. |

En PDF, `page_labels` en `documento.md` relaciona las páginas físicas con los números impresos cuando se reconocen. Así, una página física puede tener una etiqueta visible distinta.

Los marcadores del cuerpo, como `<!-- okf-source-span: b-2 -->`, vinculan el bloque siguiente con el `id` correspondiente de `source_spans` (en PDF, el del archivo `block_spans`). **Un pasaje se identifica mediante el `unit_id` de su concepto y el `id` de su localizador.** Las notas añadidas por el conversor se distinguen con `<!-- okf-generated -->`, aplicado al bloque siguiente.

### Estado y señales de revisión

El significado de `status` depende del archivo que lo declara:

| Campo y ubicación | Interpretación |
| --- | --- |
| `status: draft` en un concepto de contenido | La unidad requiere revisión por ambigüedad, contenido incompleto o texto obtenido por OCR. |
| `status: complete` en el informe | El bundle es válido y no se detectaron pérdidas ni incertidumbres sustantivas pendientes. |
| `status: partial` en el informe | El bundle es válido, pero tiene pérdidas o incertidumbres sustantivas declaradas en el informe. |
| `title_inferred: true` | El título se dedujo de la estructura o del aspecto del contenido. Esta señal, por sí sola, no vuelve incompleta la conversión. |
| `visual_only: true` en una diapositiva | El contenido sustantivo está en una imagen sin transcripción; el concepto también lleva `status: draft`. |
| `view_hides_text: true` en una diapositiva | La vista estática oculta texto que sí se conserva en el contenido extraído. |

Un concepto obtenido con OCR puede llevar `status: draft` y pertenecer a un bundle `complete` si no se detectan pérdidas ni incertidumbres sustantivas. El estado `failed` corresponde al trabajo de conversión: en ese caso no hay un bundle publicado. El informe detalla la cobertura, las incidencias y las observaciones en su cuerpo Markdown.

<details>
<summary><strong>Identidad del documento, versiones, configuración y permisos</strong></summary>

Los siguientes campos permiten comparar conversiones y distinguir documentos y versiones. Se encuentran en `documento.md` y en el informe, salvo donde se indica otra ubicación. Una **huella SHA-256** es un valor calculado a partir del contenido de un archivo o de unos datos; permite detectar si cambiaron.

| Campo | Significado |
| --- | --- |
| `source_file.path` y `source_file.sha256` | En `documento.md`, ruta de la copia del original y huella de sus bytes. El informe repite esa misma huella en `sha256`. |
| `document_key` | Clave opcional recibida del sistema de origen para agrupar versiones de un mismo documento. Se conserva sin interpretarla; el servicio no decide cuál es la versión vigente. |
| `conversion_id` | Identificador calculado a partir de la huella del original, la versión del perfil y la configuración que afecta a la salida. Es distinto del `job_id`, que sirve para seguir una ejecución del servicio. |
| `profile_version` | Versión de las reglas documentales de este proyecto; actualmente, `1.16.0`. |
| `converter_version` y `converter_commit` | Versión del programa y revisión del código que realizó la conversión. El commit puede ser `unknown` si no se proporcionó al construir el programa. |
| `conversion_settings` | Configuración que afecta al contenido del resultado. En PDF incluye el límite de píxeles y la configuración del OCR; en PPTX, la huella de la fuente usada para las vistas. En DOCX y XLSX es `{}`. |
| `settings_sha256` | Huella de esa configuración, para comprobar si dos conversiones usaron los mismos valores. |
| `reproducible: false` | El OCR agotó su plazo y dejó páginas sin transcribir. Repetir la conversión con la misma configuración puede producir otro resultado. |
| `access` | Permisos opcionales recibidos del sistema de origen, conservados como una cadena con el JSON original. Su presencia no añade control de acceso a la API. |
| `format` | En el informe, formato del archivo recibido: `pdf`, `docx`, `xlsx` o `pptx`. |
| `okf_version` | En el `index.md` raíz, versión del formato OKF: `0.2`. Es independiente de `profile_version` y de la versión del programa. |

El mismo original, con el mismo perfil y la misma configuración, conserva sus identificadores de conversión y de unidad. Estos identificadores no incorporan el commit del conversor: una corrección del programa puede conservarlos, por lo que también conviene comparar `converter_commit`. Si aparece `reproducible: false`, el identificador tampoco garantiza que el resultado sea idéntico al repetir la conversión.

Los campos opcionales, como `document_key` y `access`, aparecen solo cuando se reciben o se cumplen sus condiciones. La [especificación del perfil](docs/especificacion/ESPECIFICACION_OKF.md) define las reglas completas y el [esquema del frontmatter](docs/especificacion/perfil-1.17.0.schema.json) permite comprobar los campos y sus tipos automáticamente.

</details>

## Cómo funciona

Hay dos protagonistas: la **API**, que recibe las peticiones, y el **worker**, que hace la conversión en segundo plano. Esto permite subir un archivo y consultar su avance sin mantener abierta la petición inicial.

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

Al aceptar el archivo, la API entrega un **`job_id`**, el código de seguimiento de esa conversión. Con él se consulta el estado y se descarga el resultado.

Cada worker procesa un trabajo a la vez. El trabajo y sus archivos se conservan entre reinicios.

### Cómo interpretar el estado

```mermaid
flowchart TD
    accTitle: Estados de un trabajo y qué hacer en cada caso
    accDescr: Un trabajo pasa de queued a processing. Puede terminar complete, partial o failed. Un fallo temporal puede devolverlo a la cola para un reintento limitado.
    d4_queue("<b>queued</b><br/>Esperando turno") --> d4_work("<b>processing</b><br/>Convirtiendo y comprobando")
    d4_work --> d4_complete["✅ <b>complete</b><br/>Sin pérdidas detectadas<br/>Descarga disponible"]
    d4_work --> d4_partial["⚠️ <b>partial</b><br/>Hay pérdidas o incertidumbre<br/>Descargar y revisar el informe"]
    d4_work --> d4_failed["⛔ <b>failed</b><br/>La conversión falló<br/>Consultar el error"]
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

**`partial` sí permite descargar el ZIP.** Por ejemplo, el conversor puede conservar una página escaneada de un PDF como imagen aunque no haya podido transcribirla. El informe explica esa pérdida. En `failed` no hay un bundle publicado.

Los archivos se conservan indefinidamente por defecto. Si se activa la retención con `OKF_RETENTION_DAYS`, los trabajos antiguos pasan a `expired` y su descarga responde `410`: el archivo ya se eliminó.

## Servicio completo con Docker

**Se necesitan Docker con Compose, acceso a Internet para la primera construcción y una copia de este repositorio.** Docker prepara el entorno; no es necesario instalar Rust para esta ruta.

Desde la carpeta del proyecto:

```bash
# Crear la configuración solo si todavía no existe.
test -f .env || cp .env.example .env
chmod 600 .env

# Construir la imagen que comparten API y worker.
docker compose build api

# Iniciar PostgreSQL, API y worker.
docker compose up -d --no-build
```

La primera construcción puede tardar porque incluye la compilación del servicio y la descarga de los modelos de OCR. Las siguientes aprovechan la caché.

### Servicios en ejecución

```mermaid
flowchart LR
    accTitle: Despliegue local con Docker Compose
    accDescr: El navegador habla con la API en localhost 8080. API y worker comparten PostgreSQL para los trabajos y un volumen de archivos para originales y resultados. Compose ejecuta tres servicios en el mismo equipo.
    d5_browser["🌐 <b>Navegador</b><br/>localhost:8080"] <--> d5_api
    subgraph d5_local["Equipo local · Docker Compose"]
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

Para comprobar que están activos, se usa `docker compose ps`. Después se consulta [la salud del servicio](http://127.0.0.1:8080/health): `status: "ok"` indica que la base de datos, el almacenamiento y el worker responden.

El acceso al servicio se limita a `127.0.0.1` en esta configuración, pensada para un solo equipo. Para dar acceso desde otra red, se necesita una puerta de entrada que autentique a los usuarios: el servicio no incluye autenticación propia.

<details>
<summary><strong>Detalles del despliegue: imagen, aislamiento y actualización</strong></summary>

API y worker usan la misma imagen `okf-microservice:local`; basta construirla una vez. BuildKit y `cargo-chef` reutilizan las dependencias cuando solo cambia el código.

La imagen final usa Distroless Debian 13, con fuentes, certificados y modelos OCR incluidos. Ejecuta el binario sin símbolos como UID 10001 y no contiene shell ni gestor de paquetes. Las imágenes base están fijadas por digest, que identifica su contenido exacto.

El worker accede a PostgreSQL por una red interna. Cada conversión se ejecuta en un proceso separado, con límites de memoria y tiempo. En Linux, seccomp bloquea las conexiones de red del proceso de conversión y Landlock restringe las rutas a las que este puede acceder.

Para registrar qué revisión de Git produjo la imagen, se construye a partir de una revisión ya confirmada con:

```bash
docker compose build --build-arg OKF_BUILD_COMMIT="$(git rev-parse HEAD)" api
```

Sin ese argumento, la imagen registra `unknown`. Después de cambiar el código, se reconstruye la imagen y se recrean API y worker para utilizar la versión nueva:

```bash
docker compose build api
docker compose up -d --no-build --force-recreate api worker
```

En [Compilación y empaquetado Docker](docs/decisiones/COMPILACION_DOCKER.md) se describen las capas, las cachés y las mediciones.

</details>

## Probar los endpoints con Swagger UI

**Swagger UI es la pantalla de pruebas de la API.** Con el servicio iniciado, la interfaz está disponible en [http://127.0.0.1:8080/docs/](http://127.0.0.1:8080/docs/). Cada *endpoint* es una dirección de la API que permite realizar una operación, como subir o descargar un archivo.

```mermaid
flowchart LR
    accTitle: Primera conversión desde el navegador
    accDescr: Subir un documento mediante POST jobs, copiar el job_id, consultar GET jobs id hasta complete o partial y descargar el ZIP mediante GET jobs id bundle.
    d6_upload("<b>1. Subir</b><br/>POST /jobs<br/>Se recibe un job_id") --> d6_check("<b>2. Consultar</b><br/>GET /jobs/{id}<br/>Esperar complete o partial")
    d6_check --> d6_download("<b>3. Descargar</b><br/>GET /jobs/{id}/bundle<br/>Abrir el ZIP y su informe")
    classDef input fill:#DBEAFE,stroke:#2563EB,color:#1E3A8A,stroke-width:2px;
    classDef process fill:#EDE9FE,stroke:#7C3AED,color:#4C1D95,stroke-width:2px;
    classDef output fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:2px;
    class d6_upload input;
    class d6_check process;
    class d6_download output;
    linkStyle default stroke:#64748B,stroke-width:2px;
```

1. **Subir un ejemplo.** Expandir `POST /jobs`, pulsar **Try it out** si aparece, elegir `examples/docx/breve.docx` en el campo `file` y pulsar **Execute**. **Solo `file` es obligatorio**: `document_key` y `access` aparecen vacíos y pueden dejarse así, con **Send empty value** desmarcado. La respuesta `202` significa «aceptado» e incluye el `job_id`.
2. **Consultar el avance.** Copiar ese identificador en `id` de `GET /jobs/{id}` y pulsar **Execute**. Repetir mientras el estado sea `queued` o `processing`. Si aparece `failed`, consultar el campo `error`.
3. **Descargar el resultado.** Cuando el estado sea `complete` o `partial`, usar el mismo identificador en `GET /jobs/{id}/bundle`. Pulsar **Execute** y guardar el ZIP desde **Download file**. Descomprimirlo y abrir `index.md` y `conversion-report.md`.

<details>
<summary><strong>Las seis operaciones y los campos opcionales</strong></summary>

| Objetivo | Operación |
| --- | --- |
| Enviar un documento | `POST /jobs` |
| Ver las versiones de un mismo documento | `GET /jobs?document_key=…` |
| Consultar una conversión | `GET /jobs/{id}` |
| Descargar su resultado | `GET /jobs/{id}/bundle` |
| Comprobar que el servicio funciona | `GET /health` |
| Consultar métricas de operación | `GET /metrics` |

Al subir el archivo se pueden añadir:

- **`document_key`:** una clave que agrupa versiones del mismo documento, por ejemplo `manual-equipo`. El servicio lista sus trabajos en orden de llegada; no elige cuál es la versión vigente.
- **`access`:** permisos procedentes de otro sistema, escritos como JSON. El servicio los conserva en el resultado; no los aplica como control de acceso a la API.

`GET /health` devuelve `503` si alguna comprobación falla, por ejemplo, si el worker está inactivo. `GET /metrics` entrega métricas en el formato de Prometheus, una herramienta de monitorización.

El contrato [OpenAPI JSON](http://127.0.0.1:8080/api-docs/openapi.json) se genera desde el código. Swagger sirve todos sus recursos desde el propio binario, sin CDN ni validador externo; las peticiones van al mismo servicio que muestra la interfaz.

</details>

<details>
<summary><strong>Mostrar u ocultar Swagger UI</strong></summary>

En `.env`, `OKF_SWAGGER_ENABLED=true` habilita la interfaz y `false` la apaga. Si no se define la variable, queda habilitada. Solo se admiten esos dos valores.

Con `false`, `/docs`, `/docs/`, sus recursos y `/api-docs/openapi.json` responden `404`. Las seis operaciones del servicio siguen disponibles.

Después de cambiar la variable, se recrea la API:

```bash
docker compose up -d --no-deps --force-recreate api
```

`docker compose restart` no recarga las variables de `.env`. Si el binario se ejecuta directamente, es necesario detener el proceso y volver a iniciarlo con `cargo run -- api`. Una variable ya exportada en el entorno del binario tiene prioridad sobre el archivo.

</details>

## OCR de PDF escaneados

Un escaneo contiene una imagen de las letras. El **OCR** intenta reconocerlas y convertirlas en texto. Se ejecuta localmente y está **activo por defecto**.

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

La imagen Docker ya incluye los modelos que necesita. Para apagarlo, se cambia esta línea en `.env`:

```dotenv
OKF_OCR_ENABLED=false
```

Después se recrea el worker:

```bash
docker compose up -d --no-deps --force-recreate worker
```

**El PDF se sube de la misma manera.** El servicio selecciona las páginas sin capa de texto que contienen una imagen grande; no aplica OCR de página a las páginas mixtas con texto digital. Además transcribe el texto de las imágenes de contenido (capturas de pantalla, diagramas, tablas pegadas como imagen) y lo coloca detrás de cada imagen, marcado como reconocido por OCR. El resultado identifica el texto obtenido por OCR e indica la confianza del reconocimiento. Si el servicio no logra transcribir una página, el resultado queda en `partial`; una confianza baja también puede producir ese estado.

<details>
<summary><strong>Límites del OCR y ejecución fuera de Docker</strong></summary>

| Ajuste | Valor inicial | Para qué sirve |
| --- | ---: | --- |
| `OKF_OCR_MAX_PAGES` | 40 | Máximo de páginas que se intentan transcribir por documento. |
| `OKF_CONVERSION_TIMEOUT_S` | 300 segundos | Plazo de la conversión. El OCR deja de iniciar páginas al consumir dos tercios de ese plazo o del límite de CPU (`OKF_MAX_CPU_S`). |
| `OKF_OCR_MAX_IMAGES` | 300 | Máximo de imágenes de contenido cuyo texto se transcribe por documento. |
| `OKF_OCR_MEMORY_MB` | 512 MB | Memoria adicional al límite general cuando se convierte un PDF con OCR activo. |

Con el plazo inicial, el rendimiento medido permite unas 12–40 páginas, según su densidad.

`OKF_CONVERSION_THREADS` reparte la lectura de texto, las imágenes y las vistas de un PDF entre varios hilos sin cambiar el bundle; cada hilo necesita memoria para su copia del documento. `OKF_MAX_CPU_S` fija el tiempo de CPU de una conversión, sumados sus hilos; sin declarar, es el plazo por el número de hilos. Para escaneos largos hay que ampliar el plazo y la memoria: uno de 200 páginas se transcribe completo con 4 hilos, `OKF_CONVERSION_TIMEOUT_S=900` y `OKF_MAX_MEMORY_MB=1500` (unos 6 minutos y 2 GB). Si se agota el presupuesto, las páginas restantes quedan como vistas y el bundle sale `partial`.

Para usar OCR al ejecutar el binario en el equipo local, primero se descargan los modelos:

```bash
tools/ocr_models.sh
```

Se guardan en `models/ocr`, la ruta de `OKF_OCR_MODELS_DIR` en `.env.example`. El script verifica sus huellas SHA-256 para comprobar que sean los archivos esperados. Con el OCR activo y sin modelos, el worker no arranca.

El OCR está orientado a texto impreso en alfabeto latino. Sus mediciones, ajustes y límites están en [OCR local](docs/decisiones/OCR_LOCAL.md).

</details>

## Resolución de problemas

| Situación | Qué revisar |
| --- | --- |
| El trabajo sigue en `queued` | Comprobar que el worker esté activo: `docker compose ps` y `/health`. |
| El resultado es `partial` | Descargar el ZIP y leer las incidencias de `conversion-report.md`. |
| El resultado es `failed` | Consultar `error` en `GET /jobs/{id}` y los registros con `docker compose logs worker`. |
| Swagger responde `404` | Revisar `OKF_SWAGGER_ENABLED` y recrear la API tras cambiar `.env`. |
| Falta una operación nueva en Swagger | Reconstruir la imagen y recrear API y worker; el contenedor puede ejecutar una versión anterior. |
| La descarga responde `410` | El trabajo expiró por la política de retención. |

## PostgreSQL de prueba y configuración

La instalación con Docker usa la configuración de desarrollo incluida en `.env.example`. Los siguientes apartados explican cómo cambiar la conexión o trabajar con Rust directamente.

<details>
<summary><strong>Conexiones, credenciales de desarrollo y archivo .env</strong></summary>

PostgreSQL escucha en `127.0.0.1:5432`, con base y usuario `okf` y contraseña `okf_test_password`. Son valores genéricos para pruebas. Sus datos persisten en `okf-postgres`; la inicialización crea también la base `okf_test`, separada de la base del servicio. La imagen está fijada por digest.

En `.env` hay tres conexiones porque cada una se usa desde un lugar distinto:

```dotenv
OKF_DATABASE_URL=postgresql://okf:okf_test_password@127.0.0.1:5432/okf?sslmode=disable
OKF_DATABASE_URL_DOCKER=postgresql://okf:okf_test_password@postgres:5432/okf?sslmode=disable
OKF_TEST_DATABASE_URL=postgresql://okf:okf_test_password@127.0.0.1:5432/okf_test?sslmode=disable
```

| Variable | Quién la utiliza |
| --- | --- |
| `OKF_DATABASE_URL` | El binario ejecutado directamente en el equipo local. |
| `OKF_DATABASE_URL_DOCKER` | Compose la pasa a API y worker como `OKF_DATABASE_URL`; `postgres` es el nombre interno de la base. |
| `OKF_TEST_DATABASE_URL` | Las pruebas de persistencia, en su propia base desechable. |

Para una base externa, se cambian las conexiones correspondientes y se configura TLS según su certificado; por ejemplo, `sslmode=verify-full`. Los caracteres reservados del nombre de usuario o de la contraseña deben codificarse en la URL.

Los valores `POSTGRES_*` inicializan un volumen nuevo. Cambiarlos después no modifica los usuarios ni las contraseñas de una base ya creada.

El binario lee `.env` del directorio actual antes de crear hilos. Las variables ya presentes en el entorno tienen prioridad. `.env` está excluido de Git. La [arquitectura técnica, §5](docs/arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#5-implementación-en-las-entregas-pptx-docx-xlsx-y-pdf) describe todas las variables `OKF_*`.

</details>

<details>
<summary><strong>Ejecutar API y worker con Rust en el equipo local</strong></summary>

Se necesitan Rust 1.99 o posterior y una fuente TrueType para las vistas PPTX. La fuente puede indicarse con `OKF_FONT_PATH`. Se prepara `.env` como en la instalación con Docker y se inicia solo la base:

```bash
docker compose up -d --wait postgres
```

Si API y worker de Compose ya están activos, deben detenerse antes de iniciar los procesos locales para evitar conflictos:

```bash
docker compose stop api worker
```

Se inicia la API en una terminal y el worker en otra:

```bash
# Terminal 1
cargo run --locked -- api
```

```bash
# Terminal 2
cargo run --locked -- worker
```

Swagger UI usa la misma dirección: [http://127.0.0.1:8080/docs/](http://127.0.0.1:8080/docs/). Si se activa OCR, primero se descargan sus modelos con `tools/ocr_models.sh`.

También se puede elegir un archivo de configuración distinto:

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

Las pruebas del OCR usan los modelos de `models/ocr` o de `OKF_OCR_MODELS_DIR`. Estas pruebas se omiten si faltan los modelos; `tools/ocr_models.sh` los descarga. Para exigirlas también en el equipo local:

```bash
OKF_REQUIRE_OCR_MODELS=1 cargo test --locked
```

En la CI son obligatorias. Las pruebas de persistencia crean esquemas aislados dentro de `okf_test`; nunca usan la base del servicio ni requieren SQLite. La CI levanta PostgreSQL antes de ejecutarlas. Esos esquemas son desechables: se pueden eliminar recreando exclusivamente `okf_test` cuando no haya pruebas en curso.

</details>

## Para seguir explorando

| Tema | Documento |
| --- | --- |
| Qué está hecho y qué falta | [Estado del proyecto](docs/estado/ESTADO_DEL_PROYECTO.md) |
| Las reglas exactas del resultado | [Especificación OKF](docs/especificacion/ESPECIFICACION_OKF.md) |
| Los componentes y las decisiones de operación | [Arquitectura técnica](docs/arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md) |
| Cómo se mide la conservación del texto | [Cobertura independiente](docs/decisiones/COBERTURA_INDEPENDIENTE.md) |
| El mapa completo de documentación | [Índice de documentación](docs/README.md) |
