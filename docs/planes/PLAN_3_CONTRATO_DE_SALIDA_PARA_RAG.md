# Plan 3: contrato de salida para el pipeline RAG

**Fecha:** 8 de noviembre de 2026.  
**Punto de partida:** los requisitos del contrato de salida que pide el pipeline RAG (lista recibida el 8 de noviembre) contrastados con el perfil 1.2.0, el código y los bundles de referencia.  
**Planes anteriores:** [Plan 1](PLAN_1_RECUPERACION_DEL_PLAN_ORIGINAL.md) (`R1`…`R9`) y [Plan 2](PLAN_2_CALIDAD_Y_ROBUSTEZ.md) (`C1`…`C10`).

## 1. Objetivo

El pipeline pide un contrato de salida común a PDF, PowerPoint, Excel y DOCX que el RAG interprete siempre igual: identificadores, contenido y relaciones. Sus prioridades son tres:

- **identidad estable:** saber qué documento, qué versión y qué unidad es cada fragmento, y obtener los mismos identificadores al repetir la conversión;
- **trazabilidad:** volver desde cada unidad a su lugar exacto en el original;
- **integridad de relaciones:** recuperar unidades completas y ampliar el contexto sin mezclar documentos ni versiones.

La mayor parte del contrato ya existe (§2). Este plan cubre lo que falta, sin cambiar la división en conceptos ni las reglas de extracción.

Tamaños relativos, como en los planes anteriores: **S** (un cambio acotado en un módulo), **M** (varios módulos o una herramienta nueva), **L** (cambio transversal).

## 2. Situación de partida

| Requisito del pipeline | Situación con el perfil 1.2.0 | Trabajo |
| --- | --- | --- |
| Esquema uniforme y versionado | Hecho: modelo de salida común y ensamblador único (`R1`); `profile_version`, `converter_version` y `converter_commit` en `documento.md` y el informe (`C9`). Falta una descripción del frontmatter legible por máquina. | `I9` |
| Identificadores del documento y sus unidades | Implícitos: la versión se reconoce por `source_file.sha256`, la unidad por su ruta en el bundle y el pasaje por `source_spans[].id`. No hay un campo con el identificador ni una identidad lógica que agrupe las versiones de un documento. Los de conversión y de unidad se hicieron después, en el perfil 1.4.0, y la clave del documento en el 1.5.0. | `I1`, `I2` |
| Mismos identificadores con el mismo documento y configuración | Las rutas y los localizadores son deterministas (`C1` lo prueba). La configuración que cambia la salida no se registra, así que la condición no se puede comprobar desde el bundle. | `I3` |
| Trazabilidad: Excel (hoja y rango) | Hecho: `xlsx_range`. | — |
| Trazabilidad: DOCX (ruta estructural, sin páginas) | Hecho: `docx_blocks` en orden estructural. | — |
| Trazabilidad: PowerPoint (diapositiva y elemento) | Diapositiva, sí (`slide_index`, `slide_id`, `display_label`); elemento, no. Hecho después en el perfil 1.8.0. | `I6` |
| Trazabilidad: PDF (página y coordenadas) | Página física por bloque y `page_labels`; coordenadas, no, aunque el lector las tiene. Hecho después en el perfil 1.8.0. | `I6` |
| Fidelidad: texto, numeración, orden, tablas, fórmula frente a valor, omisiones | Hecho (§4 y §5 de la especificación; cobertura independiente, `R2` y `C2`). | — |
| Relaciones padre-hijo válidas | Solo en el Markdown (`index.md`, «Estructura» de `documento.md`, subencabezados); ningún campo explícito. Hecho después en el perfil 1.6.0. | `I4` |
| Enlaces internos resueltos | Los enlaces emitidos resuelven (el validador lo exige). PPTX enlaza otras diapositivas y XLSX otras hojas. DOCX no enlaza los marcadores internos; PDF no lee sus anotaciones de enlace. Hecho después en el perfil 1.7.0. | `I5` |
| Referencias externas identificadas | Hecho para libros externos, imágenes vinculadas y objetos incrustados; los hipervínculos de DOCX, PPTX y XLSX se escriben como enlaces, sin inventario; los de PDF no se leen. Hecho después en el perfil 1.7.0. | `I5` |
| Referencias no resueltas marcadas | No: un destino interno que no existe se descarta en silencio en los cuatro formatos. Hecho después en el perfil 1.7.0. | `I5` |
| Contenido extraído frente a generado | No se generan resúmenes; lo inferido se marca (`title_inferred`, OCR, `status: draft`). `description` y las notas del conversor no se distinguen de forma uniforme. | `I8` |
| Versión del conversor | Hecho (`C9`). | — |
| Configuración utilizada | No se registra (hecho después en el perfil 1.3.0). | `I3` |
| Advertencias de conversión | Hecho: incidencias y observaciones del informe. | — |
| Permisos recibidos | No: la API solo recibe el archivo; un PDF con contraseña se rechaza. | `I7` |
| Estado válido, parcial o fallido; cobertura y errores | Hecho: `complete`, `partial` y `failed`; cobertura por unidad en el informe. | — |

## 3. Identidad

### I1. Identificadores explícitos de conversión y de unidad (M)

**Estado:** implementado el 8 de noviembre (perfil 1.4.0) en `src/identity.rs`, sin `converter_commit` en `conversion_id`, como se decidió. `documento.md` y el informe también llevan `unit_id`. El validador recalcula `conversion_id` con la huella real del original, no con la declarada. Véase la [especificación, §2](../especificacion/ESPECIFICACION_OKF.md#2-estructura-común-del-bundle).

**Problema.** El RAG tiene que componer la clave de cada fragmento (huella del original, ruta, localizador) con reglas que no están escritas en el contrato. Dos conversiones del mismo original con distinta configuración producen las mismas rutas con contenido distinto.

**Trabajo.**

1. `documento.md` e informe declaran `conversion_id`: SHA-256 de la huella del original, `profile_version` y la huella de la configuración de `I3`, escritos en una forma canónica que fija la especificación.
2. Cada concepto declara `unit_id`: `conversion_id` y la ruta del concepto relativa a la raíz. Un pasaje se identifica con `unit_id` y su `source_spans[].id`; la especificación lo dice y no añade otro campo.
3. El validador recalcula `conversion_id` a partir del original incluido y de la configuración declarada, y comprueba que cada `unit_id` corresponda a su ruta.
4. La consulta `GET /jobs/{id}` devuelve también `conversion_id`.

**Aceptación.** Dos conversiones del mismo original con la misma configuración dan los mismos `conversion_id` y `unit_id`; activar el OCR los cambia en un PDF; el validador rechaza un `unit_id` alterado.

**Decisión (8 de noviembre): fuera.** Si `converter_commit` entra en `conversion_id`. Con él, cualquier commit cambia todos los identificadores aunque el bundle sea igual; sin él, un cambio de extracción que no sube el perfil conserva identificadores con contenido distinto. La propuesta es dejarlo fuera y apoyarse en la regla de AGENTS.md: todo cambio de regla sube el perfil.

### I2. Identidad lógica y versiones del documento (M)

**Estado:** implementado el 8 de noviembre (perfil 1.5.0). La clave admite de 1 a 256 caracteres ASCII visibles; puede ir antes o después del archivo en el formulario. Además de `documento.md` y el informe, la consulta de estado y el listado la exponen. Véase la [arquitectura técnica](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#clave-del-documento-plan-3-i2).

**Problema.** Cada subida es un trabajo nuevo con su original. Nada relaciona dos versiones del mismo documento, y el RAG no puede retirar la anterior al indexar la nueva.

**Trabajo.**

1. `POST /jobs` acepta un campo opcional `document_key`: una cadena opaca del sistema de origen, con longitud y caracteres limitados. El servicio no la interpreta.
2. Se guarda con el trabajo (migración nueva) y se escribe en `documento.md` y en el informe; la consulta de estado la devuelve.
3. `GET /jobs?document_key=…` lista los trabajos de esa clave con su huella, su `conversion_id`, su estado y su fecha, en orden de llegada.
4. El servicio no decide cuál es la versión vigente; eso corresponde al consumidor.

**Aceptación.** Dos subidas con la misma clave y distinto original se listan juntas, con huellas distintas; sin clave, el comportamiento actual no cambia; la clave no aparece en el registro de diagnóstico.

### I3. Configuración que afecta a la salida (S)

**Estado:** implementado el 8 de noviembre (perfil 1.3.0) en `src/settings.rs`. En DOCX y XLSX ningún parámetro cambia el bundle y la configuración es un mapa vacío; `reproducible: false` es un campo propio, fuera de `conversion_settings`, para que la huella dependa solo de la configuración. Véase la [especificación, §2](../especificacion/ESPECIFICACION_OKF.md#2-estructura-común-del-bundle).

**Problema.** El mismo original da bundles distintos según `OKF_OCR_ENABLED`, `OKF_OCR_MAX_PAGES`, los modelos del OCR, `OKF_MAX_IMAGE_PIXELS` o la fuente del renderizador PPTX (`OKF_FONT_PATH`), y el bundle no lo dice.

**Trabajo.**

1. Identificar las variables que cambian el contenido del bundle; las que solo rechazan el trabajo (`OKF_MAX_SLIDES`, `OKF_MAX_UPLOAD_MB`…) no cuentan.
2. `documento.md` y el informe declaran `conversion_settings` con esos valores, la revisión y el SHA-256 de los modelos del OCR y la huella de la fuente cuando se usan, y `settings_sha256` en la forma canónica de `I1`.
3. Documentar en la especificación qué entra en la configuración y en la arquitectura técnica, §5, qué variables la forman.

**Aceptación.** Los bundles de referencia muestran la configuración; cambiar una de esas variables cambia `settings_sha256` y no cambiar ninguna lo conserva.

**Riesgo.** Con el OCR activo, una página puede quedar sin transcribir por agotar dos tercios del plazo de conversión, así que la misma configuración no garantiza el mismo bundle. Cuando ocurre, el informe ya lo declara; además, el bundle declarará la conversión como no reproducible, para que el consumidor no confíe en ese identificador.

## 4. Trazabilidad e integridad

### I4. Relaciones explícitas entre unidades (M)

**Estado:** implementado el 8 de noviembre (perfil 1.6.0) en `src/output.rs` y `src/relations.rs`. OKF v0.2 no tiene campos de relación (solo enlaces en el cuerpo y la jerarquía de directorios, y admite claves adicionales en el frontmatter), así que son extensiones del perfil. Solo los conceptos de `contenido/` las declaran. Al implementarlo apareció un fallo de I2, ya corregido: el listado por clave no conservaba el orden de llegada de dos subidas del mismo segundo (migración `0004`). Véase la [arquitectura técnica](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#relaciones-entre-conceptos-plan-3-i4).

**Problema.** Para ampliar el contexto de un fragmento, el RAG necesita saber a qué documento pertenece una unidad, cuál es su padre y cuáles la preceden y la siguen. Hoy solo puede deducirlo de los índices Markdown.

**Trabajo.**

1. Comprobar si OKF v0.2 ofrece un mecanismo para estas relaciones; si no, definirlas como extensiones del perfil, como los campos de la §5.
2. Cada concepto de contenido declara `parent` (el concepto o documento que lo contiene: `documento.md`, o la hoja de una región grande de XLSX), y `previous` y `next` en el orden de lectura de su nivel, como rutas relativas a la raíz.
3. El ensamblador (`src/output.rs`) los calcula a partir del orden de los conceptos; ningún adaptador los escribe por su cuenta.
4. El validador comprueba que cada relación resuelve y no sale del bundle, que `next` y `previous` son recíprocos, que no hay ciclos y que el orden coincide con el de `index.md`.

**Aceptación.** Las relaciones aparecen en los 27 bundles de referencia; las pruebas del validador rechazan un `next` roto, uno no recíproco y un padre de otro bundle.

### I5. Referencias internas y externas (M)

**Estado:** implementado el 8 de noviembre (perfil 1.7.0). Los enlaces PDF se prueban con el corpus real: `p2` enlaza sus 57 citas con la sección de referencias y `p1` y `p2` conservan sus URL. Ninguna muestra tiene un destino PDF ausente, así que ese caso solo se prueba al escribir el texto. Las marcas de DOCX, PPTX y XLSX se prueban rompiendo el destino de una muestra en memoria. En el inventario, en lugar de contar marcas, el validador exige que el informe declare las referencias «sin destino» cuando hay alguna marca: cada formato las agrupa a su manera (bloques, diapositivas, hojas o conceptos). Véase la [arquitectura técnica](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#referencias-internas-y-externas-plan-3-i5).

**Problema.** Una referencia interna sin destino se pierde sin aviso; DOCX y PDF no resuelven las suyas; las referencias externas no tienen inventario.

**Trabajo.**

1. **DOCX:** un hipervínculo con `w:anchor` y sin relación apunta a un marcador; se resuelve al bloque del marcador y se enlaza a su concepto y a su localizador. Los campos `REF` se tratan igual cuando el marcador existe.
2. **PDF:** leer las anotaciones de enlace de cada página (`pdf_oxide` las expone). Un destino interno se resuelve a la página y al bloque que la contiene; una URI se escribe como enlace externo.
3. **PPTX y XLSX:** un destino interno que no existe (diapositiva o hoja ausente, rango inválido) deja de descartarse.
4. Una referencia interna que no se resuelve se escribe como texto, seguida de una marca fija (por ejemplo, «[referencia interna no resuelta]»), y se declara como observación con su ubicación.
5. El informe añade un inventario de referencias externas: URL, libros, imágenes vinculadas y objetos incrustados, con su ubicación.
6. El validador comprueba que el número de marcas coincide con el de referencias no resueltas declaradas.

**Aceptación.** Muestras nuevas, en DOCX y PDF, con un enlace interno resuelto y uno roto; el destino correcto se enlaza y el roto queda marcado y declarado. Los inventarios de los bundles de referencia coinciden con los enlaces del original.

### I6. Coordenadas en PDF y elementos en PowerPoint (M)

**Estado:** implementado el 8 de noviembre (perfil 1.8.0). Dar una caja a cada bloque obligó a cambiar los marcadores PDF: los de página (`p-12`), compartidos por los bloques de una página y ausentes en un concepto de una sola página, pasan a ser uno por bloque (`u-N`). Las coordenadas no aplican la rotación de la página (`/Rotate`). Una imagen que se sale de la página se recorta a la parte visible, porque `p1` tiene una a sangre. Las formas PPTX pueden salirse de la diapositiva, así que el validador no exige que estén dentro. La caja de un párrafo coincide con pdfium dentro de 0,61 pt. Los bundles de referencia crecen un 46 % (PDF) y un 73 % (PPTX). Véase la [arquitectura técnica](../arquitectura/ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#coordenadas-pdf-y-formas-pptx-plan-3-i6).

**Problema.** El pipeline pide localizar el fragmento dentro de la página o de la diapositiva, por ejemplo para resaltarlo en el original. El lector PDF calcula cajas de caracteres y el lector PPTX conoce la posición y el `id` de cada forma, pero ninguna de las dos cosas llega al bundle.

**Trabajo.**

1. **PDF:** los localizadores `pdf_pages` de cada bloque añaden `boxes`, una caja por página, en puntos PDF, con el origen arriba a la izquierda y la rotación de la página aplicada. La maquetación conserva las cajas desde la línea hasta el bloque; las líneas del OCR traen la suya.
2. **PPTX:** un localizador nuevo por forma con contenido (`pptx_shape`): `slide_index`, el `id` de `cNvPr` y su rectángulo en EMU, con un marcador antes de cada bloque, como en DOCX.
3. Fijar en la especificación el sistema de coordenadas y su precisión (por ejemplo, dos decimales), para que el resultado siga siendo determinista.
4. El validador comprueba que cada caja esté dentro de la página o de la diapositiva y que cada `id` de forma sea único en su diapositiva.

**Aceptación.** En las muestras, la caja de un párrafo conocido coincide con la que da pdfium con un margen de un punto; las formas de las diapositivas de referencia tienen localizador. Los bundles crecen, y el aumento se mide y se registra.

## 5. Procedencia, permisos y esquema

### I7. Permisos recibidos (M)

**Problema.** Si el sistema de origen restringe quién puede ver un documento, el RAG necesita esa restricción en cada fragmento. Hoy no hay dónde recibirla ni conservarla.

**Trabajo.**

1. `POST /jobs` acepta un campo opcional `access`: JSON opaco, con tamaño máximo configurable. El servicio no lo interpreta ni lo amplía.
2. Se guarda con el trabajo y se copia sin cambios a `documento.md` y al informe; la consulta de estado lo devuelve. No aparece en el registro de diagnóstico ni en las métricas.
3. Las restricciones del propio original se declaran como observación: los permisos de un PDF cifrado que se abre con la contraseña vacía y la protección de documento o de libro en OOXML.
4. La documentación recuerda que la API no autentica: respetar los permisos de la descarga corresponde al proxy que está delante.

**Aceptación.** El valor recibido aparece byte a byte en el bundle; uno demasiado grande o mal formado responde `400`; un PDF con restricciones de copia se convierte con su observación.

**Decisión abierta.** Si un PDF que prohíbe la extracción de texto debe convertirse con una observación o rechazarse como fallo permanente.

### I8. Contenido generado por el conversor (S)

**Problema.** El conversor no resume, pero escribe texto propio: `description`, el resumen inicial de `documento.md`, las notas de título inferido, de OCR, de página en blanco o de recurso ausente. Para no indexar ese texto como si viniera del original, el RAG necesita reconocerlo sin depender de frases concretas.

**Trabajo.**

1. Cada nota del conversor en el cuerpo va precedida de un marcador `<!-- okf-generated -->`, que se aplica solo al bloque siguiente, como `okf-source-span`.
2. La especificación declara que `description` y el resumen inicial de `documento.md` son texto generado y describe cómo se forman.
3. El validador comprueba que cada marcador va seguido de un bloque.
4. La cobertura independiente (`R2`) sigue igual: el marcador es un comentario y no añade palabras.

**Aceptación.** Las notas de los bundles de referencia llevan el marcador y una prueba quita el texto marcado de todos ellos sin perder palabras del original.

### I9. Esquema del frontmatter legible por máquina (S)

**Problema.** Los campos y su significado están en la especificación en prosa. El RAG no puede comprobar mecánicamente un bundle ni detectar un campo que cambia de significado.

**Trabajo.**

1. Un JSON Schema por versión del perfil (`docs/especificacion/perfil-<versión>.schema.json`) para el frontmatter de cada tipo de concepto, el informe y `documento.md`.
2. Una prueba valida contra él el frontmatter de los 27 bundles de referencia.
3. Regla de trabajo: un cambio de perfil que añade, quita o cambia un campo actualiza su esquema en el mismo commit.

**Aceptación.** Los bundles de referencia cumplen el esquema de su versión; un campo con un tipo alterado lo incumple.

## 6. Versión del perfil y compatibilidad

`I1`, `I3`, `I4`, `I5`, `I6` e `I8` cambian reglas de la especificación. Cada uno sube la versión menor del perfil en su commit, con su entrada en el registro de cambios y los bundles de referencia regenerados y revisados. Todos los campos son nuevos y no cambian los existentes, así que las versiones anteriores pasan a `PREVIOUS_PROFILES` y sus bundles siguen siendo válidos. El RAG tiene que tratar un bundle anterior como si careciera de esos campos, no como inválido.

## 7. Orden

| Fase | Trabajos | Motivo |
| --- | --- | --- |
| A. Identidad | `I3`, `I1`, `I2` | La primera prioridad del pipeline. `I1` necesita la huella de configuración de `I3`. |
| B. Trazabilidad e integridad | `I4`, `I5`, `I6` | Relaciones y referencias para ampliar el contexto; después, las coordenadas que pide la trazabilidad. |
| C. Procedencia y contrato | `I8`, `I7`, `I9` | `I9` al final, para describir el perfil ya completo. |

**Fuera de este plan:** la indexación, la división en fragmentos para embeddings y la elección de la versión vigente, que corresponden al consumidor del bundle; la autenticación de la API, que sigue delegada en un proxy; y la evaluación DOCX y el corpus pendiente, que siguen en el [estado del proyecto](../estado/ESTADO_DEL_PROYECTO.md#6-roadmap-pendiente).
