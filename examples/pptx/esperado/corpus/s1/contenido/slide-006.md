---
type: Slide
title: "1. Historial almacenado ≠ contexto enviado"
description: "Diapositiva 6 de s1.pptx: 1. Historial almacenado ≠ contexto enviado. ESTADO A"
sources:
  - id: original
    resource: /references/original.pptx
generated:
  by: process:document-conversion
  at: "<generado>"
source_spans:
  - id: slide-6
    source_id: original
    kind: pptx_slide
    slide_index: 6
    slide_id: 261
    display_label: "06"
title_inferred: true
kicker: "¿POR QUÉ CONSIDERAR RAG?"
---

# 1. Historial almacenado ≠ contexto enviado

> Título inferido por tamaño y posición del texto; la diapositiva no tiene marcador de título.

## Contenido

- ESTADO A
- Contexto de una invocación
- P1
- Mensaje del usuario
- R1
- Respuesta en historial
- P2
- Mensaje del usuario
- R2
- Respuesta en historial
- ESTADO B
- Contexto seleccionado para la siguiente
- P1
- No enviado en esta invocación
- R1
- Respuesta en historial
- P2
- Mensaje del usuario
- R2
- Respuesta en historial
- P3
- Mensaje del usuario
- Historial conservado por la aplicación
- P1
- R1
- P2
- R2
- P3
- Seleccionar / resumir
- / recuperar

![Imagen 1 de la diapositiva 6](../assets/slide-006-image-01.svg)

- La aplicación decide qué información llega al modelo
- Estrategia de selección ilustrativa; no es un comportamiento universal del LLM

## Notas del presentador

GUÍA DOCENTE

Explique que la aplicación puede conservar una conversación más extensa que la información enviada al modelo en una llamada. Presente los dos estados de la imagen como una selección ilustrativa del historial: al incluir nuevas interacciones, un mensaje anterior queda fuera del contexto elegido. Ese mensaje puede seguir almacenado y recuperarse posteriormente; para la respuesta actual, su contenido no está disponible si no fue incorporado a la entrada. La persistencia y la disponibilidad para una invocación son responsabilidades diferentes.

Sustituya la idea de un olvido automático por una decisión de gestión de contexto. La aplicación puede seleccionar, resumir o recuperar información; también puede recibir un error si supera el límite del servicio. La política depende del sistema y debe conservar información relevante para la tarea. Relacione esta distinción con RAG: recuperar contenido desde un almacenamiento externo permite suministrarlo cuando se necesita, pero exige una selección correcta y no crea una memoria completa por sí mismo.

CONDUCCIÓN · 04:00–06:00 · 2 min

00:00–00:25 · Estado inicial: primera fila.

00:25–00:55 · Paso 1: segunda fila; detenerse en P1.

00:55–01:30 · Paso 2: almacenamiento inferior; el mensaje sigue conservado.

01:30–02:00 · Paso 3: bloque de gestión, conexión con el contexto seleccionado y P1 resaltado en ambos lugares.

FUENTES

Conversation state, OpenAI: https://developers.openai.com/api/docs/guides/conversation-state

Context windows, Anthropic: https://platform.claude.com/docs/en/build-with-claude/context-windows

## Vista visual

![Vista de la diapositiva 6](../assets/slide-006.png)
