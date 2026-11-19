# Microservicio de segmentación de bundles OKF para RAG

Propuesta de diseño del 19 de noviembre de 2026. La versión `1.0` del contrato del segmentador consume el perfil documental OKF `1.23.0`.

El servicio recibe un bundle OKF y produce fragmentos de texto que un índice de búsqueda puede incorporar. Cada fragmento conserva una ruta de regreso al archivo original, con la precisión que permite la fuente. Por ejemplo, al recuperar el importe de una venta, el consumidor podrá mostrar el valor junto con su hoja y celda. El mismo contrato permite señalar un párrafo en un PDF o una forma en una diapositiva.

El conversor extrae y valida el documento. El segmentador decide qué texto agrupar y cómo citarlo. El indexador posterior calcula embeddings y gestiona qué versión del documento está vigente. El sistema de recuperación aplica los permisos al servir resultados. [Perfil OKF documental](ESPECIFICACION_OKF.md) · [Alcance del contrato del productor](../planes/PLAN_3_CONTRATO_DE_SALIDA_PARA_RAG.md)

## 1. Contrato de entrada

**`POST /v1/segmentations`** recibe `Content-Type: application/zip`. El cuerpo es el ZIP completo que publica `GET /jobs/{id}/bundle` en el conversor. La operación del MVP es síncrona. Quien invoca el servicio conserva el bundle y su asociación con el sistema de origen mientras necesite resolver citas. El segmentador puede trabajar con ambos servicios desplegados por separado.

| Dato | Fuente y regla de admisión |
| --- | --- |
| Formato y versión | `index.md` declara OKF `0.2`. Se admite el perfil `1.23.0` y los formatos `pdf`, `docx`, `pptx` y `xlsx`. Un perfil anterior o posterior recibe el diagnóstico `unsupported_profile` para que el cliente distinga incompatibilidad de corrupción. |
| Identidad | `documento.md` y `conversion-report.md` deben concordar en perfil, conversor, configuración, `conversion_id` y, cuando existan, `document_key`, `access` y `reproducible`. Se comprueban la huella del original incluido, la derivación de `conversion_id` y el `unit_id` de cada concepto según el perfil. |
| Estado de origen | Se aceptan `complete` y `partial` del frontmatter del informe. Un bundle parcial válido aporta evidencia utilizable. Su estado y su informe acompañan el resultado. Un trabajo fallido del conversor carece de bundle publicable. |
| Integridad | Se exige la conformidad del bundle con el perfil, incluido su esquema, relaciones, marcadores, localizadores y recursos referenciados. Las rutas de las entradas ZIP deben ser internas y únicas. Un enlace del perfil como `/contenido/texto.spans.json` se resuelve desde la raíz **del bundle**. Los enlaces externos se tratan como datos. [Esquema admitido](perfil-1.23.0.schema.json) |

### Respuestas HTTP

| HTTP | Resultado |
| --- | --- |
| `200 application/zip` | Resultado completo de la operación descrito en la sección 2, incluso si el origen es parcial o hay cero fragmentos. Se termina de validar la salida antes de comenzar la respuesta. |
| `400` / `415` | `invalid_archive` para un cuerpo vacío o ZIP ilegible, o `unsupported_media_type` si el tipo de contenido es incorrecto. |
| `413` | `limit_exceeded` si la entrada supera los límites documentados del despliegue en bytes comprimidos, bytes expandidos o número de entradas. |
| `422` | `unsupported_profile`, `unsupported_format` o `invalid_bundle`. Este último incluye una huella discordante, una ruta insegura, un marcador roto o la ausencia de un recurso obligatorio. |
| `503` | `temporarily_unavailable` si el servicio no puede terminar por capacidad o por un fallo transitorio. Es seguro reenviar el mismo ZIP. |

Los errores se devuelven como `application/json` con `code` y `message` de tipo cadena, por ejemplo `{"code":"invalid_bundle","message":"El archivo de localizadores declarado por el concepto está ausente."}`. El mensaje puede identificar la ruta interna afectada. El contenido del original y el valor de `access` se mantienen fuera de los diagnósticos.

## 2. Contrato de salida

El ZIP contiene tres archivos en su raíz. Los JSON usan UTF-8. `chunks.jsonl` tiene un objeto JSON por línea, en orden documental. El manifiesto y los fragmentos forman una entrega indivisible.

| Archivo | Qué recibe el consumidor |
| --- | --- |
| `manifest.json` | Identidad de la entrada y de la estrategia, estado de origen y de segmentación, inventario de unidades y recuento de fragmentos. |
| `chunks.jsonl` | Texto listo para indexar, contexto de lectura y citas resolubles. Un archivo de cero bytes representa cero fragmentos. |
| `conversion-report.md` | Copia exacta del informe de origen, con sus incidencias y observaciones. Se conserva para consulta bajo los mismos permisos del documento. |

### Manifiesto

| Campo | Tipo y significado |
| --- | --- |
| `contract_version`, `strategy` | Cadenas. En el MVP valen `1.0` y `estructura-1`. |
| `bundle_sha256`, `result_id` | Cadenas hexadecimales. Identifican el ZIP de entrada exacto y el resultado según la sección 3. |
| `format`, `profile_version`, `converter_version`, `converter_commit`, `conversion_id`, `settings_sha256` | Cadenas tomadas del bundle validado. |
| `source_file`, `document_title` | Objeto `{path, sha256}` y título de `documento.md`. La ruta resuelve dentro del ZIP identificado por `bundle_sha256`. |
| `document_key`, `access` | Cadenas opcionales, presentes exactamente cuando las aporta el bundle. `access` conserva, una vez decodificada la cadena JSON, los mismos bytes UTF-8 del valor de origen, incluidos sus espacios. |
| `source_status`, `reproducible` | `complete` o `partial` del informe y booleano que vale `false` si el productor lo declara. En el perfil admitido, la ausencia de esa marca se representa como `true`. |
| `segmentation_status`, `chunk_count` | `complete` si se representó todo el texto elegible según la sección 4. `partial` si hubo texto elegible que no pudo citarse. El entero es el número exacto de líneas de `chunks.jsonl`. |
| `units` | Lista de todos los conceptos de `contenido/`, incluidos los que produzcan cero fragmentos. Cada entrada contiene `unit_id`, `title`, `parent`, `chunk_count` y `notices`. Se copian las rutas `parent`, `previous` y `next` del frontmatter cuando correspondan. |

Cada elemento de `notices` tiene `code` y `detail` de tipo cadena, y `span_id` cuando existe. `excluded_by_policy` identifica contenido fuera de la selección editorial del MVP, `no_text` indica ausencia de texto elegible, `coarse_citation` señala una cita a nivel de concepto y `uncitable_text` registra una omisión de texto elegible sin localizador aplicable. Solo el último determina `segmentation_status: partial`. Un concepto de imagen sin texto puede tener cero fragmentos y una segmentación completa. Un marcador presente que apunta a un identificador inexistente invalida la entrada.

### Fragmento y cita

| Campo | Tipo y regla |
| --- | --- |
| `chunk_id`, `unit_id`, `ordinal` | Identificador del fragmento, identificador OKF del concepto y entero desde 1, consecutivo dentro de ese concepto. |
| `text` | Cadena no vacía. Es el texto que se cuenta en tokens y se entrega para búsqueda. |
| `channel`, `headings` | Canal `main`, `footnote` o `speaker_notes` y lista de títulos de la unidad y subsecciones, de mayor a menor nivel. Los títulos facilitan la búsqueda contextual. La evidencia citable se identifica en `citations`. |
| `columns` | Lista opcional de encabezados de columna, en orden, cuando se divide una fila o tabla y su cabecera queda fuera de `text`. |
| `flags` | Lista, posiblemente vacía. Admite `draft`, `title_inferred`, `ocr`, `visual_only` y `view_hides_text` cuando los declara el bundle para el contenido o su unidad. Una señal de OCR a nivel de unidad se conserva con ese alcance. |
| `document_key`, `access`, `source_status` | Misma presencia y valores que en el manifiesto. Esto permite cargar fragmentos por separado y aplicar la política de acceso del origen. |
| `citations` | Lista no vacía, en orden de aparición en `text`. Cada dato o pasaje sustantivo debe tener al menos una cita. Un fragmento puede contener varios pasajes del mismo concepto. |

Una cita contiene `start_byte` y `end_byte`, intervalo UTF-8 de `text` con inicio incluido y fin excluido, y `span`, copia del localizador OKF con `id`, `source_id`, `kind` y sus campos por formato. Debe cumplirse `0 ≤ start_byte < end_byte ≤ longitud UTF-8 de text` y ambos extremos deben caer en límites de caracteres. La clave `(unit_id, span.id)` resuelve el pasaje y `span.source_id` resuelve el recurso original. Los separadores de presentación pueden quedar fuera de los intervalos.

`precision` vale `span` para un bloque, una forma o una región identificada, `unit` cuando la evidencia solo ubica el concepto completo, y `cell` para una celda o subrango de Excel demostrado. Con `cell` se exige `cell_range` en notación A1 dentro de `span.range`. En una cita a una celda se añade `cell_record` si el recurso JSONL contiene su registro.

**Ejemplo de una línea de `chunks.jsonl`**, expandida para lectura. El texto y la caja corresponden a la primera sección de la muestra pública del informe técnico. [Concepto](../../examples/pdf/esperado/muestras/informe_tecnico/contenido/seccion-001.md) · [Localizadores](../../examples/pdf/esperado/muestras/informe_tecnico/contenido/seccion-001.spans.json)

```json
{
  "chunk_id": "28e4791dec7cfa6c9ab81dab3b27e4c49df6a712c3f8d7e425eb9b96b81dace4",
  "unit_id": "d8b51eab7d207020c18dc867e465894eb1663eec190e1759a67b7428e45108f5/contenido/seccion-001.md",
  "ordinal": 1,
  "text": "Este informe describe la migración ficticia de un sistema de inventario a una nueva plataforma. Todos los nombres, cifras y fechas son artificiales y solo sirven para probar la conversión de documentos.",
  "channel": "main",
  "headings": ["Informe técnico de migración"],
  "flags": [],
  "source_status": "complete",
  "citations": [{
    "start_byte": 0, "end_byte": 204, "precision": "span",
    "span": {
      "id": "u-2", "source_id": "original", "kind": "pdf_pages", "from": 1, "to": 1,
      "boxes": [{"page": 1, "x0": 72.00, "top": 104.53, "x1": 512.54, "bottom": 142.03}]
    }
  }]
}
```

## 3. Identidad, permisos y versiones

Sea `H` el SHA-256 de los bytes, en hexadecimal minúsculo. En las fórmulas siguientes se codifica el texto en UTF-8, `LF` es un byte `0x0a`, `||` concatena y `decimal` escribe un entero sin ceros iniciales.

```text
bundle_sha256 = H(bytes del ZIP recibido)
result_id     = H("okf-segmentation-v1" || LF || strategy || LF || bundle_sha256)
chunk_id      = H("okf-chunk-v1" || LF || strategy || LF || unit_id || LF || decimal(ordinal) || LF || text)
```

Reintentos y peticiones simultáneas con el mismo ZIP y estrategia producen el mismo manifiesto lógico, los mismos fragmentos y el mismo orden. Reempaquetar el ZIP puede cambiar `result_id` y conservar los `chunk_id`. Cuando el OCR declara `reproducible: false`, dos conversiones pueden compartir `conversion_id` y diferir en contenido. La huella del ZIP distingue los artefactos concretos. Todo cambio en las reglas que pueda alterar texto, límites, citas u orden exige una nueva `strategy`.

El consumidor incorpora cada fragmento bajo la clave `(ámbito de origen, result_id, chunk_id)`. El ámbito lo aporta el sistema que invoca el servicio, por ejemplo una colección de un cliente. `chunk_id` puede repetirse en documentos con claves o permisos diferentes. `access` ausente representa una política desconocida que debe resolver el sistema de origen antes de publicar. El indexador activa una versión según la política de vigencia del origen. Una revocación de acceso se aplica desde su entrada en vigor, incluso durante una reindexación. Si falta `document_key`, el segmentador carece de evidencia para relacionar dos versiones.

## 4. Estrategia de segmentación `estructura-1`

1. **Recorrer el contenido en orden.** Seguir el índice de `contenido/` y las relaciones entre unidades. Procesar cada unidad una vez. Leer `documento.md`, los índices y el informe para obtener metadatos. La política inicial limita el texto buscable a los conceptos de `contenido/`. El material exclusivo de `documento.md`, como los encabezados y pies repetidos, queda disponible en el bundle.
2. **Resolver las marcas antes de limpiar Markdown.** `okf-source-span` y `okf-generated` se aplican únicamente al siguiente bloque según el perfil. Dentro de código literal son texto. Se descarta el bloque editorial marcado, se conservan los pasajes originales que lo rodean y se separa `description` del texto buscable. Las imágenes de vista previa, los rótulos de navegación y las descripciones automáticas quedan como metadatos. Un texto alternativo sustantivo y un pie de figura sí pueden producir evidencia.
3. **Agrupar por estructura.** Los títulos abren contexto en `headings`. Dentro de una misma unidad, subsección y canal, añadir párrafos consecutivos en orden mientras quepan, separados por dos saltos LF. Listas, tablas y bloques de código forman fragmentos propios. En prosa se retiran los delimitadores de énfasis y se conserva el texto visible de los enlaces. En código se retiran las cercas y se conservan caracteres, sangría y líneas vacías. Los saltos de línea de `text` se representan con LF. Las filas XLSX siguen la regla específica de la sección 5. Un bloque sin marcador propio solo puede usar un localizador superior que efectivamente lo contenga. Si se usa, registrar `coarse_citation`. Si falta, registrar `uncitable_text`.
4. **Imponer un máximo de 512 tokens de `text`**, sin solapamiento entre fragmentos de prosa. La referencia de conteo es `cl100k_base` de `tiktoken 0.9.0` con codificación ordinaria, incluso para cadenas que coincidan con tokens especiales. Los campos de contexto viajan aparte. [Definición del tokenizador](https://github.com/openai/tiktoken/blob/0.9.0/tiktoken_ext/openai_public.py)
5. **Partir un bloque sobredimensionado.** Conservar el prefijo más largo que quepa, primero por filas en tablas, elementos en listas o líneas en código. En prosa, o si una unidad de esas sigue excediendo el máximo, cortar en el último espacio que quepa y, en su ausencia, entre caracteres Unicode. Continuar desde el corte. Conservar la cita del bloque al que pertenece cada parte. Las filas tabulares conservan sus encabezados como contexto, aun si una celda obliga a dividir la fila. Una caja de bloque sigue teniendo precisión de bloque después del corte.

## 5. Cómo se cita cada formato

| Formato | Formación del fragmento | Cita y casos especiales |
| --- | --- | --- |
| **PDF** | Seguir el orden editorial de la sección. Un párrafo que el productor unió a través de dos páginas se procesa como un bloque. Conservar la transcripción OCR identificada y su señal de incertidumbre. Una página exclusivamente visual se registra con `no_text` cuando carece de texto elegible. | Resolver el marcador en `source_spans` y en el archivo señalado por `block_spans`. Copiar `pdf_pages` con el intervalo de páginas físicas y **todas** sus cajas, en puntos y sin aplicar `/Rotate`. La etiqueta impresa opcional se puede consultar en `documento.md.page_labels`. |
| **DOCX** | Seguir la vista final del cuerpo. Separar párrafos, listas y tablas bajo sus encabezados. Emitir notas al pie y finales en `footnote`. Excluir los comentarios de revisión, el historial de cambios y el texto oculto de las secciones auxiliares del conversor. | Copiar `docx_blocks.from/to`, numerados desde 1. Una lista con un solo marcador conserva el intervalo completo. Una nota sin marcador puede citar el bloque que contiene su llamada si el número y el tipo de nota resuelven inequívocamente. En caso contrario se aplican las reglas de cita superior. |
| **PPTX** | Mantener los límites de la diapositiva. Agrupar contenido presentado en `main` y emitir las notas del presentador en `speaker_notes`. Los comentarios de revisión y las diapositivas que el productor señala como ocultas quedan registrados con `excluded_by_policy`. Una diapositiva con solo un título explícito citable puede producir un fragmento con ese título. | Citar `pptx_shape` en los bloques con marcador de forma, copiando `shape_id` y la geometría en EMU cuando exista. Para un título, una nota o un bloque sin forma identificada, usar `pptx_slide` con su `slide_index` y `slide_id` cuando exista. La forma del bloque anterior no se hereda. |
| **XLSX** | Procesar regiones de hojas visibles. En las pequeñas, leer la tabla o la lista con su marcador. En `Sheet Region`, leer el CSV completo y enriquecer por dirección con el JSONL de celdas cuando exista. Visitar el concepto hijo una sola vez. La muestra Markdown y el enlace de la hoja padre solo sirven para navegar. | Conservar el `xlsx_range` de la región. Derivar la dirección desde la esquina superior izquierda y la posición del registro CSV, respetando filas vacías y campos entrecomillados con saltos de línea. En una tabla Markdown se usa el número real de `Fila` y en una lista, la dirección explícita. Emitir `cell_range` solo con una correspondencia demostrable. |

**Filas de Excel.** La unidad inicial de búsqueda es una fila de datos de una región. Se toma la fila de encabezado que identifica el bundle. Las celdas con contenido se escriben en orden de columna, una por línea, como `dirección encabezado = valor`. Se usa la letra de columna cuando falta el encabezado. El intervalo de la cita cubre el valor y `cell_range` identifica la celda. La dirección y la etiqueta son contexto de lectura. Si solo se conoce la región, se omite la dirección y se entrega la cita con precisión `span`. Si una fila supera el máximo, se divide primero por celdas. Para una celda sobredimensionada se lleva el encabezado a `columns` y se divide el valor según la sección 4. Una fila vacía conserva su posición de origen sin producir un fragmento.

El valor mostrado procede del CSV o del bloque de la región. `cell_record` conserva, si existen, fórmula, valor almacenado, valor mostrado, formato y rango combinado. Una fórmula sin valor almacenado puede citarse como expresión literal cuando el bundle la identifica. El segmentador nunca la evalúa. Las estadísticas de la región, los gráficos sin una tabla identificable y los comentarios de revisión quedan fuera de la selección tabular. Las regiones y hojas ocultas por el estado `hidden` o `veryHidden` de su hoja se registran como excluidas.

En `ventas_trimestrales.xlsx`, la pregunta «¿Qué importe figura para el cacao en grano en la región Andina el 7 de junio de 2026?» se resuelve en la fila 250. Su fragmento incluye fecha, región, producto e importe. La línea `F250 Importe = 777.75` cita el valor en `Detalle!F250`, con `span.id = region`, `span.range = A1:G301` y `cell_range = F250`. La fórmula registrada es `=D250*E250`. Así se puede abrir la celda exacta incluso si la búsqueda se hizo por producto y fecha. [Concepto de la región](../../examples/xlsx/esperado/muestras/ventas_trimestrales/contenido/hoja-002-region-01.md) · [Registros por celda](../../examples/xlsx/esperado/muestras/ventas_trimestrales/assets/hoja-002-region-01-celdas.jsonl)

## 6. Alcance y aceptación del primer MVP

La primera entrega comprende una operación HTTP, un perfil de entrada, una estrategia fija y los cuatro formatos. Su resultado se puede inspeccionar y comprobar antes de elegir un motor de búsqueda. La generación de embeddings, la indexación, la recuperación y la selección de la versión vigente corresponden a etapas posteriores. El MVP usa los recursos y las transcripciones que ya produjo OKF.

| Prueba de aceptación | Resultado exigido |
| --- | --- |
| Contrato con el productor | Convertir los 32 documentos públicos de ejemplo y enviar los ZIP publicados. Las carpetas `esperado/` sirven de referencia normalizada. Para cada resultado, el número de líneas coincide con el total y la suma por unidad. El 100 % de las citas resuelve al original y a un intervalo UTF-8 válido del fragmento. |
| PDF | Al convertir `informe_tecnico` con la configuración de la referencia (OCR desactivado), el registro de ejemplo de la sección 2 reproduce el texto, la caja de `u-2` y su identificador. Una prueba con un bloque de dos páginas conserva ambas cajas tras el corte. Un escaneo exclusivamente visual con cero texto elegible produce cero fragmentos. |
| DOCX | En `guia_tecnica`, el pasaje sobre `F_1` resuelve a `b-7` en «Fundamentos». En `informe_revisado`, la nota al pie sale separada y la frase eliminada en las revisiones queda fuera del cuerpo buscable. |
| PPTX | En `rich`, la ecuación `E=mc^2+a/b` cita la forma 4 de la diapositiva 1. En `multiple`, las notas conservan el canal `speaker_notes` y la diapositiva oculta figura en el manifiesto con cero fragmentos. |
| XLSX | En `ventas_trimestrales` se representan las 300 filas de datos de «Detalle» una vez cada una. En la fila 250, el valor `777.75` resuelve a `F250` y conserva su fórmula. Las diez filas de la vista previa no generan duplicados. |
| Reprocesamiento y acceso | Dos peticiones simultáneas con el mismo ZIP dan los mismos identificadores y registros. Dos entradas con el mismo `conversion_id` y distintos permisos conservan ambas asociaciones. Dos salidas OCR diferentes que declaran `reproducible: false` tienen `result_id` diferentes. |
| Degradación y errores | Un `partial` válido, como `libro_con_perdidas`, conserva el informe y el estado en cada fragmento. Un marcador roto, una huella errónea o un CSV obligatorio ausente producen `422` sin artefacto. Las entradas válidas sin texto generan un manifiesto y cero líneas. |
| Límites y fidelidad | Ningún `text` supera 512 tokens. Al cortar prosa con acentos, una lista, código con sangría o una celda larga, se conserva todo su contenido sustantivo y se puede volver al bloque o celda de cada parte. |

El criterio de cierre es que un consumidor pueda leer el resultado con este contrato y llegar desde el texto recuperable hasta la evidencia en cada formato. La calidad del ranking se medirá cuando exista un índice que consuma estos fragmentos.
