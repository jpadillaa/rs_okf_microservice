# Plan 5. Implementación del MVP de segmentación para RAG

**Estado al 19 de noviembre de 2026.** Plan propuesto. Las siete etapas están pendientes.

**Objetivo.** Entregar un microservicio que reciba por HTTP un bundle OKF publicado y devuelva fragmentos cuya evidencia pueda localizarse en el original. El MVP implementará el contrato `1.0`, el perfil documental `1.24.0` y la estrategia `estructura-1` para PDF, DOCX, PPTX y XLSX. La entrega termina en el ZIP verificado que recibe el cliente de ingesta. La creación del índice y la evaluación de recuperación pertenecen a la siguiente fase del pipeline. [Especificación del MVP](../especificacion/SEGMENTACION_PARA_RAG.md) · [Arquitectura técnica](../arquitectura/ARQUITECTURA_TECNICA_SEGMENTACION_PARA_RAG.md)

## Secuencia de implementación

| Etapa | Entrega verificable | Requiere |
| --- | --- | --- |
| SEG1 | Validador compartido y corpus de entrada reproducible | Perfil y ejemplos del productor |
| SEG2 | Motor común, escritor y verificador del resultado | SEG1 |
| SEG3 | Primera petición HTTP completa con una cita PDF | SEG2 |
| SEG4 | Citas de bloques DOCX y formas PPTX | SEG3 |
| SEG5 | Citas de filas y celdas XLSX | SEG3 |
| SEG6 | Operación aislada, reintentos y recuperación de recursos | SEG3 |
| SEG7 | Aceptación de la imagen con los cuatro formatos y medición de capacidad | SEG4, SEG5 y SEG6 |

El primer hito observable es SEG3. El PDF `informe_tecnico` ya tiene un fragmento esperado con texto, caja, intervalo UTF-8 e identificador. Esto permite probar la ruta real de descarga, segmentación y entrega antes de incorporar las particularidades de los demás formatos. A partir de ese hito, SEG4, SEG5 y SEG6 pueden avanzar de forma independiente.

## SEG1. Extraer el validador y preparar las entradas

**Entrega.** Extraer del conversor una biblioteca Rust versionada que valide el contenedor, el original incluido, las identidades, las relaciones y los localizadores. Mantener el comando existente como consumidor de la biblioteca. Añadir en el segmentador la admisión explícita del perfil y la validación en ejecución del frontmatter y de los archivos PDF, PPTX y DOCX `block_spans` contra el esquema empacado. Preparar la generación de ZIP a partir de los originales públicos y registrar la revisión y configuración del productor. [Validador del productor](../../src/validate.rs) · [Esquema del perfil](../especificacion/perfil-1.24.0.schema.json)

**Criterios de aceptación**

- Comparar los resultados del comando antes y después de la extracción sobre los 32 ejemplos y los casos inválidos de sus pruebas actuales. Deben conservarse la aceptación, el rechazo y la compatibilidad previa del productor.
- Ante un ZIP ilegible, un límite excedido, un perfil o formato incompatible, una huella discordante o un localizador alterado, la biblioteca o la capa de admisión devuelve la causa tipada correspondiente. Un perfil anterior válido para el productor se identifica como `unsupported_profile` en el segmentador.
- El corpus de integración contiene ZIP completos publicados mediante `GET /jobs/{id}/bundle`. Las carpetas `esperado/` sirven para contrastar resultados, ya que normalizan campos y representan algunos binarios mediante huellas. [Pruebas de referencia](../../src/golden.rs)

## SEG2. Implementar el motor común y el contrato de salida

**Entrega.** Construir el inventario ordenado de unidades y el lector Markdown que asocia marcas al bloque antes de limpiarlo. Representar por separado el texto citable y los separadores de presentación. Implementar el corte, el cálculo de identidades y un solo escritor para `manifest.json`, `chunks.jsonl` y `conversion-report.md`. El verificador releerá la entrega antes de autorizar su devolución.

**Criterios de aceptación**

- Con casos de párrafos, listas, tablas y código, el conteo ordinario coincide con `cl100k_base` de `tiktoken 0.9.0`. Probar entradas de 511, 512 y 513 tokens, acentos, caracteres multibyte y tokens especiales literales. Los cortes conservan el contenido sustantivo, la sangría del código y los límites de unidad, subsección y canal. Cada `text` tiene entre 1 y 512 tokens.
- Una nota `okf-generated` se excluye junto con su bloque. El mismo marcador dentro de código se conserva literalmente. Cada intervalo `[start_byte, end_byte)` selecciona el pasaje esperado en el UTF-8 del texto definitivo, incluso después de un corte.
- Un lector de prueba independiente recalcula `bundle_sha256`, `result_id` y `chunk_id` conforme al contrato. Comprueba el orden y los ordinales desde 1 por unidad, la suma de recuentos, los tres archivos del ZIP y la igualdad byte a byte del informe.
- Un caso por aviso comprueba `excluded_by_policy`, `no_text`, `coarse_citation` y `uncitable_text`. Solo el último produce `segmentation_status: partial`. Un origen `partial` conserva su `source_status` aunque se segmente todo el texto elegible. Una entrada válida sin texto produce un manifiesto con sus unidades y un JSONL de cero bytes.

## SEG3. Completar el primer recorrido HTTP con PDF

**Entrega.** Conectar el servidor síncrono Rust/Axum/Tokio y el proceso hijo con el adaptador PDF. El padre reserva el cupo, recibe el ZIP por bloques y supervisa al hijo. El hijo resuelve los localizadores del concepto y sus archivos de cajas, segmenta y verifica. El padre inicia la descarga cuando la salida está cerrada y verificada.

**Criterios de aceptación**

- Convertir `informe_tecnico.pdf` con la configuración de referencia, incluido el OCR desactivado. Enviar su ZIP a `POST /v1/segmentations` y obtener `200 application/zip`. El registro de `u-2` coincide con el ejemplo completo del contrato, incluido el intervalo `[0, 204)`, la caja de la página 1 y el `chunk_id`. [Pasaje PDF](../../examples/pdf/esperado/muestras/informe_tecnico/contenido/seccion-001.md) · [Caja esperada](../../examples/pdf/esperado/muestras/informe_tecnico/contenido/seccion-001.spans.json)
- Un bloque que cruce dos páginas conserva ambas cajas al dividirse. Una transcripción identificada conserva la señal de OCR con su alcance original. Un escaneo solo visual con cero texto elegible devuelve cero fragmentos.
- La API responde `415 unsupported_media_type` para un tipo incorrecto, `400 invalid_archive` para un ZIP ilegible y `422` con `unsupported_profile`, `unsupported_format` o `invalid_bundle` según el caso. Los errores tienen `code` y `message` en JSON. Un fallo de verificación previo a la descarga deja la salida privada.

## SEG4. Incorporar DOCX y PPTX

**Entrega.** Conectar ambos adaptadores al motor y al escritor comunes. En DOCX, distinguir la vista final de las notas y del material de revisión. En PPTX, reconocer la visibilidad antes de descartar notas editoriales y separar contenido presentado y notas del orador.

**Criterios de aceptación**

- En `guia_tecnica`, el pasaje sobre `F_1` resuelve al bloque `b-7` de «Fundamentos». En `informe_revisado`, una nota con llamada inequívoca sale en `footnote` y el material eliminado, oculto o de revisión queda excluido del cuerpo buscable. Una lista con marcador compartido conserva su intervalo estructural. [Referencia DOCX](../../examples/docx/esperado/muestras/guia_tecnica/contenido/seccion-002.md)
- En `rich`, la ecuación `E=mc^2+a/b` cita la forma 4 de la diapositiva 1 y conserva su geometría disponible. En `multiple`, las notas salen en `speaker_notes` con cita a la diapositiva. La diapositiva oculta figura con `excluded_by_policy` y cero fragmentos. Un elemento sin marcador propio no hereda la forma anterior. [Referencia PPTX](../../examples/pptx/esperado/fixtures/rich/contenido/slide-001.md)

## SEG5. Incorporar XLSX y comprobar la cita de celda

**Entrega.** Leer las regiones pequeñas desde sus tablas o listas. Para una región grande, recorrer el CSV completo y asociar los registros JSONL por dirección de celda. Procesar cada región una vez y conservar la visibilidad heredada de su hoja. Cuando la asociación requiera almacenamiento auxiliar, limitarlo a la región y a la petición.

**Criterios de aceptación**

- En `ventas_trimestrales`, la hoja «Detalle» aporta sus 300 filas de datos una vez cada una. En la fila 250, los bytes `[140, 146)` seleccionan `777.75`. La cita devuelve `precision = cell`, `cell_range = F250`, el localizador `region` y la fórmula `=D250*E250` en `cell_record`. La muestra Markdown no duplica las primeras diez filas. [Región de referencia](../../examples/xlsx/esperado/muestras/ventas_trimestrales/contenido/hoja-002-region-01.md) · [Datos por celda](../../examples/xlsx/esperado/muestras/ventas_trimestrales/assets/hoja-002-region-01-celdas.jsonl)
- Verificar las direcciones con una región que empiece fuera de A1, una fila vacía, un campo CSV multilínea y registros JSONL en otro orden. Una celda larga se divide conservando su encabezado y su cita. Si solo se demuestra la región, la cita tiene precisión `span`.
- Una hoja oculta y sus regiones quedan inventariadas y excluidas. `libro_con_perdidas` conserva el informe y el estado parcial del origen. Una fórmula sin valor almacenado conserva la expresión identificable. Un ZIP al que le falte el CSV obligatorio recibe `422 invalid_bundle`.

## SEG6. Aislar las peticiones y demostrar la recuperación

**Entrega.** Completar el ciclo de vida del supervisor, con un cupo inicial por réplica, temporales privados, límites de tiempo y recursos, aislamiento Linux del hijo con Landlock y seccomp, apagado ordenado y limpieza al reiniciar. Instrumentar tiempos por etapa, bytes, cupos, formatos, avisos, fragmentos y máximos de memoria y disco.

**Criterios de aceptación**

- Probar los límites de bytes recibidos, expansión real y número de entradas. Los excesos dan `413 limit_exceeded`. Con un cupo ocupado, una petición adicional a la misma réplica obtiene `503 temporarily_unavailable`. El mismo código cubre el agotamiento temporal de disco o la interrupción transitoria antes de responder.
- Interrumpir una subida, terminar el hijo durante el procesamiento y cortar una descarga. Tras cada caso se recuperan cupo y espacio y un reintento con el ZIP íntegro puede terminar. Al reiniciar se eliminan los restos de la réplica. Durante el apagado cesan las admisiones y se respeta el plazo de las peticiones activas.
- En la imagen Linux, comprobar que el hijo trabaja con cuenta sin privilegios, sin sockets ni acceso a los temporales de otra petición. Usar valores de prueba reconocibles y verificar que el texto, `access`, `document_key` y los identificadores de contenido quedan fuera de registros y etiquetas de métricas.
- En dos réplicas con capacidad disponible, enviar el mismo ZIP simultáneamente y comparar el manifiesto lógico y las líneas ordenadas. Con archivos idénticos y permisos o claves de documento diferentes, cada entrega conserva su asociación. Comprobar la ausencia de campos opcionales y los bytes UTF-8 de `access` decodificado en el manifiesto y en cada fragmento.

## SEG7. Aceptar el MVP con el corpus completo

**Entrega.** Empaquetar el ejecutable, el esquema y el vocabulario local en la imagen. Ejecutar la integración completa por HTTP y conservar los resultados con las revisiones del productor y del segmentador, la configuración de conversión y el entorno de la prueba.

**Criterios de aceptación**

- Procesar los **32 ZIP publicados** correspondientes a 13 PDF, 9 DOCX, 7 PPTX y 3 XLSX. En cada entrega, verificar los criterios de SEG2 y resolver el **100 % de las citas** desde el intervalo UTF-8 del fragmento hasta el localizador del concepto y el original identificado por su huella. Contrastar además el contenido de los pasajes conocidos de SEG3 a SEG5.
- Reempaquetar un mismo contenido cambiando los bytes del ZIP. Cambian `bundle_sha256` y `result_id`, y los fragmentos con idéntica unidad, ordinal y texto conservan su `chunk_id`. Probar además dos bundles válidos con el mismo `conversion_id`, contenido OCR distinto y `reproducible: false`. Deben distinguirse por `result_id`.
- Medir por formato tiempo por etapa, tamaño de entrada y salida, memoria máxima del hijo y máximo de disco temporal con una petición y bajo carga concurrente. Registrar los límites de despliegue elegidos y el comportamiento observado al alcanzarlos.

**Cierre del plan.** El MVP se podrá declarar implementado cuando las siete etapas cuenten con evidencia reproducible sobre la revisión evaluada. Al cerrar cada entrega se actualizarán el corte, las tablas afectadas y el roadmap del [estado del proyecto](../estado/ESTADO_DEL_PROYECTO.md).
