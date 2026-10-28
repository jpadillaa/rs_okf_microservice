# Perfil documental OKF para PDF, DOCX, XLSX y PPTX

**Versión objetivo:** Open Knowledge Format (OKF) v0.2.  
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

Las fechas de los campos oficiales usan ISO 8601 con zona horaria explícita. Los campos adicionales `source_file`, `source_spans` y `page_labels` definidos abajo **no pertenecen a OKF oficial**. `Attested Computation` tampoco se asigna automáticamente a una fórmula extraída de Excel.

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

**Pasajes:** si un concepto PDF abarca varias páginas, cada bloque citable (párrafo, lista o grupo contiguo, tabla, figura con pie o nota) lleva inmediatamente antes un marcador `<!-- okf-source-span: ID -->`. El marcador corresponde **solo al bloque siguiente** y puede repetirse. Si el bloque cruza dos páginas, el intervalo refleja ambas. Un concepto contenido íntegramente en una página o diapositiva puede usar únicamente el localizador del frontmatter. En DOCX, XLSX y conceptos que agrupen varias diapositivas se aplica el mismo mecanismo cuando el intervalo general no permite ubicar un bloque con precisión.

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

- **Unidad semántica:** un documento breve y coherente puede usar `contenido/texto.md`; uno extenso se divide por partes, capítulos y secciones reales. Una hoja se divide en regiones si las tiene; PPTX usa por defecto un concepto por diapositiva. Páginas, filas y tamaños fijos de tokens no determinan la división final.
- **Texto:** se conserva orden, encabezados, listas, referencias y contenido. Se permite unir líneas partidas por maquetación o palabras divididas inequívocamente. No se resume, parafrasea ni completa contenido ausente.
- **Continuaciones:** párrafos, tablas y notas que cruzan páginas o secciones se reconcilian si la continuidad es comprobable. Si no lo es, se mantienen las partes separadas y se declara el problema.
- **Recursos:** figuras y diagramas sustantivos se adjuntan y enlazan cerca de su posición. Una descripción generada no reemplaza la imagen ni se hace pasar por texto extraído; no se inventan pies o textos alternativos.
- **Incertidumbre:** el OCR, los títulos inferidos, el orden de lectura y cualquier interpretación dudosa se identifican junto al contenido y en el informe. Las referencias internas solo se convierten en enlaces si el destino es inequívoco.

## 5. Reglas por formato

| Fuente | Estructura de conceptos | Contenido y casos que deben conservarse |
| --- | --- | --- |
| **PDF** | Breve: `contenido/texto.md`. Extenso: índices de partes/capítulos y conceptos de secciones; `capitulo.md` contiene texto introductorio, mientras `index.md` solo navega. | Clasificar cada página como texto digital, escaneo o mixta. Conservar páginas físicas, etiquetas impresas, orden de lectura, columnas, tablas continuadas, notas, figuras y anexos. Una página solo visual se adjunta y enlaza aunque no pueda transcribirse. Toda página, incluso blanca o ilegible, figura en la cobertura. |
| **DOCX** | Breve: un concepto de contenido. Extenso: jerarquía de encabezados, capítulos y secciones; localizadores por bloques. | Preservar párrafos, listas, tablas, imágenes, notas y referencias. El texto principal representa la vista final sin modificar el original. Revisiones pendientes, comentarios, cuadros de texto y contenido oculto sustantivo se representan como contenido auxiliar enlazado y se declaran en el informe. No se mezclan versiones editoriales. |
| **XLSX** | `documento.md` describe el libro; un concepto por hoja, con conceptos adicionales por tabla o región significativa. Incluir hojas ocultas. | Toda región no vacía se representa o se declara. Tabla pequeña: Markdown completo. Tabla grande: concepto con esquema, rango, dimensiones y recurso de datos **completo**; CSV UTF-8 para valores rectangulares sencillos, más registros estructurados por celda cuando hay fórmulas, tipos o posiciones que CSV no conserva. Distinguir fórmula, valor almacenado y eventual valor recalculado; no presumir recálculo. Declarar celdas combinadas, errores, formatos con significado, gráficos, tablas dinámicas y vínculos externos no representados fielmente. No crear un concepto por celda o fila. |
| **PPTX** | Un concepto `Slide` por diapositiva, en orden, con `slide_index` y `slide_id` cuando esté disponible. | Conservar texto visible, tablas, imágenes, gráficos y notas del presentador en una sección distinta. Adjuntar una vista visual de cada diapositiva para mantener relaciones espaciales. Conservar series y etiquetas de gráficos si se pueden extraer. Animaciones, audio, video y objetos incrustados sin representación fiel se declaran en el informe. |

En DOCX la paginación que muestra un editor depende de su representación y no sustituye los bloques estructurales. En PPTX un número visible (`display_label`) no se deduce de la posición (`slide_index`). En XLSX una muestra de filas nunca se presenta como tabla completa.

## 6. Informe y aceptación

`conversion-report.md` es un concepto OKF, por ejemplo `type: Conversion Report`. Registra formato y SHA-256 del original; inventario frente a unidades representadas; cobertura de páginas, bloques, hojas/regiones o diapositivas; transformaciones (OCR, unión de tablas, deduplicación); recursos; y cada omisión o ambigüedad con ubicación. Su estado propio es `complete` o `partial`: `complete` no significa reproducción visual idéntica ni revisión humana; `partial` identifica pérdida o incertidumbre sustantiva sin ocultarla. El problema también se señala junto al contenido afectado.

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
