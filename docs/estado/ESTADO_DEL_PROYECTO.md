# Estado del proyecto

**Corte:** 1 de noviembre de 2026, commit `d3a765e` en `main`.  
**Alcance:** qué está implementado, con qué evidencia de calidad, cómo se compara con el plan inicial y qué queda pendiente. Los contratos están en la [especificación](../especificacion/ESPECIFICACION_OKF.md), las [arquitecturas](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md) y el [stack](../decisiones/STACK_TECNOLOGICO_MICROSERVICIO_OKF.md); este documento no los modifica y debe actualizarse al cerrar cada entrega.

## 1. Resumen

Las cuatro entregas previstas están implementadas: el servicio convierte PPTX, DOCX, XLSX y PDF en bundles OKF v0.2 con la misma API, el mismo worker y el mismo validador. Cada formato tiene pruebas de aceptación con documentos sintéticos y, salvo XLSX, con copias reducidas de documentos reales. La suite (48 pruebas), `clippy` sin advertencias y `rustfmt` pasan en local.

Lo que falta no es un formato, sino **evidencia y operación**: evaluar DOCX con archivos guardados por Word, conseguir un corpus XLSX real, y cerrar los requisitos operativos que la arquitectura exige antes de desplegar (límites configurables, retención, imágenes reproducibles).

Frente al plan inicial (§5), el alcance funcional está cumplido, pero los extractores son propios en lugar de Xberg y no existe el modelo intermedio común que prevé la arquitectura conceptual; ambas cosas requieren una decisión explícita.

## 2. Qué está implementado

### 2.1 Servicio

| Pieza | Estado | Detalle |
| --- | --- | --- |
| API HTTP (Axum) | Hecho | `POST /jobs` recibe el archivo y devuelve el identificador; `GET /jobs/{id}` da estado, intento y error; `GET /jobs/{id}/bundle` entrega el ZIP solo si el trabajo terminó `complete` o `partial`. Escucha en `127.0.0.1:8080` por defecto. |
| Registro y cola durable | Hecho | SQLite (WAL, `busy_timeout`) en `OKF_DATA_DIR`; originales, intentos y publicados en `originals/`, `attempts/<job>/<intento>/` y `published/<job>/`. |
| Worker | Hecho | Reclamación exclusiva con asignación renovable, conversión en un proceso hijo vigilado (memoria y tiempo), reintentos con espera creciente para fallos transitorios, fallo inmediato para los permanentes. |
| Publicación y recuperación | Hecho | Publicación atómica por renombrado; al arrancar, reconciliación de publicaciones interrumpidas, bundles perdidos e intentos huérfanos. |
| Validador | Hecho | Conformidad OKF e integridad editorial por separado, con reglas comunes y por formato ([arquitectura técnica, §5](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#5-implementación-en-las-entregas-pptx-docx-xlsx-y-pdf)). Un bundle que no lo supera no se publica. |
| Límites | Hecho, salvo el de subida | Memoria, diapositivas, bloques, celdas, páginas, tiempo, asignación e intentos por variables `OKF_*`. El tamaño máximo de subida (100 MB) está fijo en el código. |
| Registro de diagnóstico | Hecho | `tracing` con el identificador de trabajo, intento y memoria máxima; sin contenido del documento. |

API y worker son subcomandos de un único ejecutable (`okf-microservice api` y `okf-microservice worker`); el stack prevé dos ejecutables. Se ejecutan como dos procesos, que es lo que exige la arquitectura.

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
| Servicio (`src/main.rs`) | 9 | Ruta completa por formato a través del worker, estados `complete`/`partial`/`failed`, clasificación de fallos y reintentos, asignaciones, reconciliación al arrancar y rechazo de bundles alterados por el validador. |
| Proceso hijo (`tests/worker_process.rs`) | 1 | Conversión real en proceso hijo y parada al superar los límites. |
| PPTX | 9 | Extracción con muestras y con el corpus reducido. |
| DOCX | 9 | Corpus real reducido, muestras artificiales y fixtures mínimos. |
| XLSX | 8 | Muestras sintéticas, referencias A1 y lectura de celdas. |
| PDF | 12 | Corpus real reducido, muestras sintéticas, maquetación y lectura. |

La [CI](../../.github/workflows/ci.yml) ejecuta `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` y `cargo test --locked` en Ubuntu 24.04 con cada push a `main`. Todo pasa en local en el commit de corte; el resultado de la CI para ese commit no se ha revisado.

Las pruebas de PDF y del validador alteran bundles válidos (marcador sin bloque, localizador fuera de su sección, tramos desordenados, índices en otro orden) y comprueban que se rechazan.

### 3.2 Evidencia por formato

| Formato | Corpus real | Muestras sintéticas | Resultado y rendimiento (`--release`, validación incluida) |
| --- | --- | --- | --- |
| **PPTX** | 3 presentaciones del curso (`s1`–`s3`), reducidas en `examples/pptx/corpus/` | 4 fixtures | Se convierten las tres; `s3` declara `view_hides_text` en sus cinco diapositivas animadas. Renderizado: 0,85–1,24 s y 288–368 MB de pico. |
| **DOCX** | 3 documentos (`d1`–`d3`), reducidos en `examples/docx/corpus/` | 4 muestras y 2 fixtures | Los tres reales, `complete`; menos de 0,2 s y 25 MB. Se probaron también documentos de otros productores (`textutil`, `python-docx`). |
| **XLSX** | Ninguno | 3 libros | Las muestras dan el estado esperado (dos `complete`, una `partial`). Libro de 800 000 celdas: 3,6 s y 242 MB. |
| **PDF** | 4 PDF; 3 reducidos en `examples/pdf/corpus/` y el recibo solo en local | 4 muestras | Los cuatro reales, `complete`. Presentación de 68 MB: 5,5 s y 306 MB; artículo: 0,16 s; exportación de Word: 0,04 s; recibo: < 0,01 s. Frente a pdfium se conservan el 100 % de las palabras de la exportación, todos los caracteres de la presentación y el 98,8–99,3 % del artículo y del recibo (lo que falta es el encabezado repetido, registrado una vez). |

Las copias reducidas conservan el texto y la estructura y sustituyen imágenes y fuentes; sirven para probar la extracción, no el renderizado. Los originales están en `datos_de_prueba/`, que no se versiona.

### 3.3 Límites conocidos

- **PPTX:** la vista no dibuja los bordes de tabla; las fuentes no instaladas se sustituyen con otras métricas; las diapositivas animadas muestran todas las formas a la vez. No se ha medido la fidelidad con tablas nativas, gráficos ni SmartArt reales. El pico de memoria (hasta 368 MB) queda cerca del límite de 500 MB.
- **DOCX:** solo tres documentos reales, ninguno guardado expresamente por Word para la evaluación; es la comprobación pendiente de mayor valor.
- **XLSX:** toda la evidencia es sintética. Por diseño, las fórmulas no se evalúan y los formatos con unidad o texto no se aplican (se declaran).
- **PDF:** sin OCR (una página escaneada queda como vista y el bundle es `partial`); solo tablas sencillas, sin detección de reglas ni celdas combinadas; no se usa la estructura etiquetada del PDF; en el recibo, algunas parejas etiqueta–valor del encabezado salen como líneas sueltas; las páginas con texto, como diapositivas exportadas, no llevan vista.
- **División:** DOCX y PDF se dividen en un solo nivel (`seccion-NNN.md`). La tabla de la §5 de la especificación menciona índices de partes y capítulos y `capitulo.md`, que no se generan; los párrafos de reglas de esa misma sección describen la división en un nivel que sí se implementa.

## 4. Brechas frente a la arquitectura y el stack

| Requisito | Fuente | Situación |
| --- | --- | --- |
| Límite de tamaño de entrada configurable | Arquitectura técnica, §4 | Fijo en 100 MB (`MAX_UPLOAD`). |
| Control de carga en la admisión | Arquitectura técnica, §4 | La API acepta trabajos sin límite de cola; el worker convierte uno a la vez. |
| Política de retención de originales, intentos y bundles | Arquitectura técnica, §2 | No definida; los publicados y los originales se conservan indefinidamente. |
| Imágenes reproducibles de API y worker con versiones fijadas | Stack, §1 y regla 4 | No existen; el worker necesita además una fuente TrueType para el renderizado PPTX (`OKF_FONT_PATH`). |
| Dos ejecutables | Stack, §1 | Un ejecutable con subcomandos (ver §2.1). |
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
| Xberg como extractor general de los cuatro formatos. | No se usa en ningún formato. | Solo en PDF. El plan pedía medir y registrar resultados antes de sustituir Xberg: en PDF se midió y se descartó con datos; en PPTX, DOCX y XLSX no se midió, como reconoce el [stack](../decisiones/STACK_TECNOLOGICO_MICROSERVICIO_OKF.md). |
| No implementar otro conversor OOXML completo; `zip` y `quick-xml` solo para los localizadores que faltaran. | Se escribió un lector OOXML propio de unas 9 000 líneas para PPTX, DOCX y XLSX. | No. La decisión está justificada en el stack (geometría, notas, comentarios y localizadores directamente del XML), pero contradice una regla explícita del plan. |
| LibreOffice headless para las vistas PPTX, sin cambiar a un renderizador Rust sin comparar antes. | Se cambió a BetterOffice raster. | Sí: se compararon las dos rutas y se aceptaron pérdidas acotadas, como los bordes de tabla ([decisión de renderizado](../decisiones/RENDERIZADO_PPTX_MEDICIONES.md)). |
| Calamine solo si Xberg no daba hoja y rango A1. | No se usa: el lector propio los da. | Sí, coherente con la condición. |
| `pdf_oxide` como alternativa si Xberg no alcanzaba. | Se adoptó para PDF, con maquetación propia. | Sí. |

El proyecto pasó de integrar una biblioteca general a escribir sus propios extractores. Eso dio control sobre los localizadores y la calidad, a cambio de mucho más código propio que mantener y de una decisión tomada sin la medición exigida en tres formatos.

### 5.3 Arquitectura

| Plan inicial | Estado actual |
| --- | --- |
| Modelo intermedio común: todos los adaptadores entregan los mismos tipos (unidades, bloques, recursos, localizadores, incidencias) y un ensamblador genera el bundle ([arquitectura conceptual, §3](../arquitectura/ARQUITECTURA_CONCEPTUAL_MICROSERVICIO_OKF.md)). | No existe. Cada adaptador tiene su propio modelo y su propio ensamblador; solo se comparten incidencias, comentarios, utilidades de Markdown y el validador. Es la mayor desviación arquitectónica: una regla común o un formato nuevo exige tocar cuatro ensambladores. |
| Estados `queued → processing → validating → …`. | `validating` no se registra; la validación ocurre dentro de `processing` (decisión documentada en la arquitectura técnica). |
| Dos ejecutables, `api` y `worker`. | Un ejecutable con dos subcomandos que corren como dos procesos. |
| Límites configurables de entrada, trabajos simultáneos, recursos y tiempo; control de carga en la admisión. | Parcial: memoria, tiempo, páginas, diapositivas, bloques y celdas son configurables; la subida (100 MB) y la expansión ZIP están fijas en el código; la API no limita la cola. |
| Política de retención. | No definida. |
| Imágenes reproducibles de API y worker. | No existen. |
| Registro por trabajo, intento, formato, etapa y duración. | Parcial: trabajo, intento y memoria máxima; no formato, etapa ni duración. |
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

## 6. Roadmap pendiente

En orden de prioridad:

1. **Revisar la CI del commit `d3a765e`.**
2. **Evaluación DOCX con archivos guardados por Word**: las muestras reabiertas y guardadas en Word, o nuevos DOCX reales. Está en pausa hasta disponer de esos archivos.
3. **Corpus real pendiente**: libros XLSX, un PDF escaneado y un PDF largo de texto, para contrastar lo que hoy solo cubren documentos sintéticos.
4. **Modelo intermedio común**: construirlo ahora que hay cuatro adaptadores que unificar, o actualizar la arquitectura conceptual para describir adaptadores autónomos (§5.3).
5. **Xberg en OOXML**: medirlo con el corpus de PPTX, DOCX y XLSX, o registrar formalmente su abandono en el stack (§5.2).
6. **Cierre operativo para desplegar**: límite de subida y control de admisión configurables, política de retención, imágenes reproducibles de API y worker con la fuente de renderizado incluida, y registro con formato, etapa y duración.
7. **Coherencia de la especificación sobre la división**: decidir si DOCX y PDF necesitan índices de partes y capítulos o si la tabla de la §5 de la especificación debe describir la división en un nivel.
8. **Revisión del renderizado PPTX** cuando aparezcan presentaciones reales con tablas, gráficos o SmartArt, o con picos de memoria cercanos al límite.
9. **OCR local opcional** para PDF escaneados, activable por configuración y con presupuesto propio de CPU y memoria (stack, §4).
10. **Mejoras de PDF según aparezcan casos**: tablas con reglas y celdas combinadas, uso de la estructura etiquetada, parejas etiqueta–valor.

**Fuera del alcance actual:** despliegue multinodo, formatos binarios antiguos (`.doc`, `.xls`, `.ppt`) y con macros, e indexación, embeddings o síntesis de conocimiento, que corresponden a consumidores del bundle.
