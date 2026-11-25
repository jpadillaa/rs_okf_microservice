---
type: "Slide"
unit_id: "<conversión>/contenido/slide-020.md"
parent: "documento.md"
previous: "contenido/slide-019.md"
next: "contenido/slide-021.md"
title: "Encontrar fragmentos no es conectarlos"
description: "Diapositiva 20 de s1.pptx: Encontrar fragmentos no es conectarlos. PREGUNTA DE SALTOS MÚLTIPLES"
sources:
  - id: "original"
    resource: "/references/original.pptx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "slide-20"
    source_id: "original"
    kind: "pptx_slide"
    slide_index: 20
    slide_id: 275
    display_label: "20"
block_spans: "/contenido/slide-020.spans.json"
title_inferred: true
kicker: "ARQUITECTURA Y DIAGNÓSTICO · EJEMPLO DE EVOLUCIÓN"
---

# Encontrar fragmentos no es conectarlos

<!-- okf-generated -->
> Título inferido por tamaño y posición del texto; la diapositiva no tiene marcador de título.

## Contenido

<!-- okf-source-span: f-6 -->
- PREGUNTA DE SALTOS MÚLTIPLES

<!-- okf-source-span: f-7 -->
- ¿Quién está a cargo de la unidad que autoriza la cancelación en el Programa P?

<!-- okf-source-span: f-9 -->
- RAG tradicional

<!-- okf-source-span: f-10 -->
- localiza fragmentos precisos

<!-- okf-source-span: f-13 -->
- D1

<!-- okf-source-span: f-14 -->
- Programa P pertenece a la Facultad F

<!-- okf-source-span: f-17 -->
- D2

<!-- okf-source-span: f-18 -->
- Facultad F requiere a la Coordinación C

<!-- okf-source-span: f-21 -->
- D3

<!-- okf-source-span: f-22 -->
- Ana Ruiz está a cargo de la Coordinación C

<!-- okf-source-span: f-23 -->
![Imagen 1 de la diapositiva 20](../assets/slide-020-image-01.svg)

<!-- okf-source-span: f-24 -->
![Imagen 2 de la diapositiva 20](../assets/slide-020-image-02.svg)

<!-- okf-source-span: f-26 -->
- ?

<!-- okf-source-span: f-28 -->
- Conexión directa

<!-- okf-source-span: f-29 -->
- D1 y D2 se recuperan: coinciden con la pregunta

<!-- okf-source-span: f-32 -->
- Limitación de RAG

<!-- okf-source-span: f-33 -->
- D3 solo se alcanza desde D2; una búsqueda única no da ese salto

<!-- okf-source-span: f-35 -->
- GraphRAG

<!-- okf-source-span: f-36 -->
- conecta relaciones

<!-- okf-source-span: f-38 -->
- Programa P

<!-- okf-source-span: f-41 -->
- D1

<!-- okf-source-span: f-43 -->
- Facultad F

<!-- okf-source-span: f-46 -->
- D2

<!-- okf-source-span: f-48 -->
- Coordinación C

<!-- okf-source-span: f-51 -->
- D3

<!-- okf-source-span: f-53 -->
- Ana Ruiz

<!-- okf-source-span: f-55 -->
- GraphRAG construye relaciones entre los datos para responder preguntas de saltos múltiples

<!-- okf-source-span: f-56 -->
- Ejemplo universitario hipotético · Comparación ilustrativa · Microsoft GraphRAG

## Notas del presentador

GUÍA DOCENTE

Los sistemas RAG tradicionales operan como buscadores avanzados que localizan fragmentos de texto precisos. Su límite aparece cuando la respuesta depende del vínculo lógico entre piezas de información que están en fuentes distintas: la búsqueda devuelve cada fragmento por separado, pero no establece la relación entre ellos.

Frente a consultas que exigen conectar varias fuentes independientes —preguntas de saltos múltiples—, la recuperación en una sola pasada puede resultar insuficiente y entregar textos aislados en lugar de una ruta de evidencia. En el ejemplo, D1 y D2 se recuperan porque coinciden con la pregunta, y comparten la Facultad F: esa conexión directa funciona. D3, que nombra a Ana Ruiz, solo se alcanza a partir de la Coordinación C mencionada en D2; una búsqueda única basada en la pregunta original no da ese segundo salto. Ningún fragmento responde por sí solo.

GraphRAG está diseñado para unir la información: construye entidades y relaciones explícitas entre los datos y las usa, junto con los fragmentos, para reunir el contexto. Así puede seguir la cadena completa y responder con sus tres fuentes. Precisión para la discusión: existen también estrategias de recuperación iterativa (p. ej., IRCoT) para este tipo de preguntas, y el aporte del grafo depende de la calidad de las relaciones extraídas y debe evaluarse.

CONDUCCIÓN · 2 min

Estado inicial: pregunta, tres fragmentos y conexión directa D1–D2. Paso 1: el salto fallido hacia D3. Paso 2: GraphRAG y la cadena de relaciones. Paso 3: frase final.

FUENTES

Microsoft GraphRAG · Búsqueda local: https://microsoft.github.io/graphrag/query/local\_search/

IRCoT, Trivedi et al., ACL 2023: https://aclanthology.org/2023.acl-long.557/

## Vista visual

![Vista de la diapositiva 20](../assets/slide-020.png)
