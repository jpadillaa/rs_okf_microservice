---
type: "Slide"
unit_id: "<conversión>/contenido/slide-025.md"
parent: "documento.md"
previous: "contenido/slide-024.md"
next: "contenido/slide-026.md"
title: "Precisión del Contexto · Componente: Recuperador"
description: "Diapositiva 25 de s3.pptx: Precisión del Contexto · Componente: Recuperador. Evalúa si los fragmentos recuperados son realmente relevantes para la pregunta,"
sources:
  - id: "original"
    resource: "/references/original.pptx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "slide-25"
    source_id: "original"
    kind: "pptx_slide"
    slide_index: 25
    slide_id: 280
  - id: "f-416"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 25
    shape_id: 416
    x: 1596175
    y: 2108200
    cx: 10106400
    cy: 4203600
  - id: "f-418"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 25
    shape_id: 418
    x: 3239
    y: 0
    cx: 12185523
    cy: 6858001
view_hides_text: true
---

# Precisión del Contexto · Componente: Recuperador

<!-- okf-generated -->
> La vista estática muestra una imagen sobre texto que la animación original presenta en secuencia; el texto se conserva abajo.

## Contenido

<!-- okf-source-span: f-416 -->
- Evalúa si los fragmentos recuperados son realmente relevantes para la pregunta,
- Penaliza la recuperación de información irrelevante que podría confundir al modelo generador.

<!-- okf-source-span: f-418 -->
![Imagen 1 de la diapositiva 25](../assets/slide-025-image-01.png)

## Notas del presentador

El modelo revisa los primeros K fragmentos (por ejemplo, los 5 más relevantes).

Por cada posición (k), calcula si ese fragmento realmente aporta información útil.

Luego, promedia esos valores, dando más peso a los fragmentos que aparecen antes (porque se supone que deberían ser los más relevantes).

## Vista visual

![Vista de la diapositiva 25](../assets/slide-025.png)
