---
type: "Slide"
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
title_inferred: true
kicker: "¿POR QUÉ CONSIDERAR RAG?"
---

# 1. La ventana de contexto es un presupuesto

> Título inferido por tamaño y posición del texto; la diapositiva no tiene marcador de título.

## Contenido

- Información disponible para una invocación, medida en tokens
- Presupuesto de contexto

![Imagen 1 de la diapositiva 5](../assets/slide-005-image-01.svg)

- Instrucciones
- \+ consulta
- Historial
- incluido
- Evidencia
- seleccionada
- Salida
- reservada
- G
- E
- B
- No aporta a esta consulta
- Límite de contexto
- Máximo de salida
- Ambos se verifican en la API del modelo
- RAG selecciona evidencia que también ocupa contexto
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
