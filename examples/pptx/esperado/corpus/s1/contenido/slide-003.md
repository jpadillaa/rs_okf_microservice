---
type: "Slide"
unit_id: "<conversión>/contenido/slide-003.md"
parent: "documento.md"
previous: "contenido/slide-002.md"
next: "contenido/slide-004.md"
title: "¿Qué es RAG?"
description: "Diapositiva 3 de s1.pptx: ¿Qué es RAG?. Recuperar información externa, incorporarla al contexto y generar una respuesta con esa evidencia"
sources:
  - id: "original"
    resource: "/references/original.pptx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "slide-3"
    source_id: "original"
    kind: "pptx_slide"
    slide_index: 3
    slide_id: 258
    display_label: "03"
  - id: "f-5"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 5
    x: 1047750
    y: 1864162
    cx: 17811750
    cy: 426720
  - id: "f-6"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 6
    x: 914400
    y: 2724150
    cx: 10858500
    cy: 6115050
  - id: "f-9"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 9
    x: 12954000
    y: 2933700
    cx: 4714875
    cy: 381000
  - id: "f-10"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 10
    x: 12954000
    y: 3352800
    cx: 4414838
    cy: 678180
  - id: "f-13"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 13
    x: 12954000
    y: 4392930
    cx: 4714875
    cy: 381000
  - id: "f-14"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 14
    x: 12954000
    y: 4812030
    cx: 4714875
    cy: 358140
  - id: "f-17"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 17
    x: 12954000
    y: 5532120
    cx: 4714875
    cy: 381000
  - id: "f-18"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 18
    x: 12954000
    y: 5951220
    cx: 4714875
    cy: 358140
  - id: "f-20"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 20
    x: 12534900
    y: 7286625
    cx: 4606290
    cy: 1135380
  - id: "f-21"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 21
    x: 1047750
    y: 9315450
    cx: 3063621
    cy: 266700
  - id: "f-22"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 22
    x: 3947160
    y: 9315450
    cx: 10470035
    cy: 266700
  - id: "f-23"
    source_id: "original"
    kind: "pptx_shape"
    slide_index: 3
    shape_id: 23
    x: 1047750
    y: 9715500
    cx: 6117289
    cy: 266700
title_inferred: true
kicker: "GENERACIÓN AUMENTADA POR RECUPERACIÓN"
---

# ¿Qué es RAG?

> Título inferido por tamaño y posición del texto; la diapositiva no tiene marcador de título.

## Contenido

<!-- okf-source-span: f-5 -->
- Recuperar información externa, incorporarla al contexto y generar una respuesta con esa evidencia

<!-- okf-source-span: f-6 -->
![Imagen 1 de la diapositiva 3](../assets/slide-003-image-01.png)

<!-- okf-source-span: f-9 -->
- 1\. Recuperar

<!-- okf-source-span: f-10 -->
- Seleccionar G y E entre las fuentes autorizadas

<!-- okf-source-span: f-13 -->
- 2\. Ampliar el contexto

<!-- okf-source-span: f-14 -->
- Instrucciones + consulta + evidencia

<!-- okf-source-span: f-17 -->
- 3\. Generar

<!-- okf-source-span: f-18 -->
- El LLM responde citando \[G\] y \[E\]

<!-- okf-source-span: f-20 -->
- La evidencia recuperada forma parte del contexto de esta consulta

<!-- okf-source-span: f-21 -->
- Reglamentos estudiantiles:

<!-- okf-source-span: f-22 -->
- [https://www.uniandes.edu.co/es/la-universidad/secretaria-general/reglamentos-estudiantiles](https://www.uniandes.edu.co/es/la-universidad/secretaria-general/reglamentos-estudiantiles)

<!-- okf-source-span: f-23 -->
- Caso y respuesta ilustrativos · La calidad debe evaluarse

## Notas del presentador

GUÍA DOCENTE

Presente la Generación Aumentada por Recuperación (RAG) como un proceso en el que la aplicación recupera información desde fuentes externas, la incorpora al contexto del modelo junto con la consulta y las instrucciones y utiliza ese contexto para generar una respuesta. Siga visualmente los tres momentos: recuperar, ampliar el contexto y generar. El modelo recibe información seleccionada para esa solicitud; por ello, la búsqueda, el contexto y la generación son responsabilidades distintas. La recuperación puede ser léxica, semántica o estructurada, según la tarea.

Use el caso universitario solo para hacer visible el recorrido: ante una consulta de posgrado sobre cancelación, G representa la regla general y E la excepción aplicable; B contiene información de biblioteca. Muestre que G y E se seleccionan entre las fuentes autorizadas, aparecen dentro del contexto y pueden respaldar partes de la respuesta mediante sus identificadores. Los contenidos son hipotéticos y las frases de respuesta son abstractas. Cierre explicando que RAG aporta evidencia a la generación, mientras la corrección y la suficiencia deben comprobarse; el resto de la clase desarrollará esas condiciones.

CONDUCCIÓN · 00:00–02:00 · 2 min · L01, L02

00:00–00:20 · Estado inicial: título, definición, consulta y fuentes.

00:20–00:50 · Paso 1: recuperación, selección de G y E y evidencia seleccionada.

00:50–01:20 · Paso 2: contexto con sus tres compartimentos y las conexiones de consulta y evidencia.

01:20–01:50 · Paso 3: LLM, respuesta abstracta y relaciones de respaldo.

01:50–02:00 · Paso 4: frase final; enlazar con el análisis de la consulta en la siguiente diapositiva.

## Vista visual

![Vista de la diapositiva 3](../assets/slide-003.png)
