---
type: "Slide"
unit_id: "<conversión>/contenido/slide-006.md"
parent: "documento.md"
previous: "contenido/slide-005.md"
next: "contenido/slide-007.md"
title: "1. Historial almacenado ≠ contexto enviado"
description: "Diapositiva 6 de s1.pptx: 1. Historial almacenado ≠ contexto enviado. ESTADO A"
sources:
  - id: "original"
    resource: "/references/original.pptx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "slide-6"
    source_id: "original"
    kind: "pptx_slide"
    slide_index: 6
    slide_id: 261
    display_label: "06"
block_spans: "/contenido/slide-006.spans.json"
title_inferred: true
kicker: "¿POR QUÉ CONSIDERAR RAG?"
---

# 1. Historial almacenado ≠ contexto enviado

<!-- okf-generated -->
> Título inferido por tamaño y posición del texto; la diapositiva no tiene marcador de título.

## Contenido

<!-- okf-source-span: f-5 -->
- ESTADO A

<!-- okf-source-span: f-6 -->
- Contexto de una invocación

<!-- okf-source-span: f-9 -->
- P1

<!-- okf-source-span: f-10 -->
- Mensaje del usuario

<!-- okf-source-span: f-12 -->
- R1

<!-- okf-source-span: f-13 -->
- Respuesta en historial

<!-- okf-source-span: f-15 -->
- P2

<!-- okf-source-span: f-16 -->
- Mensaje del usuario

<!-- okf-source-span: f-18 -->
- R2

<!-- okf-source-span: f-19 -->
- Respuesta en historial

<!-- okf-source-span: f-20 -->
- ESTADO B

<!-- okf-source-span: f-21 -->
- Contexto seleccionado para la siguiente

<!-- okf-source-span: f-24 -->
- P1

<!-- okf-source-span: f-25 -->
- No enviado en esta invocación

<!-- okf-source-span: f-27 -->
- R1

<!-- okf-source-span: f-28 -->
- Respuesta en historial

<!-- okf-source-span: f-30 -->
- P2

<!-- okf-source-span: f-31 -->
- Mensaje del usuario

<!-- okf-source-span: f-33 -->
- R2

<!-- okf-source-span: f-34 -->
- Respuesta en historial

<!-- okf-source-span: f-36 -->
- P3

<!-- okf-source-span: f-37 -->
- Mensaje del usuario

<!-- okf-source-span: f-39 -->
- Historial conservado por la aplicación

<!-- okf-source-span: f-41 -->
- P1

<!-- okf-source-span: f-43 -->
- R1

<!-- okf-source-span: f-45 -->
- P2

<!-- okf-source-span: f-47 -->
- R2

<!-- okf-source-span: f-49 -->
- P3

<!-- okf-source-span: f-51 -->
- Seleccionar / resumir

<!-- okf-source-span: f-52 -->
- / recuperar

<!-- okf-source-span: f-53 -->
![Imagen 1 de la diapositiva 6](../assets/slide-006-image-01.svg)

<!-- okf-source-span: f-55 -->
- La aplicación decide qué información llega al modelo

<!-- okf-source-span: f-56 -->
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
