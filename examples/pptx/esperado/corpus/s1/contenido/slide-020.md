---
type: "Slide"
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
title_inferred: true
kicker: "ARQUITECTURA Y DIAGNÓSTICO · EJEMPLO DE EVOLUCIÓN"
---

# Encontrar fragmentos no es conectarlos

> Título inferido por tamaño y posición del texto; la diapositiva no tiene marcador de título.

## Contenido

- PREGUNTA DE SALTOS MÚLTIPLES
- ¿Quién está a cargo de la unidad que autoriza la cancelación en el Programa P?
- RAG tradicional
- localiza fragmentos precisos
- D1
- Programa P pertenece a la Facultad F
- D2
- Facultad F requiere a la Coordinación C
- D3
- Ana Ruiz está a cargo de la Coordinación C

![Imagen 1 de la diapositiva 20](../assets/slide-020-image-01.svg)

![Imagen 2 de la diapositiva 20](../assets/slide-020-image-02.svg)

- ?
- Conexión directa
- D1 y D2 se recuperan: coinciden con la pregunta
- Limitación de RAG
- D3 solo se alcanza desde D2; una búsqueda única no da ese salto
- GraphRAG
- conecta relaciones
- Programa P
- D1
- Facultad F
- D2
- Coordinación C
- D3
- Ana Ruiz
- GraphRAG construye relaciones entre los datos para responder preguntas de saltos múltiples
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
