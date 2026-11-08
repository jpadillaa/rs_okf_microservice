# Perfil documental OKF para PDF, DOCX, XLSX y PPTX

**Versión objetivo:** Open Knowledge Format (OKF) v0.2.  
**Versión del perfil documental:** 1.4.0 (independiente de OKF v0.2).
**Alcance:** especificar exclusivamente el bundle de salida. No define API, arquitectura, herramientas ni ejecución del conversor.

Este perfil añade convenciones para convertir documentos a OKF sin perder estructura, contenido ni referencia al original. Las reglas indicadas como **OKF** proceden de la [especificación oficial](https://github.com/GoogleCloudPlatform/open-knowledge-format/blob/main/SPEC.md); las indicadas como **perfil** son requisitos de este proyecto. Un bundle puede cumplir OKF y aun así tener pérdidas de extracción: ambos controles se evalúan por separado.

## 1. Reglas de formato

| Regla | Requisito |
| --- | --- |
| **OKF: conceptos** | Todo `.md` distinto de `index.md` y `log.md` es un concepto UTF-8 con frontmatter YAML válido y `type` no vacío. Los tipos son abiertos; `title`, `description`, `resource` y `tags` no son obligatorios en OKF. |
| **OKF: reservados** | `index.md` enumera el directorio; solo el índice raíz puede tener frontmatter con `okf_version: "0.2"`. `log.md` registra cambios, no problemas de conversión, y es opcional. |
| **OKF: procedencia** | `sources[].resource` identifica el material fuente; `generated` identifica al productor y el instante de generación. `verified` solo se usa si el contenido fue confirmado efectivamente contra la fuente. |
| **Perfil: metadatos** | Todo concepto documental lleva `type`, `title`, `description`, `sources` y `generated`. `status: draft` señala contenido ambiguo o incompleto. El informe lleva `type`, `title` y `generated`. |
| **Perfil: navegación** | Se generan índices en la raíz y en directorios con varios conceptos. Los enlaces internos emitidos resuelven dentro del bundle; se prefieren rutas relativas a la raíz para enlazar conceptos. |

Las fechas de los campos oficiales usan ISO 8601 con zona horaria explícita. Los campos adicionales `source_file`, `source_spans` y `page_labels`, así como los campos PPTX, DOCX, XLSX y PDF de la §5, **no pertenecen a OKF oficial**. `Attested Computation` tampoco se asigna automáticamente a una fórmula extraída de Excel.

## 2. Estructura común del bundle

```text
bundle/
├── index.md                 # Versión y navegación
├── documento.md             # Fuente completa y su organización
├── contenido/
│   ├── index.md
│   └── ...                  # Conceptos según el formato
├── assets/                  # Figuras, vistas de diapositivas y datos adjuntos
├── references/
│   └── original.<ext>       # Copia inmutable del archivo recibido
└── conversion-report.md     # Cobertura e incidencias; concepto con type
```

**Perfil:** `documento.md` y `conversion-report.md` declaran los mismos `converter_version` (versión del paquete), `converter_commit` (SHA Git de 40 caracteres; `unknown` si la fuente no lo proporciona) y `profile_version` (`1.4.0`). Son extensiones del perfil, no campos oficiales de OKF.

**Perfil: configuración de la conversión.** `documento.md` y `conversion-report.md` declaran también, con el mismo valor, la configuración que cambia el contenido del bundle de su formato (`conversion_settings`) y su huella (`settings_sha256`). Dos conversiones del mismo original con el mismo perfil y la misma huella usaron la misma configuración. Los parámetros que solo limitan o rechazan el trabajo no forman parte de ella.

| Formato | `conversion_settings` |
| --- | --- |
| DOCX, XLSX | Mapa vacío (`{}`): ningún parámetro cambia su bundle. |
| PPTX | `view_font_sha256`: SHA-256 de la fuente con que se dibujan las vistas. |
| PDF | `max_image_pixels`: píxeles máximos de una imagen que se decodifica. `ocr.enabled`: si se aplica el OCR local; solo cuando es `true`, `ocr.max_pages` (páginas transcritas como máximo), `ocr.timeout_s` (plazo de conversión del que sale el tiempo reservado al OCR) y `ocr.det_sha256`, `ocr.rec_sha256` y `ocr.dict_sha256` (SHA-256 de cada archivo de los modelos, o `ausente` si no se encontró). |

`settings_sha256` es el SHA-256, en hexadecimal en minúsculas, de `conversion_settings` en forma canónica: JSON compacto, sin espacios, con las claves de cada mapa ordenadas por sus bytes UTF-8, los enteros en decimal y las cadenas con el escape de JSON (la de `{}` es `44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a`). Cuando el resultado depende del tiempo de ejecución, porque el OCR dejó páginas sin transcribir al agotar su plazo, ambos archivos declaran además `reproducible: false`: la misma configuración puede dar otro bundle. El campo no aparece en los demás casos.

**Perfil: identificadores.** `documento.md` y `conversion-report.md` declaran el mismo `conversion_id`: el SHA-256, en hexadecimal en minúsculas, de `{"profile_version":…,"settings_sha256":…,"source_sha256":…}` en la forma canónica anterior, con la versión del perfil, la huella de la configuración y la huella del original. Todo concepto, incluidos `documento.md` y el informe, declara `unit_id`: `conversion_id`, una barra y su ruta en el bundle (`<conversion_id>/contenido/seccion-003.md`). Un pasaje se identifica con el `unit_id` de su concepto y su `source_spans[].id`. Convertir el mismo original con el mismo perfil y la misma configuración da los mismos identificadores, salvo que la conversión declare `reproducible: false`; cambiar cualquiera de los tres los cambia todos. La versión y el commit del conversor no intervienen: una corrección que cambia el contenido sin cambiar una regla de este perfil conserva los identificadores, y quien necesite distinguirla compara `converter_commit`.

**Perfil:** `documento.md`, `conversion-report.md` y el original son obligatorios. `documento.md` registra `source_file.path` como ruta del original relativa a la raíz del bundle y `source_file.sha256` como 64 caracteres hexadecimales de sus bytes; el informe repite esa huella. Los recursos no Markdown son adjuntos, no conceptos OKF. Toda imagen o archivo de datos sustantivo se enlaza desde el concepto correspondiente. Las rutas son únicas y estables; se usa un ordinal cuando importa el orden editorial. Se puede entregar el directorio como ZIP sin alterar su estructura interna.

## 3. Procedencia y citas al original

`source_file` identifica la versión exacta del archivo; `sources` señala de qué material procede cada concepto; `source_spans` localiza sus unidades dentro de ese material. Cada `source_spans[].source_id` debe coincidir con un `sources[].id` del mismo concepto. Cuando un pasaje necesita una referencia precisa, `source_spans[].id` permite enlazarlo desde el cuerpo Markdown.

| `source_spans[].kind` | Localizador obligatorio | Regla |
| --- | --- | --- |
| `pdf_pages` | `from`, `to` | Páginas **físicas**, desde 1; intervalo inclusivo. |
| `docx_blocks` | `from`, `to` | Bloques en orden estructural del DOCX, desde 1; pueden añadirse encabezados. La página renderizada no es canónica. |
| `xlsx_range` | `sheet`, `range` | Nombre exacto de hoja y rango A1. Se admiten varios rangos no contiguos. |
| `pptx_slide` | `slide_index` | Posición en el archivo, desde 1 e incluidas las ocultas. Se añade `slide_id` si existe y `display_label` si aparece un número visible. |

**PDF:** `documento.md` incluye `page_labels` cuando existan etiquetas impresas legibles, como `"42": "40"` o `"2": "i"`, donde la clave es la página física y el valor es la etiqueta literal. Una página sin etiqueta se omite del mapa. Una cita puede mostrar el número impreso, pero debe conservar el índice físico para resolver ambigüedades. No se infieren etiquetas ilegibles.

**Pasajes:** si un concepto PDF abarca varias páginas, cada bloque citable (párrafo, lista o grupo contiguo, tabla, figura con pie o nota) lleva inmediatamente antes un marcador `<!-- okf-source-span: ID -->`. El marcador corresponde **solo al bloque siguiente**, que debe existir, y puede repetirse. Si el bloque cruza dos páginas, el intervalo refleja ambas. Un concepto contenido íntegramente en una página o diapositiva puede usar únicamente el localizador del frontmatter. En DOCX, XLSX y conceptos que agrupen varias diapositivas se aplica el mismo mecanismo cuando el intervalo general no permite ubicar un bloque con precisión.

Ejemplo de concepto PDF que cubre varias páginas y ubica un pasaje en una de ellas:

```markdown
---
type: Source Section
title: "Diseño experimental"
description: "Sección del documento original."
sources:
  - id: original
    resource: /references/original.pdf
generated: { by: "process:document-conversion", at: "2026-10-27T19:00:00Z" }
source_spans:
  - { id: seccion, source_id: original, kind: pdf_pages, from: 42, to: 49 }
  - { id: pagina-44, source_id: original, kind: pdf_pages, from: 44, to: 44 }
---

# Diseño experimental

<!-- okf-source-span: pagina-44 -->
El texto extraído de esta página aparece aquí.
```

La fecha y el actor del ejemplo son ilustrativos; cada salida registra los valores reales. Todos los marcadores deben resolver a un `source_spans[].id` del concepto. Los localizadores corresponden al original incluido y su huella, no a una edición posterior. Los marcadores son convenciones del perfil, no citas bibliográficas visibles. Las notas al pie de OKF vinculadas a `sources[].id` se reservan para atribuir afirmaciones derivadas cuando existan.

## 4. División y fidelidad comunes

- **Unidad semántica:** un documento breve y coherente puede usar `contenido/texto.md`; uno extenso se divide por sus secciones reales, en un solo nivel: el del encabezado más alto que lo estructura (reglas de DOCX y PDF en §5); los niveles inferiores se conservan como subencabezados dentro de cada concepto. Una hoja se divide en regiones si las tiene; PPTX usa por defecto un concepto por diapositiva. Páginas, filas y tamaños fijos de tokens no determinan la división final.
- **Texto:** se conserva orden, encabezados, listas, referencias y contenido. Se permite unir líneas partidas por maquetación o palabras divididas inequívocamente. No se resume, parafrasea ni completa contenido ausente.
- **Continuaciones:** párrafos, tablas y notas que cruzan páginas o secciones se reconcilian si la continuidad es comprobable. Si no lo es, se mantienen las partes separadas y se declara el problema.
- **Recursos:** figuras y diagramas sustantivos se adjuntan y enlazan cerca de su posición. Una descripción generada no reemplaza la imagen ni se hace pasar por texto extraído; no se inventan pies o textos alternativos.
- **Incertidumbre:** el OCR, los títulos inferidos, el orden de lectura y cualquier interpretación dudosa se identifican junto al contenido y en el informe. Las referencias internas solo se convierten en enlaces si el destino es inequívoco.

## 5. Reglas por formato

| Fuente | Estructura de conceptos | Contenido y casos que deben conservarse |
| --- | --- | --- |
| **PDF** | Breve: `contenido/texto.md`. Extenso: un concepto por sección del nivel de división, con sus subencabezados dentro; `index.md` solo navega. | Clasificar cada página como texto digital, escaneo o mixta. Conservar páginas físicas, etiquetas impresas, orden de lectura, columnas, tablas continuadas, notas, figuras y anexos. Una página solo visual se adjunta y enlaza aunque no pueda transcribirse. Toda página, incluso blanca o ilegible, figura en la cobertura. |
| **DOCX** | Breve: un concepto de contenido. Extenso: un concepto por sección del nivel de división, con la jerarquía de encabezados inferior dentro; localizadores por bloques. | Preservar párrafos, listas, tablas, imágenes, notas y referencias. El texto principal representa la vista final sin modificar el original. Revisiones pendientes, comentarios, cuadros de texto y contenido oculto sustantivo se representan como contenido auxiliar enlazado y se declaran en el informe. No se mezclan versiones editoriales. |
| **XLSX** | `documento.md` describe el libro; un concepto por hoja, con conceptos adicionales por tabla o región significativa. Incluir hojas ocultas. | Toda región no vacía se representa o se declara. Tabla pequeña: Markdown completo. Tabla grande: concepto con esquema, rango, dimensiones y recurso de datos **completo**; CSV UTF-8 para valores rectangulares sencillos, más registros estructurados por celda cuando hay fórmulas, tipos o posiciones que CSV no conserva. Distinguir fórmula, valor almacenado y eventual valor recalculado; no presumir recálculo. Declarar celdas combinadas, errores, formatos con significado, gráficos, tablas dinámicas y vínculos externos no representados fielmente. No crear un concepto por celda o fila. |
| **PPTX** | Un concepto `Slide` por diapositiva, en orden, con `slide_index` y `slide_id` cuando esté disponible. | Conservar texto visible, tablas, imágenes, gráficos y notas del presentador en una sección distinta. Adjuntar una vista visual de cada diapositiva para mantener relaciones espaciales. Conservar series y etiquetas de gráficos si se pueden extraer. Animaciones, audio, video y objetos incrustados sin representación fiel se declaran en el informe. |

**PPTX: campos y señales del perfil.** Se emiten en el frontmatter de cada concepto `Slide` solo cuando aplican; son señales objetivas para consumidores posteriores, no decisiones de indexación.

| Campo | Significado |
| --- | --- |
| `title_inferred: true` | La diapositiva no tiene marcador de título; el título es el texto de mayor tamaño, único, breve y situado en la parte superior. Por esas reglas estrictas no vuelve borrador el concepto: el campo y una nota bajo el título bastan para señalarlo. |
| `kicker` | Antetítulo inferido: texto breve en mayúsculas situado justo encima del título. Se registra solo aquí y no se repite en el contenido. |
| `section` | Nombre de la sección de PowerPoint que contiene la diapositiva, cuando la presentación las define. |
| `visual_only: true` | Lo sustantivo está en una imagen sin texto alternativo que cubre al menos el 15 % de la diapositiva y el texto que la acompaña tiene menos de 40 caracteres. La imagen se adjunta sin transcripción; el concepto lleva `status: draft`. |
| `view_hides_text: true` | La diapositiva tiene animaciones y una imagen grande dibujada sobre texto; la vista estática no muestra ese texto, que sí se conserva en el concepto. |

Un contador visible del tipo `05 / 29`, cuyo total coincide con la presentación, se registra como `display_label` y no se repite como contenido. Un título repartido en varios párrafos con el mismo formato se une con espacios; cuando una parte cambia de formato (por ejemplo, un subtítulo en otro color) se separa con « · ». Un número breve que ocupa por sí solo una forma, contigua a una forma de texto, es el distintivo de un paso o tarjeta y se antepone a ese texto («1. Tarea»). Las imágenes sin texto alternativo útil que ocupan menos del 1 % del área, y las de fondo, se consideran decorativas: no se adjuntan por separado, se cuentan en el informe y permanecen en la vista. El texto de formas fijas del layout o del patrón, visible en varias diapositivas, se registra una vez en `documento.md` bajo «Texto del diseño», con el número de diapositivas que lo muestran; un carácter suelto, como un glifo de una fuente de símbolos, no cuenta como texto.

**PPTX: ecuaciones, comentarios y extensiones.** De cada `mc:AlternateContent` se lee una sola rama: `mc:Choice` cuando el conversor conoce todos sus requisitos (hoy `a14`, que incluye las ecuaciones OMML) y, si no, `mc:Fallback`; así no se duplican formas ni imágenes de respaldo. Las ecuaciones OMML se convierten a una notación lineal cercana a UnicodeMath (`c^2`, `a/b`, `√x`, `∑_(i=1)^n`), se escriben como código en línea y se declaran como observación; la vista conserva su forma. El texto alternativo de formas, grupos y marcos gráficos se conserva en su posición como «Texto alternativo». Los comentarios de revisión, clásicos y modernos, se conservan con autor, fecha y respuestas en la sección «Comentarios de revisión» del concepto, como contenido auxiliar distinto del presentado, y se declaran en el informe.

**DOCX: bloques, división y título.** Un bloque es cada párrafo o tabla de primer nivel del cuerpo, en orden, incluidos los párrafos vacíos; los controles de contenido y el XML personalizado se abren y su contenido cuenta como bloques propios. Los párrafos de celdas, notas, comentarios y cuadros de texto pertenecen al bloque que los contiene. El nivel de un encabezado sale del nivel de esquema del párrafo o de su estilo (incluidos los estilos integrados «heading N»). El documento se divide por el nivel más alto que tenga al menos dos encabezados, en conceptos `contenido/seccion-NNN.md`; si no hay tal nivel o el texto tiene menos de 1500 caracteres, forma un solo concepto `contenido/texto.md`. El contenido anterior al primer encabezado de división forma un concepto de inicio, salvo que solo contenga el título. Los tramos `docx_blocks` de los conceptos son contiguos y cubren todos los bloques; los subencabezados se escriben con `##` en adelante. El título del documento es el párrafo con estilo «Title» al comienzo o, si no lo hay, un primer encabezado seguido sin texto intermedio por otro de igual o mayor rango; o un primer encabezado único de su nivel y de rango superior a todos los demás; se escribe en `documento.md` con el localizador `titulo` y no se repite en los conceptos. En su defecto se usa el título de las propiedades del documento o el nombre del archivo, y el informe indica la procedencia.

**DOCX: campos y señales del perfil.** Cada bloque escrito en un concepto lleva su marcador (`b-12`) y una lista contigua lleva uno solo para todo el grupo (`b-47-50`). Un párrafo breve sin estilo de encabezado, con el mismo tamaño que los encabezados de un nivel y mayor que el texto corriente, se trata como encabezado de ese nivel; si el documento no tiene ningún encabezado con estilo, los párrafos breves en negrita y mayores que el texto corriente son encabezados, con los niveles ordenados por tamaño de mayor a menor. Si un encabezado inferido abre un concepto, este lleva `title_inferred: true` y una nota bajo el título. La negrita y la cursiva se conservan como énfasis Markdown, y una etiqueta en negrita no se convierte en encabezado. Las listas conservan su numeración real, incluida la que continúa tras una interrupción; las etiquetas que Markdown no representa («a)», «1.2.») se escriben como texto tras la viñeta. Un párrafo sin numeración que empieza por una viñeta, un número o una letra con signo seguidos de una tabulación (una lista escrita como texto) se trata como elemento de lista, con el nivel de su sangría. Un párrafo con estilo de cita se escribe como cita Markdown.

**DOCX: contenido auxiliar.** El texto principal es la vista final: incluye las inserciones pendientes, los destinos de los traslados y el texto con cambios de formato, y omite las eliminaciones y los orígenes de los traslados; todos se listan con su tipo y su bloque en la sección «Revisiones pendientes» del concepto. Las notas al pie y finales se marcan en el texto como «[nota N]» y «[nota final N]», numeradas por orden de referencia, y su contenido va en «Notas al pie» y «Notas finales»; no se usa la sintaxis de notas de OKF, reservada para atribuciones. Los comentarios se conservan con autor, fecha, respuestas y el texto que anclan en «Comentarios de revisión». El texto oculto se lista en «Texto oculto». Un cuadro de texto se escribe como cita junto al párrafo que lo ancla, leyendo de cada `mc:AlternateContent` una sola rama, como en PPTX. Una tabla de una sola celda se escribe como cita; en las demás, una celda combinada conserva su contenido y las celdas que cubre quedan vacías. De un campo se conserva el resultado y no la instrucción; el resultado de los que dependen de la paginación (`PAGE`, `NUMPAGES`, `PAGEREF`) se sustituye por «[página]» o «[total de páginas]», y un campo `HYPERLINK` se convierte en enlace. Una tabla de contenido generada (campo `TOC` o control de contenido de la galería «Table of Contents») se resume en una línea con su marcador, porque repite los encabezados y sus números de página no son canónicos. Los párrafos con estilo de código se escriben como bloque de código literal. Los encabezados y pies de página se registran una vez en `documento.md`, con sus imágenes. Las ecuaciones OMML se linealizan como en PPTX. Un SmartArt se representa por el texto de sus nodos. Un SmartArt sin texto, un objeto incrustado, una imagen vinculada a un archivo externo, un gráfico sin datos o una imagen ausente se señalan en su posición, además de en el informe.

**XLSX: hojas, regiones y localizadores.** Cada hoja del libro, en su orden e incluidas las ocultas y las hojas de gráfico, tiene un concepto `contenido/hoja-NNN.md` de tipo `Sheet`, con el nombre exacto de la hoja como título, `sheet_index` (posición desde 1) y, si está oculta, `sheet_state` (`hidden` o `veryHidden`). Una celda tiene contenido si guarda un valor o una fórmula; el localizador `hoja` es el rango que ocupan esas celdas. La hoja se divide en regiones: cada tabla de Excel y cada tabla dinámica es una región; el resto de celdas se agrupa por contigüidad, incluidas las diagonales; las celdas combinadas extienden su bloque, los bloques cuyos rangos se solapan se unen y los textos de una sola columna separados por una fila vacía forman una sola región. Una celda de texto suelta a una o dos filas por encima de una región tabular, dentro de sus columnas, es su título. Cada celda con contenido pertenece a una región, y cada región tiene un localizador `xlsx_range` con su rango. Una región pequeña se escribe en el concepto de su hoja precedida de su marcador (`r-2`): como tabla Markdown completa, con la fila de la hoja en la primera columna y el encabezado detectado o, si no lo hay, las letras de columna; o, si ocupa una sola columna, como lista de celdas. Una región con más de 60 filas de datos o más de 1500 celdas con contenido es grande y tiene concepto propio `contenido/hoja-NNN-region-KK.md` de tipo `Sheet Region`, con su esquema por columna, sus fórmulas, una muestra de las primeras 10 filas declarada como tal (marcador `muestra`) y sus datos completos. Los datos completos son un CSV UTF-8 con el rango entero fila por fila, incluidas las filas vacías. Si la región tiene fórmulas, fechas, booleanos, errores, formatos con significado o celdas combinadas, se añade un archivo JSON Lines con un registro por celda con contenido: celda, tipo, valor almacenado, valor mostrado, fórmula, formato y rango combinado.

**XLSX: valores, fórmulas y formatos.** El valor mostrado parte del valor almacenado:

- las fechas y horas se convierten de su número de serie a ISO 8601 según el sistema del libro (1900 o 1904);
- los porcentajes se multiplican por cien y llevan el signo «%»;
- los números se escriben con la precisión de Excel (15 cifras significativas), sin separadores ni redondeo de presentación;
- los booleanos se escriben `TRUE` o `FALSE`.

Un formato que añade una unidad o un texto (moneda, literales, duraciones) no se aplica y se lista con sus celdas. De cada fórmula se conserva el texto; el conversor no la evalúa. Las celdas dependientes de una fórmula compartida reciben su fórmula reconstruida. Las celdas contiguas cuya fórmula es la anterior con sus referencias relativas desplazadas se listan como un grupo. Una fórmula sin valor almacenado se muestra como «[sin valor almacenado]», y los valores de error se conservan tal como están.

**XLSX: contenido auxiliar.** El concepto de cada hoja registra, con su celda:

- los gráficos, con los datos de su caché y sus rangos de origen;
- las imágenes, adjuntas;
- los cuadros de texto y los objetos incrustados;
- los formatos numéricos con significado y las celdas combinadas;
- las reglas de formato condicional;
- las filas y columnas ocultas;
- los comentarios de revisión: los encadenados con sus respuestas, y los clásicos que no duplican un hilo.

Un hipervínculo de celda se escribe como enlace; si apunta a otra hoja del libro, enlaza su concepto. Una tabla dinámica se representa por sus valores almacenados y su origen de datos. `documento.md` enumera las hojas con su estado y rango, y registra los nombres definidos y los vínculos externos.

**PDF: páginas, división y título.** Las páginas son las físicas, desde 1, y todas figuran en algún concepto. Los encabezados salen de los marcadores del PDF cuando al menos cuatro de cada cinco se encuentran en el texto (cada uno se busca, en orden, como un párrafo con el mismo texto o como sus primeras líneas, a partir de su página de destino); su nivel es la profundidad del marcador. Si no, un párrafo breve (hasta 150 caracteres y tres líneas, que no termina en punto, coma, punto y coma ni dos puntos) con letra al menos un 10 % mayor que el texto corriente es encabezado, con los niveles ordenados por tamaño de mayor a menor; el concepto que abre lleva `title_inferred: true` y una nota bajo el título. El título del documento es el primer encabezado de la primera página si le sigue directamente otro de igual o mayor rango, o si es el único de su nivel y supera a todos los demás; en su defecto, el título de las propiedades o el nombre del archivo. Se escribe en `documento.md` con el localizador `titulo` y no se repite en los conceptos. El documento se divide como un DOCX, por el nivel más alto con al menos dos encabezados, en conceptos `contenido/seccion-NNN.md`, salvo que el primer encabezado de ese nivel llegue después de otros de menor rango (un «¡Gracias!» de cierre en letra mayor que los títulos de cada parte no divide); con menos de 1500 caracteres o sin tal nivel, forma `contenido/texto.md`. El contenido anterior al primer encabezado de división forma un concepto de inicio. Cada concepto lleva el localizador `seccion` (`pdf_pages`) con su primera y última página; dos conceptos seguidos pueden compartir la página en la que uno termina y el otro empieza. En un concepto de varias páginas, cada bloque (párrafo, lista contigua, tabla, imagen, subencabezado o nota de página) lleva el marcador de su página (`p-12`) o de las dos páginas que cruza (`p-12-13`).

**PDF: texto, orden de lectura y bloques.** Una línea de la franja superior o inferior de la página (el 12 % de su altura) que se repite, igual o con solo el número de la página cambiado, en al menos el 30 % de las páginas (y en dos como mínimo) es un encabezado o pie: se registra una vez en `documento.md` con sus páginas y no se repite en el contenido. Un número de página impreso en esas franjas («12», «xii», «- 12 -», «Página 12 de 40», «Page 4 of 9», también al final de un encabezado) se registra en `page_labels` cuando mantiene la misma diferencia con la página física en al menos dos páginas o declara el total de páginas del documento; las etiquetas internas `/PageLabels` se listan aparte en `documento.md` y no lo sustituyen. El orden de lectura se reconstruye por cortes de espacio en blanco: en cada región se prefiere un canal vertical que la atraviese de arriba abajo (columnas, que se leen de izquierda a derecha) a un espacio horizontal (bandas, de arriba abajo); dos partes con líneas alineadas fila a fila, alguna de ellas breve, son celdas de una tabla y no columnas. Las filas consecutivas de varias celdas forman una tabla Markdown, y una línea suelta que continúa una celda se une a ella. Las líneas seguidas con el mismo formato forman un párrafo hasta un espacio vertical mayor que el habitual o una frase que termina en una línea corta; una línea que empieza con viñeta o número es un elemento de lista, que conserva su número. En una palabra partida con guion al final de la línea, el guion se conserva si la palabra compuesta aparece con guion dentro de alguna línea del documento y se suprime si la palabra unida aparece entera. Si el documento no decide, el guion se conserva cuando la palabra forma parte de una dirección (contiene «://», «www.», «/» o «@», o un punto seguido de letra o cifra a cualquier lado del corte: `langchain-ai.github.io`) o cuando la parte anterior ya lleva un guion entre letras o cifras, como en una serie (`if-then-else`); en los demás casos solo se suprime entre minúsculas. Las palabras compuestas cuyas dos partes son palabras por sí solas (`real-time`) no se distinguen de una palabra partida por sílabas y se unen si no aparecen con guion en otra línea. Un párrafo cuya última línea no cierra la frase y que sigue en minúscula, con el mismo formato, en la columna o la página siguiente se une a su continuación; las notas en letra menor que quedan en medio (afiliaciones, notas al pie) pasan detrás del párrafo unido. La negrita y la cursiva se conservan como énfasis Markdown; el texto en letra monoespaciada se escribe como bloque de código con su sangría, y el texto girado, al final del contenido de su página.

**PDF: clases de página y recursos.** Cada página se clasifica, y la cobertura del informe la indica:

- **texto digital:** texto propio, fuera de encabezados y pies;
- **mixta:** texto y una imagen de contenido que cubre al menos el 30 % de la página; la imagen se adjunta sin transcribir;
- **escaneo sin texto:** sin texto propio y con una imagen de ese tamaño; se adjunta la vista de la página, una nota en su posición indica que no se aplicó OCR o por qué no dio texto (límite de páginas o de tiempo del OCR, ningún texto reconocido o fallo del OCR), el concepto lleva `status: draft` y la página es una incidencia;
- **escaneo con capa de OCR:** texto escrito en una fuente sin glifos sobre una imagen grande; se conserva ese texto, que no se verifica, y se adjunta la vista;
- **escaneo con OCR del servicio:** con el OCR local activo (`OKF_OCR_ENABLED`), un escaneo sin texto se transcribe antes de buscar encabezados y pies. La página se endereza y se reconocen sus líneas; el tamaño de letra de cada línea se mide por su tinta. El texto sigue las mismas reglas que el texto digital: orden de lectura, bloques, encabezados y pies repetidos, números de página impresos y encabezados inferidos por tamaño (sin negrita ni cursiva, que el OCR no distingue). Se adjunta la vista; una nota en su posición da la confianza media y cuántas líneas tienen confianza baja (menos del 80 %). Todo concepto con texto de la página lleva `status: draft` y, si la nota de la página quedó en otro concepto, una nota bajo su título. El informe registra la página como observación, o como incidencia si su confianza media es inferior al 80 %. Se transcriben como máximo las primeras `OKF_OCR_MAX_PAGES` páginas escaneadas de cada documento, y ninguna más tras consumir dos tercios del plazo de conversión; las demás quedan como escaneo sin texto con el motivo en su nota y en el informe;
- **solo visual:** sin texto ni imagen grande, pero con dibujos; se adjunta su vista;
- **en blanco:** nada fuera de encabezados, pies o número de página; una nota la señala en su posición.

Las imágenes que cubren menos del 1 % de la página son decorativas: se cuentan y no se adjuntan. Una imagen que se repite en tres páginas o más, o en al menos dos que sean la mitad del documento (logotipos, fondos de plantilla), se registra una vez en `documento.md`. Las demás se adjuntan en su posición de lectura; una imagen que tapa texto (un fondo) se coloca antes del primer bloque que empieza debajo de su borde superior. Un carácter sin correspondencia Unicode se escribe como «�» (U+FFFD) y es una incidencia.

En DOCX la paginación que muestra un editor depende de su representación y no sustituye los bloques estructurales. En PPTX un número visible (`display_label`) no se deduce de la posición (`slide_index`). En XLSX una muestra de filas nunca se presenta como tabla completa.

## 6. Informe y aceptación

`conversion-report.md` es un concepto OKF, por ejemplo `type: Conversion Report`. Registra formato y SHA-256 del original; inventario frente a unidades representadas; cobertura de páginas, bloques, hojas/regiones o diapositivas; transformaciones (OCR, unión de tablas, deduplicación); recursos; y cada omisión o ambigüedad con ubicación. Su estado propio es `complete` o `partial`: `complete` no significa reproducción visual idéntica ni revisión humana; `partial` identifica pérdida o incertidumbre sustantiva sin ocultarla. El problema también se señala junto al contenido afectado.

El informe separa dos listas. **Incidencias** son pérdidas o incertidumbres sustantivas y determinan `partial`: contenido solo visual sin transcripción, una vista que no muestra texto del original, objetos incrustados, audio o video, SmartArt sin texto extraído, recursos que no pudieron adjuntarse o que el renderizador omitió, imágenes vinculadas a archivos externos, gráficos sin datos, fragmentos importados (`altChunk`); en XLSX, fórmulas sin valor almacenado, vínculos a libros externos y hojas de diálogo o de macros; y, en PDF, páginas escaneadas sin capa de texto ni texto reconocido por el OCR, texto de OCR con confianza media baja, vistas de página que no pudieron generarse, imágenes que no se pudieron decodificar y caracteres sin correspondencia Unicode. **Observaciones** informan sin cambiar el estado: diapositivas sin título, títulos inferidos, animaciones que no ocultan contenido, diapositivas sin contenido propio, texto del diseño registrado en `documento.md`, ecuaciones linealizadas, comentarios de revisión conservados y, en DOCX, encabezados inferidos por formato, listas escritas como texto, revisiones pendientes, texto oculto, cuadros de texto, celdas combinadas, SmartArt representado por su texto, tablas de contenido resumidas y la procedencia del título; en XLSX, hojas, filas y columnas ocultas, celdas combinadas, valores de error, formatos numéricos con significado, formato condicional, tablas dinámicas, gráficos representados por su caché, regiones grandes con concepto propio, fechas convertidas, el recálculo que pide el libro al abrirse y la procedencia del título; en PDF, páginas escaneadas con una capa de OCR previo o transcritas con el OCR del servicio, páginas solo visuales, mixtas o en blanco, páginas leídas en varias columnas, párrafos unidos con su continuación en otra columna o página, palabras partidas unidas, tablas reconstruidas por alineación, texto girado, encabezados inferidos por su tamaño o tomados de los marcadores, encabezados y pies registrados en `documento.md`, números de página impresos, etiquetas internas, imágenes decorativas y la procedencia del título. Una incidencia u observación que afecta a varias diapositivas, bloques, celdas o páginas se informa en una sola línea con su lista.

Un bundle de este perfil se acepta cuando se cumplen **ambos** controles:

1. **Conformidad OKF:** YAML y `type` válidos en todos los conceptos; índices y nombres reservados correctos; `okf_version: "0.2"` en el índice raíz; campos oficiales bien formados; enlaces internos y recursos emitidos resolubles.
2. **Integridad editorial:** orden y cobertura frente al original; contenido sin duplicaciones introducidas ni omisiones silenciosas; recursos presentes; `source_file`, `sources`, `source_spans`, `page_labels` y marcadores coherentes; incidencias declaradas. Cada bloque citable de un concepto PDF multipágina debe poder localizarse en su página o intervalo preciso.

El perfil se limita a `.pdf`, `.docx`, `.xlsx` y `.pptx`. Los formatos binarios antiguos `.doc`, `.xls` y `.ppt`, así como los formatos con macros, requieren reglas adicionales y no quedan cubiertos por el nombre comercial del programa.

## Referencias

- [Open Knowledge Format v0.2, especificación oficial](https://github.com/GoogleCloudPlatform/open-knowledge-format/blob/main/SPEC.md).
- [Adobe, estructura y orden de lectura de PDF](https://helpx.adobe.com/acrobat/using/accessibility-features-pdfs.html).
- [Microsoft, WordprocessingML](https://learn.microsoft.com/en-us/office/open-xml/word/structure-of-a-wordprocessingml-document).
- [Microsoft, SpreadsheetML](https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/structure-of-a-spreadsheetml-document).
- [Microsoft, PresentationML](https://learn.microsoft.com/es-es/office/open-xml/presentation/structure-of-a-presentationml-document).

## Registro de cambios del perfil

Todo cambio de una regla de esta especificación incrementa `profile_version` en la
especificación y en `src/version.rs`, con una entrada aquí, en el mismo commit.

- **1.4.0 — 8 de noviembre de 2026:** `documento.md` y el informe declaran
  `conversion_id`, que sale del perfil, la configuración y la huella del original, y
  cada concepto declara `unit_id`, que es `conversion_id` seguido de su ruta (§2; plan 3,
  I1). El contenido de los bundles no cambia. El validador recalcula `conversion_id` con
  la huella real del original y comprueba cada `unit_id`; acepta también los bundles
  1.0.0 a 1.3.0, que no declaran identificadores.
- **1.3.0 — 8 de noviembre de 2026:** `documento.md` y el informe declaran la
  configuración que cambia el contenido del bundle (`conversion_settings`), su huella
  canónica (`settings_sha256`) y, cuando el plazo del OCR dejó páginas sin transcribir,
  `reproducible: false` (§2; plan 3, I3). El contenido de los bundles no cambia. El
  validador recalcula la huella, exige las claves de cada formato y acepta también los
  bundles 1.0.0, 1.1.0 y 1.2.0, que no declaran configuración.
- **1.2.0 — 7 de noviembre de 2026:** en PDF, el guion de fin de línea se conserva
  también en direcciones (URL, rutas, correos y dominios) y en series cuya primera parte
  ya lleva guion, cuando el documento no atestigua la palabra unida ni el compuesto.
  En el corpus cambia `p4`: `langchain-ai.github.io`, `openai-agents-python` e
  `if-then-else` dejan de perder el guion. El validador acepta también los bundles
  1.0.0 y 1.1.0.
- **1.1.0 — 6 de noviembre de 2026:** añade la clase de página PDF «escaneo con OCR
  del servicio», opcional y desactivada por defecto, con sus notas, observaciones e
  incidencias; la nota de un escaneo sin texto indica por qué el OCR no dio texto (límite de páginas o de tiempo, ningún texto reconocido o fallo); y un
  concepto con texto de una página escaneada (con OCR previo o del servicio) lleva
  `status: draft` aunque la nota de la página esté en otro concepto. Sin el OCR activo,
  los bundles no cambian salvo `profile_version`. El validador acepta también los
  bundles 1.0.0.
- **1.0.0 — 2 de noviembre de 2026:** primera versión explícita; conserva las reglas
  de conversión existentes y exige identificar conversor, commit y perfil en el
  documento fuente y el informe. El validador reconoce además bundles anteriores sin
  esos campos como legado sin versión; rechaza metadatos incompletos, discordantes
  o perfiles versionados que no conoce.
