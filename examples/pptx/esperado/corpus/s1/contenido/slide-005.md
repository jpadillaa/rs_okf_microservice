---
type: "Slide"
unit_id: "<conversión>/contenido/slide-005.md"
parent: "documento.md"
previous: "contenido/slide-004.md"
next: "contenido/slide-006.md"
title: "1. La ventana de contexto es un presupuesto"
description: "Diapositiva 5 de s1.pptx: 1. La ventana de contexto es un presupuesto. Información disponible para una invocación, medida en tokens"
sources:
  - id: "original"
    resource: "/references/original.pptx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "slide-5"
    source_id: "original"
    kind: "pptx_slide"
    slide_index: 5
    slide_id: 260
    display_label: "05"
block_spans: "/contenido/slide-005.spans.json"
title_inferred: true
kicker: "¿POR QUÉ CONSIDERAR RAG?"
---

# 1. La ventana de contexto es un presupuesto

<!-- okf-generated -->
> Título inferido por tamaño y posición del texto; la diapositiva no tiene marcador de título.

## Contenido

<!-- okf-source-span: f-5 -->
- Información disponible para una invocación, medida en tokens

<!-- okf-source-span: f-6 -->
- Presupuesto de contexto

<!-- okf-source-span: f-7 -->
![Imagen 1 de la diapositiva 5](../assets/slide-005-image-01.svg)

<!-- okf-source-span: f-9 -->
- Instrucciones

<!-- okf-source-span: f-10 -->
- \+ consulta

<!-- okf-source-span: f-12 -->
- Historial

<!-- okf-source-span: f-13 -->
- incluido

<!-- okf-source-span: f-15 -->
- Evidencia

<!-- okf-source-span: f-16 -->
- seleccionada

<!-- okf-source-span: f-18 -->
- Salida

<!-- okf-source-span: f-19 -->
- reservada

<!-- okf-source-span: f-22 -->
- G

<!-- okf-source-span: f-25 -->
- E

<!-- okf-source-span: f-28 -->
- B

<!-- okf-source-span: f-29 -->
- No aporta a esta consulta

<!-- okf-source-span: f-31 -->
- Límite de contexto

<!-- okf-source-span: f-33 -->
- Máximo de salida

<!-- okf-source-span: f-34 -->
- Ambos se verifican en la API del modelo

<!-- okf-source-span: f-36 -->
- RAG selecciona evidencia que también ocupa contexto

<!-- okf-source-span: f-37 -->
- Diagrama esquemático · Los límites dependen del modelo y la API

## Notas del presentador

GUÍA DOCENTE

Explique que el modelo recibe una cantidad limitada de información por invocación y que ese recurso se expresa en tokens. Dentro de la entrada pueden aparecer instrucciones, consulta, historial y evidencia recuperada; también hay que planear la salida y respetar los límites específicos de la API. Una respuesta anterior forma parte de la entrada cuando se incorpora como historial en una nueva solicitud. La documentación actual distingue ventana de contexto y máximo de salida, por lo que no basta con mirar una sola cifra.

Use la barra para mostrar que recuperar documentos también consume contexto. La decisión de diseño es asignar el presupuesto a la información necesaria para responder. Una ventana amplia permite introducir más contenido, pero la selección sigue dependiendo de calidad, latencia y costo. Relacione el punto con el caso: G y E deben conservarse si la pregunta exige regla y excepción, mientras B puede ocupar espacio sin contribuir. Deje la decisión entre documento directo, recuperación o combinación vinculada con las características de la tarea.

CONDUCCIÓN · 02:00–04:00 · 2 min

00:00–00:25 · Estado inicial: flecha y barra con instrucciones y consulta.

00:25–01:00 · Paso 1: historial, evidencia y salida reservada.

01:00–01:35 · Paso 2: G, E y B; destacar la selección.

01:35–02:00 · Paso 3: etiquetas de límites y frase final.

Proporciones esquemáticas (20/25/35/20 %); no son mediciones.

FUENTES

Conversation state, OpenAI: https://developers.openai.com/api/docs/guides/conversation-state

Long context, Google: https://ai.google.dev/gemini-api/docs/long-context

## Vista visual

![Vista de la diapositiva 5](../assets/slide-005.png)
