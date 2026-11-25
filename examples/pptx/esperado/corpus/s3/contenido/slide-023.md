---
type: "Slide"
unit_id: "<conversión>/contenido/slide-023.md"
parent: "documento.md"
previous: "contenido/slide-022.md"
next: "contenido/slide-024.md"
title: "Relevancia de la Respuesta · Componente: Generador"
description: "Diapositiva 23 de s3.pptx: Relevancia de la Respuesta · Componente: Generador. Esta métrica evalúa cuán pertinente es la respuesta generada para la pregunta original,"
sources:
  - id: "original"
    resource: "/references/original.pptx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "slide-23"
    source_id: "original"
    kind: "pptx_slide"
    slide_index: 23
    slide_id: 278
block_spans: "/contenido/slide-023.spans.json"
view_hides_text: true
---

# Relevancia de la Respuesta · Componente: Generador

<!-- okf-generated -->
> La vista estática muestra una imagen sobre texto que la animación original presenta en secuencia; el texto se conserva abajo.

## Contenido

<!-- okf-source-span: f-401 -->
- Esta métrica evalúa cuán pertinente es la respuesta generada para la pregunta original,
- Penaliza las respuestas que son incompletas o que contienen información redundante y superflua.

<!-- okf-source-span: f-403 -->
![Imagen 1 de la diapositiva 23](../assets/slide-023-image-01.png)

## Notas del presentador

El método se basa en la ingeniería inversa:

Se le pide a un LLM que genere un conjunto de N preguntas plausibles que podrían haber dado lugar a las respuestas proporcionadas.

Se calculan los embeddings tanto de la pregunta original como de las N preguntas generadas.

## Vista visual

![Vista de la diapositiva 23](../assets/slide-023.png)
