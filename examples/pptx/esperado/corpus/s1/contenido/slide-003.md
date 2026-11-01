---
type: Slide
title: "¿Qué es RAG?"
description: "Diapositiva 3 de s1.pptx: ¿Qué es RAG?. Recuperar información externa, incorporarla al contexto y generar una respuesta con esa evidencia"
sources:
  - id: original
    resource: /references/original.pptx
generated:
  by: process:document-conversion
  at: "<generado>"
source_spans:
  - id: slide-3
    source_id: original
    kind: pptx_slide
    slide_index: 3
    slide_id: 258
    display_label: "03"
title_inferred: true
kicker: "GENERACIÓN AUMENTADA POR RECUPERACIÓN"
---

# ¿Qué es RAG?

> Título inferido por tamaño y posición del texto; la diapositiva no tiene marcador de título.

## Contenido

- Recuperar información externa, incorporarla al contexto y generar una respuesta con esa evidencia

![Imagen 1 de la diapositiva 3](../assets/slide-003-image-01.png)

- 1\. Recuperar
- Seleccionar G y E entre las fuentes autorizadas
- 2\. Ampliar el contexto
- Instrucciones + consulta + evidencia
- 3\. Generar
- El LLM responde citando \[G\] y \[E\]
- La evidencia recuperada forma parte del contexto de esta consulta
- Reglamentos estudiantiles:
- [https://www.uniandes.edu.co/es/la-universidad/secretaria-general/reglamentos-estudiantiles](https://www.uniandes.edu.co/es/la-universidad/secretaria-general/reglamentos-estudiantiles)
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
