# Arquitectura conceptual del microservicio de conversión a OKF

**Qué hace el servicio, qué responsabilidades tiene y qué garantiza, sin entrar en el código.**

**Entrada:** un archivo PDF, DOCX, XLSX o PPTX.  
**Salida:** un bundle OKF v0.2 conforme al [perfil documental del proyecto](../especificacion/ESPECIFICACION_OKF.md): el texto organizado en Markdown, los recursos, el documento original y un informe de conversión.

Este documento explica el diseño de forma conceptual. Los detalles de ejecución (procesos, base de datos, límites, reintentos) están en la [arquitectura técnica](ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md).

```mermaid
flowchart TD
    accTitle: Recorrido de una conversión de principio a fin
    accDescr: El servicio registra el archivo, lo inspecciona y lo extrae con el adaptador de su formato. En un PDF escaneado, el OCR local opcional aporta el texto. Después reconcilia la estructura, ensambla el bundle y lo comprueba con dos validaciones independientes. El resultado de ambas determina el estado final y se entrega el bundle con su informe.
    a1("<b>Registro</b><br/>Guardar el original y calcular su huella") --> a2("<b>Inspección</b><br/>Reconocer el formato")
    a2 --> a3("<b>Extracción</b><br/>Adaptador de PDF, DOCX, XLSX o PPTX")
    a3 -. "PDF escaneado, con OKF_OCR_ENABLED" .-> a4("<b>OCR local</b><br/>Leer páginas sin capa de texto")
    a4 -.-> a5
    a3 --> a5("<b>Modelo de lectura</b><br/>La estructura propia del formato")
    a5 --> a6("<b>Reconciliación</b><br/>Orden, jerarquía y referencias")
    a6 --> a7("<b>Ensamblado</b><br/>Un solo ensamblador escribe el bundle")
    a7 --> a8("<b>Conformidad OKF</b><br/>Frontmatter, tipos y enlaces")
    a7 --> a9("<b>Integridad editorial</b><br/>Cobertura, orden y referencias")
    a8 --> a10{"Estado final"}
    a9 --> a10
    a10 --> a11["<b>Bundle e informe</b><br/>complete, partial o failed"]
    classDef input fill:#DBEAFE,stroke:#2563EB,color:#1E3A8A,stroke-width:2px;
    classDef process fill:#EDE9FE,stroke:#7C3AED,color:#4C1D95,stroke-width:2px;
    classDef output fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:2px;
    classDef review fill:#FEF3C7,stroke:#B45309,color:#78350F,stroke-width:2px;
    class a1,a2 input;
    class a3,a4,a5,a6,a7 process;
    class a8,a9,a10 review;
    class a11 output;
    linkStyle default stroke:#64748B,stroke-width:2px;
```

La recepción registra el trabajo y entrega su identificador. Un worker ejecuta las etapas siguientes de forma asíncrona. Las dos validaciones son independientes y determinan el estado del resultado. El OCR local es opcional: solo actúa sobre las páginas PDF escaneadas sin capa de texto, y su texto entra en el mismo modelo de lectura que el texto digital ([OCR local](../decisiones/OCR_LOCAL.md)). Si un detalle del dibujo difiere del texto de este documento, se sigue el texto.

## 1. Frontera y contrato

El microservicio convierte **un archivo de entrada en un bundle OKF** que conserva contenido, organización, recursos y referencias verificables al original. La extracción es documental; la indexación para RAG, los embeddings, las respuestas a preguntas y la síntesis de conocimiento quedan fuera de su responsabilidad. El bundle funciona como capa de fuentes: una capa de conocimiento con conceptos sintetizados para RAG puede construirse después como consumidor independiente, citando los conceptos del bundle mediante `sources` sin modificarlos.

La interacción es **asíncrona**. Tras conservar el original y registrar el trabajo, el servicio entrega un identificador. El solicitante consulta el estado y recupera el bundle al terminar. Un **worker** ejecuta la conversión fuera de la solicitud inicial. El worker forma parte del mismo microservicio; puede comenzar con una sola instancia y no constituye otro servicio.

El archivo original, la identidad y el estado del trabajo, y el resultado se conservan de forma duradera en el entorno local. La memoria de un proceso no es la única fuente de verdad. La arquitectura no requiere servicios en la nube ni prescribe una tecnología de persistencia o de ejecución.

## 2. Flujo interno

| Etapa | Responsabilidad | Resultado |
| --- | --- | --- |
| **Registro** | Identificar el trabajo, conservar el archivo sin modificar y calcular su huella. | Original inmutable y trabajo pendiente. |
| **Inspección** | Reconocer formato y características que afectan la extracción. | Inventario inicial de unidades y necesidades de extracción. |
| **Extracción** | Aplicar el adaptador de PDF, DOCX, XLSX o PPTX. En PDF, si el OCR local está activo, las páginas escaneadas sin capa de texto se transcriben aquí, antes de la maquetación, y su texto sigue el mismo camino que el digital. | Contenido, estructura, recursos, localizadores e incidencias en el modelo de lectura del formato. |
| **Reconciliación** | Resolver orden, jerarquía, continuaciones, notas, referencias y duplicaciones con visión del documento completo. | Estructura editorial consolidada. |
| **Ensamblado** | Generar conceptos Markdown, índices, recursos, procedencia y `conversion-report.md` según el perfil OKF. | Bundle candidato. |
| **Validación y publicación** | Verificar conformidad OKF e integridad editorial por separado; publicar solo un resultado final coherente. | Bundle `complete` o `partial`, o trabajo `failed`. |

Los documentos largos pueden extraerse por partes para limitar memoria. **Las partes de procesamiento no determinan la división final del bundle**: esa división sigue capítulos y secciones, hojas y regiones, o diapositivas. La reconciliación reúne las partes antes del ensamblado.

## 3. Modelos de lectura y modelo de salida

Cada formato guarda su información de manera distinta: un PDF son páginas con caracteres posicionados, un Word son bloques y encabezados, un Excel son hojas y celdas, y un PowerPoint son diapositivas con objetos. Por eso cada adaptador lee el original con un **modelo de lectura propio de su formato**, que conserva la información que el ensamblado necesita:

- **Fuente:** referencia al original inmutable, formato y huella.
- **Unidades:** jerarquía, título y orden de documento, capítulo, sección, hoja, región o diapositiva.
- **Bloques:** párrafos, listas, tablas, figuras, notas, fórmulas y otros elementos, sin reducirlos todos a texto plano.
- **Recursos y relaciones:** archivos asociados, continuaciones, pertenencia y referencias cruzadas.
- **Localizadores:** páginas físicas y etiquetas impresas; bloques DOCX; hoja y rango A1; posición e identificador de diapositiva.
- **Incidencias:** contenido omitido, incierto, inferido o conservado únicamente como recurso visual.

Cada modelo de lectura conserva la estructura propia de su formato: el PDF parte de páginas, con caracteres posicionados que proceden del propio PDF o, en un escaneo, del OCR local ([OCR local](../decisiones/OCR_LOCAL.md)); DOCX, de bloques y encabezados; XLSX, de hojas y regiones; PPTX, de diapositivas, objetos y notas.

Todos los adaptadores entregan su resultado con los mismos tipos, el **modelo de salida común**: conceptos (ruta, tipo, título, campos propios del formato, localizadores y contenido con sus marcadores), recursos e incidencias. Un único ensamblador escribe a partir de él el frontmatter, los índices, `documento.md` y el informe, de modo que las reglas comunes del bundle se aplican en un solo sitio y no necesita interpretar nuevamente los archivos originales.

```mermaid
flowchart LR
    accTitle: De cuatro modelos de lectura a un solo bundle
    accDescr: Cada adaptador lee su formato con un modelo propio y lo convierte al modelo de salida común. Un único ensamblador escribe el bundle a partir de ese modelo.
    b1["<b>PDF</b><br/>Páginas y caracteres"] --> b5("<b>Modelo de salida común</b><br/>Conceptos, recursos e incidencias")
    b2["<b>DOCX</b><br/>Bloques y encabezados"] --> b5
    b3["<b>XLSX</b><br/>Hojas y regiones"] --> b5
    b4["<b>PPTX</b><br/>Diapositivas y objetos"] --> b5
    b5 --> b6("<b>Ensamblador único</b><br/>Frontmatter, índices e informe")
    b6 --> b7["<b>Bundle OKF</b><br/>Mismas reglas para todos"]
    classDef input fill:#DBEAFE,stroke:#2563EB,color:#1E3A8A,stroke-width:2px;
    classDef process fill:#EDE9FE,stroke:#7C3AED,color:#4C1D95,stroke-width:2px;
    classDef output fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:2px;
    class b1,b2,b3,b4 input;
    class b5,b6 process;
    class b7 output;
    linkStyle default stroke:#64748B,stroke-width:2px;
```

El plan inicial preveía un único modelo intermedio común antes del ensamblado. Se sustituyó por esta combinación porque los modelos de lectura ya conservaban los localizadores y unificar la salida da las mismas garantías sobre el bundle con menos cambio ([plan 1, §3](../planes/PLAN_1_RECUPERACION_DEL_PLAN_ORIGINAL.md#3-lo-que-no-se-recupera)).

## 4. Estados y garantías

Un trabajo recorre `queued → processing` y termina en `complete`, `partial` o `failed`. La validación ocurre dentro de `processing`, antes de publicar; no se registra como estado aparte.

```mermaid
flowchart TD
    accTitle: Estados de un trabajo
    accDescr: Un trabajo pasa de queued a processing. Puede terminar complete, partial o failed. Un fallo temporal puede devolverlo a la cola para un reintento limitado. Con retención configurada, los trabajos terminados pasan a expired.
    c1("<b>queued</b><br/>Esperando turno") --> c2("<b>processing</b><br/>Convirtiendo y validando")
    c2 --> c3["<b>complete</b><br/>Bundle conforme, sin pérdida sustantiva"]
    c2 --> c4["<b>partial</b><br/>Bundle conforme con limitaciones declaradas"]
    c2 --> c5["<b>failed</b><br/>No hay bundle publicable"]
    c2 -.->|"Fallo temporal: reintento limitado"| c1
    c3 -.->|"Con retención vencida"| c6["<b>expired</b><br/>Sin archivos; descarga 410"]
    c4 -.-> c6
    c5 -.-> c6
    classDef input fill:#DBEAFE,stroke:#2563EB,color:#1E3A8A,stroke-width:2px;
    classDef process fill:#EDE9FE,stroke:#7C3AED,color:#4C1D95,stroke-width:2px;
    classDef output fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:2px;
    classDef review fill:#FEF3C7,stroke:#B45309,color:#78350F,stroke-width:2px;
    classDef error fill:#FFE4E6,stroke:#BE123C,color:#881337,stroke-width:2px;
    classDef neutral fill:#F1F5F9,stroke:#64748B,color:#334155,stroke-width:2px;
    class c1 input;
    class c2 process;
    class c3 output;
    class c4 review;
    class c5 error;
    class c6 neutral;
    linkStyle default stroke:#64748B,stroke-width:2px;
```

- **`complete`:** bundle conforme y sin pérdida editorial sustantiva no resuelta.
- **`partial`:** bundle conforme con limitaciones localizadas y declaradas en el informe. El bundle se puede descargar.
- **`failed`:** no se pudo producir un bundle publicable o el trabajo no terminó.

El original y el estado sobreviven al reinicio del receptor o del worker. Un trabajo interrumpido puede retomarse desde un punto comprobable o repetirse desde el original. Los archivos en elaboración permanecen separados del bundle publicado; el resultado aparece completo en una sola transición. Repetir un trabajo no debe mezclar archivos de intentos distintos.

La **conformidad OKF** comprueba frontmatter, tipos, archivos reservados y enlaces. La **integridad editorial** comprueba cobertura, orden, recursos, ausencia de duplicaciones y resolución de las referencias al original. Una salida estructuralmente válida con pérdidas declaradas puede ser `partial`; una salida que incumple OKF no se presenta como bundle válido.

En toda la cadena se preservan los localizadores de origen. Ninguna página, bloque, región o diapositiva sustantiva se omite en silencio. Los objetos incrustados y el contenido ejecutable del archivo se tratan como datos, sin ejecutarlos para convertir el documento.

## Retención y expiración

Con retención configurada, los trabajos terminados (`complete`, `partial`, `failed`)
pasan a `expired` cuando vence el plazo desde su finalización. Se eliminan original,
intentos y bundle; se conserva el registro, y la descarga devuelve `410 Gone`.
Sin plazo configurado no se expiran. La política y la limpieza reanudable están en
la [arquitectura técnica, §5](ARQUITECTURA_TECNICA_MICROSERVICIO_OKF.md#5-implementación-en-las-entregas-pptx-docx-xlsx-y-pdf).
