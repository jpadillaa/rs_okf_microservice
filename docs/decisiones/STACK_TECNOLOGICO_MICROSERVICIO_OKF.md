# Stack tecnológico del microservicio OKF

**Estado:** decisión de implementación inicial; revisar únicamente las decisiones condicionadas a pruebas indicadas abajo.  
**Producto:** un archivo `.pdf`, `.docx`, `.xlsx` o `.pptx` produce un bundle OKF v0.2.  
**Contratos normativos:** [perfil documental OKF](https://chatgpt.com/library?file_id=libfile_c68ef3f3f8108191b414a8a632263394&account_id=personal) y [arquitectura técnica](https://chatgpt.com/library?file_id=libfile_791496d6b0e88191b9bf16e48563aa7a&account_id=personal). Este documento elige herramientas; no modifica esos contratos.

## 1. Decisión de despliegue

- Implementar el servicio y sus adaptadores en **Rust**, en un solo workspace con dos ejecutables: `api` y `worker`. Compartir tipos, lógica de conversión, persistencia y validador como módulos o crates internos. Iniciar con una instancia de cada ejecutable y un solo nodo.
- Usar **Tokio** para ejecución asíncrona y **Axum** para HTTP. La API recibe el archivo, lo guarda, registra el trabajo y responde con su identificador; la conversión ocurre en el worker. Las tareas de análisis y renderizado intensivas en CPU no deben ocupar hilos de atención de HTTP.
- Usar **SQLite con `rusqlite`** como registro durable y cola de trabajos. Guardar estados, intentos, asignación con vencimiento y rutas del original y del bundle. Las reclamaciones, renovaciones y finalizaciones usan transacciones; habilitar WAL y `busy_timeout`. API y worker acceden al archivo SQLite en el **mismo host y disco local**, nunca a través de un sistema de archivos de red. SQLite es una dependencia nativa deliberada; evita desplegar un servidor de base de datos y un broker adicionales. `redb` puede reevaluarse si cambia el diseño de procesos, pero su acceso de escritura entre procesos aún requiere una función experimental.
- Guardar los **bytes de documentos y bundles en un volumen local persistente**, no en la base de datos: `originals/`, `attempts/<job>/<attempt>/` y `published/<job>/`. Conservar el original inmutable. Empaquetar el bundle con `zip` para la descarga. Publicar por movimiento atómico dentro del mismo volumen y reconciliar en el arranque las interrupciones entre publicación y actualización de SQLite.
- Construir imágenes reproducibles para API y worker con el mismo `Cargo.lock`. La imagen de API permanece pequeña; la del worker incluye las herramientas de renderizado indicadas en §4. Montar el volumen persistente en ambos procesos. No agregar Redis, RabbitMQ, S3 ni servicios en la nube en la instalación inicial.

## 2. Dependencias elegidas

| Uso | Elección | Regla de incorporación |
| --- | --- | --- |
| HTTP y ejecución | [`axum`](https://docs.rs/axum/), [`tokio`](https://docs.rs/tokio/) | Un binario API y un worker; límites de concurrencia y de duración configurables. |
| Trabajo durable | [`rusqlite`](https://docs.rs/rusqlite/) + SQLite | SQL explícito y migraciones pequeñas versionadas en el repositorio; no añadir ORM ni broker. |
| Extracción general | [`xberg`](https://docs.xberg.io/features/) | Habilitar solo `pdf`, `office` y `excel`, junto con las funciones de runtime necesarias. No habilitar los bundles `full`/`server` ni OCR, embeddings, API, MCP o chunking de Xberg inicialmente. |
| Referencia exacta de XLSX | [`calamine`](https://docs.rs/calamine/) si hace falta | Incorporarlo únicamente si el resultado estructurado de Xberg no permite recuperar **hoja y rango A1 absolutos**. En ese caso, Calamine pasa a ser el lector autorizado de datos de XLSX y se desactiva la extracción Excel de Xberg; evitar dos conversiones completas del mismo libro. |
| Metadatos OOXML | [`zip`](https://docs.rs/zip/) y [`quick-xml`](https://docs.rs/quick-xml/) | Leer partes concretas del paquete solo para localizadores faltantes: ordinal de bloque DOCX, `slide_id`, relaciones y recursos PPTX, o propiedades de hojas. No implementar otro conversor OOXML completo. |
| Bundle y validación | [`serde`](https://docs.rs/serde/), [`serde_json`](https://docs.rs/serde_json/), [`yaml_serde`](https://docs.rs/yaml_serde/), [`sha2`](https://docs.rs/sha2/), [`zip`](https://docs.rs/zip/) | Tipos internos; frontmatter YAML serializado y vuelto a analizar por el validador; informe; SHA-256 del original; ZIP de entrega. Evitar el `serde_yaml` archivado. |
| Diagnóstico | [`tracing`](https://docs.rs/tracing/) y [`tracing-subscriber`](https://docs.rs/tracing-subscriber/) | Registro estructurado por `job_id`, intento, formato, etapa y duración; sin registrar contenido del documento. |

**AnyDoc** se reserva como comparador de calidad de texto/Markdown durante las pruebas. Su interfaz PDF no entrega páginas estructuradas; además, hay huecos documentados en coordenadas de XLSX y límites de PPTX. No añadirlo a producción junto con Xberg sin una mejora demostrada en un caso concreto. `pdf_oxide` y `office_oxide` son alternativas por formato si Xberg no alcanza la calidad o el presupuesto de dependencias medidos, no dependencias iniciales.

## 3. Extracción y procedencia por formato

Los adaptadores traducen la salida de bibliotecas a un **modelo intermedio propio** de bloques, unidades, recursos, localizadores e incidencias. Ni el esquema de Xberg ni su Markdown son el contrato del bundle. Conservar localizadores antes de renderizar texto y dividir los documentos largos por estructura semántica durante la reconciliación.

| Entrada | Ruta inicial | Comprobación obligatoria |
| --- | --- | --- |
| PDF | Xberg por página, con tablas, imágenes y estructura cuando esté disponible. Su renderizador PDF puede producir vistas de páginas visuales. | `pdf_pages` físico desde 1; bloque citable en concepto multipágina con `source_span`; distinguir etiqueta visible impresa de la etiqueta PDF interna. Nunca inferir una etiqueta ilegible. Documentar páginas sin texto y OCR pendiente. |
| DOCX | Xberg estructurado; completar mediante OOXML si hace falta. | Ordinal estable de cada bloque desde 1 y jerarquía de encabezados; no usar la paginación renderizada como localizador canónico. Registrar revisiones, comentarios y contenido auxiliar que no pueda conservarse fielmente. |
| XLSX | Probar Xberg estructurado con hojas; si faltan coordenadas absolutas, usar Calamine para el adaptador XLSX. | Nombre exacto de hoja y rango A1 de cada región; incluir hojas ocultas. Separar fórmula y valor almacenado, sin afirmar recálculo. Datos grandes completos como recurso, nunca una muestra presentada como totalidad. |
| PPTX | Xberg por diapositiva para texto, imágenes, tablas y notas; lectura puntual de OOXML para el identificador estable. | `slide_index` desde 1, incluidas las ocultas; `slide_id` cuando exista; número visible solo si se detecta. Notas aparte y **una vista visual por diapositiva** según §4. |

La estructura de nodos de Xberg tiene página y geometría cuando están disponibles, pero actualmente no garantiza una correspondencia exacta entre cada nodo y los offsets de su texto renderizado. Crear los `source_spans` desde unidades/bloques del modelo intermedio; no intentar inferirlos después a partir de un Markdown concatenado.

## 4. Renderizado y OCR: excepciones explícitas

**PPTX:** el perfil OKF exige una vista de cada diapositiva. Para la instalación inicial usar **LibreOffice Impress en modo headless**, invocado solo por el worker y solo para producir un PDF temporal de la presentación; convertir sus páginas a PNG con el renderizador PDF de Xberg. Activar `ExportHiddenSlides` y desactivar páginas de notas en esa exportación. Verificar que la cantidad y el orden de páginas renderizadas corresponden a todas las diapositivas de origen. Ejecutar el proceso con perfil temporal por intento, límite de tiempo, de memoria y de salida; no ejecutar macros ni archivos incrustados. El PDF temporal no es la fuente del texto ni de la procedencia y se elimina después del intento. Esta es la **única herramienta de renderizado externa** del stack inicial y aumenta el tamaño de la imagen del worker. Un fallo que impida adjuntar una vista requerida impide publicar ese bundle como válido.

No sustituirla automáticamente por un renderizador PPTX nuevo de Rust: la implementación raster de BetterOffice documenta pérdidas en tablas, recortes de imágenes y efectos. Si se sustituye LibreOffice, comparar antes las vistas de un corpus real y conservar la misma regla de validación.

**PDF escaneado:** comenzar sin OCR. Cuando una página carece de texto, adjuntar su vista, localizarla y registrar `partial` con el motivo preciso. Una futura incorporación de OCR debe ser local y activable por configuración, con presupuesto independiente de CPU/memoria; no incluir modelos ni servicios alojados en el despliegue inicial. El OCR nunca reemplaza el original ni oculta su incertidumbre.

## 5. Reglas para el agente de código

1. Implementar los contratos del perfil OKF y los estados de la arquitectura técnica. La API no espera la conversión. Cada intento usa una carpeta propia; un trabajo aceptado conserva original y registro tras reinicios.
2. Validar **conformidad OKF** y **cobertura editorial** por separado antes de publicar. `complete` y `partial` exigen un bundle válido; `failed` no ofrece descarga de bundle. Incluir `conversion-report.md` y el SHA-256 del original. Un PDF visual sin OCR puede ser `partial`; una vista PPTX obligatoria ausente bloquea publicación.
3. Limitar por configuración bytes de entrada, expansión ZIP/OOXML, páginas, diapositivas, celdas, tamaño de imágenes, uso de memoria, tiempo y trabajos concurrentes. Procesar documentos largos por unidades para acotar memoria, pero ensamblar los conceptos según la estructura editorial original.
4. Fijar y registrar versiones en `Cargo.lock` y en la imagen del worker; no usar etiquetas `latest` en despliegue. Antes de cerrar el stack, inspeccionar `cargo tree`, tamaño de binarios, tiempo de compilación y licencias de las dependencias transitivas realmente activadas.
5. Probar el motor con un corpus pequeño pero representativo: PDF digital, escaneado y mixto, PDF largo multipágina, DOCX breve y largo con tablas/notas/revisiones, XLSX con varias hojas y rangos desplazados, PPTX con diapositivas ocultas, sin título, con tablas e imágenes. Medir duración, CPU y memoria máxima por archivo; verificar localizadores, orden, recursos y cobertura contra el original. Registrar resultados antes de sustituir Xberg o agregar otro extractor.

## 6. Referencias de implementación

- [Contrato OKF documental del proyecto](https://chatgpt.com/library?file_id=libfile_c68ef3f3f8108191b414a8a632263394&account_id=personal) y [arquitectura técnica](https://chatgpt.com/library?file_id=libfile_791496d6b0e88191b9bf16e48563aa7a&account_id=personal).
- [Xberg: funciones por formato](https://docs.xberg.io/features/), [estructura y resultados](https://docs.rs/xberg/latest/xberg/types/extraction/struct.ExtractedDocument.html), [renderizado de PDF](https://docs.xberg.io/guides/rust-core-api/).
- [Calamine: rangos absolutos](https://docs.rs/calamine/latest/calamine/struct.Range.html); [AnyDoc: limitación PDF por páginas](https://github.com/firecrawl/anydoc/issues/62), [XLSX](https://github.com/firecrawl/anydoc/issues/10) y [PPTX](https://github.com/firecrawl/anydoc/issues/31).
- [SQLite: WAL y procesos en un host](https://www.sqlite.org/wal.html); [redb: escritura multiproceso experimental](https://docs.rs/redb/latest/redb/).
- [LibreOffice: conversión headless](https://help.libreoffice.org/latest/en-US/text/shared/guide/start_parameters.html) y [ExportHiddenSlides](https://help.libreoffice.org/latest/en-US/text/shared/guide/pdf_params.html); [BetterOffice PPTX raster: límites de fidelidad](https://docs.rs/crate/betteroffice-pptx-raster/latest).
