---
type: "Source Section"
unit_id: "91a3142022a9ec27ac7e8f2f584003ade60d7905b141a9989e2ffee8247f5580/contenido/seccion-003.md"
parent: "documento.md"
previous: "contenido/seccion-002.md"
next: "contenido/seccion-004.md"
title: "2. Resultados"
description: "Sección 3 de informe_revisado.docx: 2. Resultados. La tabla siguiente presenta las métricas por componente y corpus. Los valores del corpus técnico son menores en todas las métricas, en especial en…"
sources:
  - id: "original"
    resource: "/references/original.docx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "seccion"
    source_id: "original"
    kind: "docx_blocks"
    from: 15
    to: 24
  - id: "b-16"
    source_id: "original"
    kind: "docx_blocks"
    from: 16
    to: 16
  - id: "b-17"
    source_id: "original"
    kind: "docx_blocks"
    from: 17
    to: 17
  - id: "b-18"
    source_id: "original"
    kind: "docx_blocks"
    from: 18
    to: 18
  - id: "b-19"
    source_id: "original"
    kind: "docx_blocks"
    from: 19
    to: 19
  - id: "b-20"
    source_id: "original"
    kind: "docx_blocks"
    from: 20
    to: 20
  - id: "b-21"
    source_id: "original"
    kind: "docx_blocks"
    from: 21
    to: 21
  - id: "b-22"
    source_id: "original"
    kind: "docx_blocks"
    from: 22
    to: 22
  - id: "b-23"
    source_id: "original"
    kind: "docx_blocks"
    from: 23
    to: 23
  - id: "b-24"
    source_id: "original"
    kind: "docx_blocks"
    from: 24
    to: 24
---

# 2. Resultados

<!-- okf-source-span: b-16 -->
La tabla siguiente presenta las métricas por componente y corpus. Los valores del corpus técnico son menores en todas las métricas, en especial en exhaustividad.

<!-- okf-source-span: b-17 -->
| **Componente** | **Métrica** | **Valor por corpus** |  |
| --- | --- | --- | --- |
|  |  | **Normativo** | **Técnico** |
| Recuperador | Precisión del contexto | 0,82 | 0,74 |
|  | Exhaustividad del contexto\[nota 2\] | 0,77 | 0,69 |
| Generador | Fidelidad | 0,91 | 0,88 |
|  | Relevancia de la respuesta | 0,86 | 0,81 |

<!-- okf-source-span: b-18 -->
*Tabla 1. Métricas por componente y corpus.*

<!-- okf-source-span: b-19 -->
Los principales riesgos identificados y su mitigación se resumen a continuación:

<!-- okf-source-span: b-20 -->
| **Riesgo** | **Mitigación** |
| --- | --- |
| Alucinaciones en respuestas largas | Verificación por fragmentos:<br>**Paso** · **Responsable**<br>Citas obligatorias · Generador<br>Juez LLM · Evaluación |
| Documentos desactualizados | Versionado de la base y filtros por fecha |

<!-- okf-source-span: b-21 -->
El costo por consulta se mantuvo por debajo del presupuesto previsto.

<!-- okf-source-span: b-22 -->
El costo por consulta se calculó con las tarifas publicadas en septiembre de 2026 y no incluye la infraestructura de observabilidad. La latencia media fue de 2,4 segundos.

<!-- okf-source-span: b-23 -->
**Hallazgo principal:** la mayoría de las respuestas incorrectas se originan en fragmentos mal recuperados.

<!-- okf-source-span: b-24 -->
Se añadió este párrafo durante la revisión para dejar constancia del cambio de criterio.

## Notas al pie

- \[nota 2\] Calculada sobre los cinco primeros fragmentos recuperados.

## Comentarios de revisión

<!-- okf-generated -->
> Comentarios del archivo original; no forman parte del contenido presentado.

- **Andrés Ruiz** · 2026-10-27: Indicar la fuente de las tarifas. (sobre «El costo por consulta»)
  - **Laura Gómez** · 2026-10-28: Añadida en la nota del anexo de costos.

## Revisiones pendientes

<!-- okf-generated -->
> Cambios registrados sin aceptar en el original. El texto principal muestra la vista final: incluye las inserciones y omite las eliminaciones.

- Bloque 21 · traslado (origen): «La latencia media fue de 2,4 segundos.»
- Bloque 22 · traslado (destino): «La latencia media fue de 2,4 segundos.»
- Bloque 23 · cambio de formato: «Hallazgo principal:»
- Bloque 24 · inserción: «Se añadió este párrafo durante la revisión para dejar constancia del cambio de criterio.»
