---
type: "Slide"
title: "Fidelidad (Faithfulness) · Componente: Generador"
description: "Diapositiva 21 de s3.pptx: Fidelidad (Faithfulness) · Componente: Generador. Evalúa si la respuesta generada es coherente con la información recuperada."
sources:
  - id: "original"
    resource: "/references/original.pptx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "slide-21"
    source_id: "original"
    kind: "pptx_slide"
    slide_index: 21
    slide_id: 276
view_hides_text: true
---

# Fidelidad (Faithfulness) · Componente: Generador

> La vista estática muestra una imagen sobre texto que la animación original presenta en secuencia; el texto se conserva abajo.

## Contenido

- Evalúa si la respuesta generada es coherente con la información recuperada.
- Permite identificar y medir afirmaciones en la respuesta que no están sustentadas en el contenido proporcionado.

![Imagen 1 de la diapositiva 21](../assets/slide-021-image-01.png)

## Notas del presentador

El proceso se desarrolla en dos etapas:

Se utiliza un LLM para descomponer la respuesta generada en un conjunto de afirmaciones individuales.

Para cada afirmación, se realiza otra llamada a un LLM para verificar si puede ser inferida o está respaldada por los fragmentos recuperados.

## Vista visual

![Vista de la diapositiva 21](../assets/slide-021.png)
