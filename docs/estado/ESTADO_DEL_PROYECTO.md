# Estado del proyecto

**Corte:** 2 de noviembre de 2026, plan 2 C6 implementado tras C5 (`e94af0d`); C5 pendiente de siete noches de estabilidad; commits locales, sin push.
**Alcance:** qué está implementado, con qué evidencia de calidad, cómo se compara con el plan inicial y qué queda pendiente. Los contratos están en la [especificación](../especificacion/ESPECIFICACION_OKF.md), las [arquitecturas](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md) y el [stack](../decisiones/STACK_TECNOLOGICO_MICROSERVICIO_OKF.md); este documento no los modifica y debe actualizarse al cerrar cada entrega.

## 1. Resumen

Las cuatro entregas previstas están implementadas: el servicio convierte PPTX, DOCX, XLSX y PDF en bundles OKF v0.2 con la misma API, el mismo worker, el mismo ensamblador y el mismo validador. Cada formato tiene pruebas de aceptación con documentos sintéticos y, salvo XLSX, con copias reducidas de documentos reales, y los 26 documentos de ejemplo tienen un bundle de referencia que las pruebas comparan archivo por archivo. La suite (64 pruebas en macOS; C4 se verificó también con Landlock activo en Docker/Linux), `clippy` sin advertencias, `rustfmt` y la auditoría de dependencias pasan en local.

El [plan 1](../planes/PLAN_1_RECUPERACION_DEL_PLAN_ORIGINAL.md) recuperó del plan inicial lo que seguía aportando (§5.6): un ensamblador único, la cobertura frente a una referencia independiente, límites configurables y control de admisión, el registro por etapa, una imagen reproducible y la auditoría de dependencias; y cerró en la documentación las decisiones abandonadas. Lo que falta es **evidencia**: evaluar DOCX con archivos guardados por Word y conseguir los documentos reales que pide el corpus (XLSX, PDF escaneado, mixto y largo); y las garantías del [plan 2](../planes/PLAN_2_CALIDAD_Y_ROBUSTEZ.md): retención, aislamiento, fuzzing y salud.

Frente al plan inicial (§5), el alcance funcional está cumplido. Los extractores son propios en lugar de Xberg, cuyo abandono registra el stack con la cobertura independiente como evidencia, y un modelo de salida común sustituye al modelo intermedio previo al ensamblado.

## 2. Qué está implementado

### 2.1 Servicio

| Pieza | Estado | Detalle |
| --- | --- | --- |
| API HTTP (Axum) | Hecho | `POST /jobs` recibe el archivo y devuelve el identificador; `GET /jobs/{id}` da estado, intento, error y versiones del conversor/perfil; `GET /jobs/{id}/bundle` entrega el ZIP solo si el trabajo terminó `complete` o `partial`. Escucha en `127.0.0.1:8080` por defecto. |
| Registro y cola durable | Hecho | SQLite (WAL, `busy_timeout`) en `OKF_DATA_DIR`; originales, intentos y publicados en `originals/`, `attempts/<job>/<intento>/` y `published/<job>/`. |
| Worker | Hecho | Reclamación exclusiva con asignación renovable, conversión en un proceso hijo vigilado (memoria y tiempo), reintentos con espera creciente para fallos transitorios, fallo inmediato para los permanentes, incluidos los pánicos del conversor con mensaje y ubicación; apagado por señales con gracia y devolución sin consumir intento. |
| Publicación y recuperación | Hecho | Publicación atómica por renombrado; al arrancar, reconciliación de publicaciones interrumpidas, bundles perdidos e intentos huérfanos. |
| Validador | Hecho | Conformidad OKF e integridad editorial por separado, con reglas comunes y por formato ([arquitectura técnica, §5](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#5-implementación-en-las-entregas-pptx-docx-xlsx-y-pdf)). Un bundle que no lo supera no se publica. `validate <ZIP o directorio>` lo ejecuta de forma independiente, con el original incluido. |
| Límites y admisión | Hecho | Rlimits del hijo (memoria virtual en Linux, CPU, archivo y descriptores), Landlock en Linux y vigilancia de memoria; diapositivas, bloques, celdas, páginas, tiempo, asignación, intentos, tamaño de subida, expansión ZIP y píxeles de imagen por variables `OKF_*`; la API responde `429` con la cola llena y `507` sin espacio libre, si se configuran. |
| Registro de diagnóstico | Hecho | `tracing` con una línea por intento: trabajo, intento, formato, resultado, duración de lectura, ensamblado y validación, duración total y memoria máxima; sin contenido del documento. |
| Imagen | Hecho | Una imagen para API y worker (`Dockerfile`), con las imágenes base fijadas por digest y la fuente del renderizado; `compose.yaml` levanta los dos procesos, el worker sin red. |

API y worker son subcomandos de un único ejecutable (`okf-microservice api` y `okf-microservice worker`) y se ejecutan como dos procesos, que es lo que exige la arquitectura; el stack registra la decisión.

### 2.2 Adaptadores

| Formato | Entrega | Lectura | Qué produce |
| --- | --- | --- | --- |
| **PPTX** | 1.ª (29–30 oct.) | Lector OOXML propio; vistas con `betteroffice-pptx` (raster) | Un concepto por diapositiva, incluidas las ocultas, con `slide_index`, `slide_id`, notas, tablas, imágenes, gráficos por su caché, ecuaciones, comentarios, texto alternativo, títulos inferidos y una vista PNG de cada diapositiva. |
| **DOCX** | 2.ª (31 oct.) | Lector OOXML propio, en streaming | Bloques numerados, división por encabezados (con estilo o inferidos por formato directo), listas, tablas, imágenes, notas, comentarios, revisiones pendientes, texto oculto, cuadros de texto y ecuaciones. |
| **XLSX** | 3.ª (31 oct.) | Lector OOXML propio, hojas en streaming | Un concepto por hoja (ocultas y de gráfico incluidas), regiones con `xlsx_range`, CSV completo y registros por celda para las regiones grandes, fórmulas separadas del valor almacenado, gráficos, tablas dinámicas, comentarios y vínculos externos declarados. |
| **PDF** | 4.ª (1 nov.) | `pdf_oxide` y maquetación propia | Orden de lectura por columnas y bandas, párrafos continuados entre columnas y páginas, tablas sencillas, encabezados por marcadores o por tamaño, encabezados y pies registrados una vez, `page_labels`, clasificación de cada página y vista de las que no pueden transcribirse. |

Las decisiones de biblioteca de cada formato y sus mediciones están en el [stack](../decisiones/STACK_TECNOLOGICO_MICROSERVICIO_OKF.md) y en la [decisión de renderizado PPTX](../decisiones/RENDERIZADO_PPTX_MEDICIONES.md).

## 3. Calidad

### 3.1 Verificación automatizada

| Grupo | Pruebas | Qué cubren |
| --- | ---: | --- |
| Servicio (`src/lib.rs`) | 10 | Ruta completa por formato a través del worker, estados `complete`/`partial`/`failed`, clasificación de fallos y reintentos, asignaciones, reconciliación al arrancar, rechazo de bundles alterados por el validador y admisión (subida, cola y disco). |
| Bundles de referencia (`src/golden.rs`) | 2 | Los 26 documentos de ejemplo, comparados archivo por archivo con `examples/<formato>/esperado/`, y determinismo de dos conversiones. |
| Cobertura independiente (`src/coverage.rs`) | 2 | Los 26 ejemplos, con listas TSV y mínimos versionados independientes de los golden; normalización Unicode y pérdida deliberada de texto. |
| Apagado (`tests/shutdown.rs`) | 1 | SIGTERM con devolución a la cola sin consumir intento y SIGINT con publicación dentro de la gracia, durante una lectura detenida. |
| Validador independiente (`src/validate.rs`, CLI) | 3 | Los 26 ejemplos como ZIP/directorio, rechazo de alteraciones y enlaces simbólicos, y códigos de salida del comando. |
| Versionado (`src/version.rs`, consulta y recuperación) | 2 | Identidad persistida y recuperada del bundle; rechazo de versiones discordantes o perfiles desconocidos. |
| Ensamblador y paquetes (`src/output.rs`, `src/ooxml.rs`) | 2 | Frontmatter legible por un analizador YAML con cadenas difíciles; límites de expansión ZIP configurables. |
| Proceso hijo (`tests/worker_process.rs`, `src/worker.rs`, `src/isolation.rs`) | 3 en macOS, 5 en Linux | Conversión real en proceso hijo, parada al superar los límites, registro del intento y pánico permanente en el primer intento; código desconocido reintentable. En Linux, pruebas de límites duros de memoria/archivo y acceso a rutas con Landlock. |
| PPTX | 9 | Extracción con muestras y con el corpus reducido. |
| DOCX | 9 | Corpus real reducido, muestras artificiales y fixtures mínimos. |
| XLSX | 8 | Muestras sintéticas, referencias A1 y lectura de celdas. |
| PDF | 13 | Corpus real reducido, muestras sintéticas, maquetación, lectura y límite de píxeles de imagen. |

La [CI](../../.github/workflows/ci.yml) ejecuta `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` y `cargo test --locked` en Ubuntu 24.04 con cada push a `main`; comprueba las licencias y los orígenes de las dependencias con `cargo deny`, y construye la imagen y ejecuta la prueba de humo `tools/smoke.sh`. Un [flujo diario](../../.github/workflows/scheduled.yml) comprueba los avisos de seguridad. Las pruebas, la auditoría, la construcción de la imagen y la prueba de humo pasan en local en el commit de corte; la CI de `1fd4347` pasó en Linux. Revisadas las dos siguientes: `cf0f869` pasó completa (`test`: 3 min 02 s) y `7abd68c` tiene `test` y dependencias correctos (`test`: 2 min 50 s), con imagen aún en curso al consultar. El aviso de Node.js 20 desapareció en ambas. El flujo programado de `1fd4347` falló por `RUSTSEC-2026-0429` (`ZipArchive::extract`); se verificó que ni el servicio ni BetterOffice usan esa función y se añadió la excepción razonada a `deny.toml`. La auditoría local actualizada pasa.

El [fuzzing nocturno](../decisiones/FUZZING.md) tiene cuatro objetivos, semillas de los 26 ejemplos y artefactos de fallos; la aceptación de siete noches consecutivas sigue pendiente.

Las pruebas de PDF y del validador alteran bundles válidos (marcador sin bloque, localizador fuera de su sección, tramos desordenados, índices en otro orden) y comprueban que se rechazan.

### 3.2 Evidencia por formato

| Formato | Corpus real | Muestras sintéticas | Resultado y rendimiento (`--release`, validación incluida) |
| --- | --- | --- | --- |
| **PPTX** | 3 presentaciones del curso (`s1`–`s3`), reducidas en `examples/pptx/corpus/` | 4 fixtures | Se convierten las tres; `s3` declara `view_hides_text` en sus cinco diapositivas animadas. Renderizado: 0,85–1,24 s y 288–368 MB de pico. |
| **DOCX** | 3 documentos (`d1`–`d3`), reducidos en `examples/docx/corpus/` | 4 muestras y 2 fixtures | Los tres reales, `complete`; menos de 0,2 s y 25 MB. Se probaron también documentos de otros productores (`textutil`, `python-docx`). |
| **XLSX** | Ninguno | 3 libros | Las muestras dan el estado esperado (dos `complete`, una `partial`). Libro de 800 000 celdas: 3,6 s y 242 MB. |
| **PDF** | 4 PDF; 3 reducidos en `examples/pdf/corpus/` y el recibo solo en local | 4 muestras | Los cuatro reales, `complete`. Presentación de 68 MB: 5,5 s y 306 MB; artículo: 0,16 s; exportación de Word: 0,04 s; recibo: < 0,01 s. Frente a pdfium se conservan el 100 % de las palabras de la exportación, todos los caracteres de la presentación y el 98,8–99,3 % del artículo y del recibo (lo que falta es el encabezado repetido, registrado una vez). |

Las copias reducidas conservan el texto y la estructura y sustituyen imágenes y fuentes; sirven para probar la extracción, no el renderizado. Los originales están en `datos_de_prueba/`, que no se versiona.

La [cobertura frente a una referencia independiente](../decisiones/COBERTURA_INDEPENDIENTE.md) (pdfium, python-docx, python-pptx y openpyxl) explica cada palabra que falta. Con el corpus real, DOCX conserva el 100 % de las palabras; PPTX, también, salvo en `s1`, donde faltan nombres de archivo usados como texto alternativo y el total del número visible; y PDF, entre el 98,48 % y el 100 % del texto, donde faltan encabezados registrados una vez y fragmentos de palabras partidas.

### 3.3 Límites conocidos

- **PPTX:** la vista no dibuja los bordes de tabla; las fuentes no instaladas se sustituyen con otras métricas; las diapositivas animadas muestran todas las formas a la vez. No se ha medido la fidelidad con tablas nativas, gráficos ni SmartArt reales. El pico de memoria (hasta 368 MB) queda cerca del límite de 500 MB.
- **DOCX:** solo tres documentos reales, ninguno guardado expresamente por Word para la evaluación; es la comprobación pendiente de mayor valor. El texto alternativo de un gráfico sin datos se pierde.
- **XLSX:** toda la evidencia es sintética. Por diseño, las fórmulas no se evalúan y los formatos con unidad o texto no se aplican (se declaran).
- **PDF:** sin OCR (una página escaneada queda como vista y el bundle es `partial`); solo tablas sencillas, sin detección de reglas ni celdas combinadas; no se usa la estructura etiquetada del PDF; en el recibo, algunas parejas etiqueta–valor del encabezado salen como líneas sueltas; las páginas con texto, como diapositivas exportadas, no llevan vista. La unión de palabras partidas al final de línea suprime guiones legítimos en URL y compuestos (`langchain-ai`, `if-then-else`); corregirlo exige cambiar la regla de la especificación.
- **División:** DOCX y PDF se dividen en un solo nivel (`seccion-NNN.md`), como describe ahora la especificación. La división en partes y capítulos que pedía el plan inicial se reabrirá si un documento largo de texto muestra secciones demasiado largas o un índice plano poco navegable.

## 4. Brechas frente a la arquitectura y el stack

| Requisito | Fuente | Situación |
| --- | --- | --- |
| Límite de tamaño de entrada configurable | Arquitectura técnica, §4 | Hecho: `OKF_MAX_UPLOAD_MB`. |
| Control de carga en la admisión | Arquitectura técnica, §4 | Hecho: `OKF_MAX_QUEUED` (`429`) y `OKF_MIN_FREE_MB` (`507`); sin configurar no limitan. |
| Política de retención de originales, intentos y bundles | Arquitectura técnica, §2 | No definida; los publicados y los originales se conservan indefinidamente. Pendiente en el [plan 2, C8](../planes/PLAN_2_CALIDAD_Y_ROBUSTEZ.md#c8-retención-s). |
| Imágenes reproducibles de API y worker con versiones fijadas | Stack, §1 y regla 4 | Hecho: una imagen para los dos procesos; el stack registra la decisión y las versiones. |
| Dos ejecutables | Stack, §1 | Cerrado: el stack registra un ejecutable con subcomandos. |
| Autenticación | No exigida | La API no autentica; solo es adecuada para uso local o detrás de un proxy que lo haga. |

## 5. Estado frente al plan inicial

El plan inicial es la documentación del commit `802b9b1` (28 de octubre), anterior a cualquier código: especificación, arquitecturas y stack. Lo que el plan pedía construir está hecho; cómo se construyó cambió en el stack y en parte de la arquitectura.

### 5.1 Alcance y calendario

| Plan inicial | Estado actual |
| --- | --- |
| Convertir PDF, DOCX, XLSX y PPTX a OKF v0.2. | Cumplido: los cuatro adaptadores funcionan sobre la misma API, el mismo worker y el mismo validador. |
| Validar conformidad OKF e integridad editorial por separado; estados `complete`, `partial` y `failed`. | Cumplido. Ningún bundle inválido se publica y las pruebas comprueban que se rechazan bundles alterados. |
| API asíncrona, cola SQLite durable, asignaciones con vencimiento, reintentos, publicación atómica y reconciliación al arrancar. | Cumplido. |
| Sin orden de entrega; los cuatro formatos sobre Xberg. | Cambiado el 30 de octubre (commit `203f76b`): primero PPTX de extremo a extremo y después un formato cada vez. |

Las cuatro entregas ocuparon del 29 de octubre al 1 de noviembre, con unas 16 800 líneas de Rust propio.

### 5.2 Stack

| Decisión inicial | Qué pasó | ¿Se siguió la regla del plan? |
| --- | --- | --- |
| Xberg como extractor general de los cuatro formatos. | No se usa en ningún formato. | Solo en PDF. El plan pedía medir y registrar resultados antes de sustituir Xberg: en PDF se midió y se descartó con datos; en PPTX, DOCX y XLSX no se midió, como reconoce el [stack](../decisiones/STACK_TECNOLOGICO_MICROSERVICIO_OKF.md). El plan 1 registró su abandono en el stack, con la [cobertura independiente](../decisiones/COBERTURA_INDEPENDIENTE.md) de los lectores propios como evidencia. |
| No implementar otro conversor OOXML completo; `zip` y `quick-xml` solo para los localizadores que faltaran. | Se escribió un lector OOXML propio de unas 9 000 líneas para PPTX, DOCX y XLSX. | No. La decisión está justificada en el stack (geometría, notas, comentarios y localizadores directamente del XML), pero contradice una regla explícita del plan. |
| LibreOffice headless para las vistas PPTX, sin cambiar a un renderizador Rust sin comparar antes. | Se cambió a BetterOffice raster. | Sí: se compararon las dos rutas y se aceptaron pérdidas acotadas, como los bordes de tabla ([decisión de renderizado](../decisiones/RENDERIZADO_PPTX_MEDICIONES.md)). |
| Calamine solo si Xberg no daba hoja y rango A1. | No se usa: el lector propio los da. | Sí, coherente con la condición. |
| `pdf_oxide` como alternativa si Xberg no alcanzaba. | Se adoptó para PDF, con maquetación propia. | Sí. |

El proyecto pasó de integrar una biblioteca general a escribir sus propios extractores. Eso dio control sobre los localizadores y la calidad, a cambio de mucho más código propio que mantener y de una decisión tomada sin la medición exigida en tres formatos.

### 5.3 Arquitectura

| Plan inicial | Estado actual |
| --- | --- |
| Modelo intermedio común: todos los adaptadores entregan los mismos tipos (unidades, bloques, recursos, localizadores, incidencias) y un ensamblador genera el bundle ([arquitectura conceptual, §3](../arquitectura/ARQUITECTURA_CONCEPTUAL_MICROSERVICIO_OKF.md)). | Sustituido por el plan 1: cada adaptador conserva su modelo de lectura y entrega conceptos, recursos e incidencias en un modelo de salida común, que escribe un único ensamblador (`src/output.rs`); DOCX y PDF comparten las secciones (`src/sections.rs`). La arquitectura conceptual y el stack lo describen. |
| Estados `queued → processing → validating → …`. | `validating` no se registra; la validación ocurre dentro de `processing`. Las dos arquitecturas lo describen así. |
| Dos ejecutables, `api` y `worker`. | Un ejecutable con dos subcomandos que corren como dos procesos; decisión registrada en el stack. |
| Límites configurables de entrada, trabajos simultáneos, recursos y tiempo; control de carga en la admisión. | Hecho (plan 1, R4): todos los límites son configurables, también la subida, la expansión ZIP y los píxeles de imagen; la API controla la cola y el espacio libre. La concurrencia se controla con el número de workers. |
| Política de retención. | No definida; pendiente en el plan 2 (C8). |
| Imágenes reproducibles de API y worker. | Una imagen para los dos procesos, con versiones fijadas (plan 1, R3). |
| Registro por trabajo, intento, formato, etapa y duración. | Hecho (plan 1, R5): una línea por intento con formato, resultado, etapas, duración total y memoria máxima. |
| Conversión fuera de los hilos HTTP. | Superado: cada conversión corre en un proceso hijo con límite de memoria medido, necesario porque la memoria de PPTX y PDF depende de las imágenes y no del número de unidades. |

### 5.4 Corpus de prueba

| El plan pedía | Cubierto |
| --- | --- |
| PDF digital, escaneado, mixto y largo multipágina. | Digital, con documentos reales. Escaneado y mixto, solo sintéticos. Largo, la presentación de 53 páginas; ningún PDF largo de texto (libro, tesis). |
| DOCX breve y largo, con tablas, notas y revisiones. | Sí: 3 reales y 6 sintéticos; revisiones y notas solo en los sintéticos. |
| XLSX con varias hojas y rangos desplazados. | Solo sintéticos. |
| PPTX con diapositivas ocultas, sin título, con tablas e imágenes. | Sí; tablas nativas y gráficos reales sin medir en el renderizado. |
| Duración, CPU y memoria máxima por archivo. | Duración y memoria; CPU solo en PPTX. |

### 5.5 Documentación

La especificación pasó de 112 a 172 líneas. Las reglas detalladas de cada formato se escribieron junto con su adaptador: el comportamiento real queda documentado, pero en esas reglas la especificación siguió al código en lugar de guiarlo. La incoherencia sobre la división en partes y capítulos (§3.3) es una consecuencia.

### 5.6 Recuperación del plan inicial

El [plan 1](../planes/PLAN_1_RECUPERACION_DEL_PLAN_ORIGINAL.md) se implementó el 1 de noviembre, salvo el corpus representativo (R6), que avanza según lleguen documentos:

| Trabajo | Resultado |
| --- | --- |
| R1. Modelo de salida común y ensamblador único | `src/output.rs` y `src/sections.rs`. Los bundles de referencia de los 26 ejemplos (plan 2, C1, hecho antes como requisito) comprobaron la migración: solo cambiaron las comillas del frontmatter, que ahora encierran toda cadena para que un analizador YAML 1.1 no lea fechas o resúmenes como otro tipo. |
| R2. Referencia de cobertura independiente | `tools/coverage.py` y la [medición](../decisiones/COBERTURA_INDEPENDIENTE.md); encontró y corrigió la pérdida del texto alternativo de una imagen DOCX ausente, y registró dos hallazgos pendientes (§3.3). |
| R3. Imagen reproducible | `Dockerfile`, `compose.yaml` y prueba de humo en la CI. |
| R4. Límites configurables y control de admisión | Variables `OKF_MAX_UPLOAD_MB`, `OKF_MAX_ZIP_*`, `OKF_MAX_IMAGE_PIXELS`, `OKF_MAX_QUEUED` y `OKF_MIN_FREE_MB`. |
| R5. Registro por formato, etapa y duración | Una línea por intento; el proceso hijo devuelve sus etapas y su memoria máxima (Linux y macOS; comprobación final añadida en C5). |
| R6. Corpus representativo | Pendiente: requiere documentos del equipo o fuentes públicas aprobadas. |
| R7. Cierres documentales | Stack, arquitecturas y especificación describen lo que hace el código. |
| R8. División en partes y capítulos | La especificación describe la división en un nivel (§3.3). |
| R9. Auditoría de dependencias | `cargo deny` en la CI; la primera comprobación llevó a actualizar `quick-xml` por dos avisos de denegación de servicio. |

## 6. Roadmap pendiente

El plan 1 cerró los antiguos puntos sobre el modelo intermedio, Xberg, el cierre operativo (salvo la retención) y la división (§5.6). El corpus pendiente es el R6 de ese [plan](../planes/PLAN_1_RECUPERACION_DEL_PLAN_ORIGINAL.md#r6-corpus-representativo-m-continuo); las garantías de regresión, aislamiento, operación y versionado están en el [plan 2](../planes/PLAN_2_CALIDAD_Y_ROBUSTEZ.md), del que C1 a C4, C6, C9 y C10 ya están hechos; C5 está implementado y espera siete noches de evidencia.

En orden de prioridad:

1. **Seguimiento de CI**: confirmar la imagen de `7abd68c` y la próxima ejecución del flujo programado con la excepción razonada de `RUSTSEC-2026-0429`. Los golden pasan en Linux; el aviso de Node.js 20 ya desapareció y `test` tarda unos tres minutos.
2. **Evaluación DOCX con archivos guardados por Word**: las muestras reabiertas y guardadas en Word, o nuevos DOCX reales. Está en pausa hasta disponer de esos archivos.
3. **Corpus real pendiente** (plan 1, R6): libros XLSX, un PDF escaneado, uno mixto y uno largo de texto, y un DOCX real con notas y revisiones, cada uno con su bundle de referencia y su medición de cobertura.
4. **Plan 2**: observar siete noches del fuzzing C5 después del push; continuar con salud y métricas (C7) y retención (C8).
5. **Guiones legítimos al final de línea en PDF**: cambiar la regla de la especificación para conservar el guion en URL y compuestos ([cobertura, §4](../decisiones/COBERTURA_INDEPENDIENTE.md#4-hallazgos)).
6. **Texto alternativo de los gráficos DOCX** cuando el gráfico no tiene datos.
7. **Revisión del renderizado PPTX** cuando aparezcan presentaciones reales con tablas, gráficos o SmartArt, o con picos de memoria cercanos al límite.
8. **OCR local opcional** para PDF escaneados, activable por configuración y con presupuesto propio de CPU y memoria (stack, §4).
9. **Mejoras de PDF según aparezcan casos**: tablas con reglas y celdas combinadas, uso de la estructura etiquetada, parejas etiqueta–valor.

**Fuera del alcance actual:** despliegue multinodo, formatos binarios antiguos (`.doc`, `.xls`, `.ppt`) y con macros, e indexación, embeddings o síntesis de conocimiento, que corresponden a consumidores del bundle.
