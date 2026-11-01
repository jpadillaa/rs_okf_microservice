---
type: Slide
title: "Indexación y embeddings"
description: "Diapositiva 23 de s2.pptx: Indexación y embeddings. Contenido solo visual."
sources:
  - id: original
    resource: /references/original.pptx
generated:
  by: process:document-conversion
  at: "<generado>"
source_spans:
  - id: slide-23
    source_id: original
    kind: pptx_slide
    slide_index: 23
    slide_id: 278
visual_only: true
status: draft
---

# Indexación y embeddings

> Contenido solo visual: lo sustantivo está en la imagen y no tiene transcripción.

## Contenido

![Imagen 1 de la diapositiva 23](../assets/slide-023-image-01.png)

## Notas del presentador

En el post-filtrado, la base de datos realiza primero una búsqueda vectorial para encontrar los $k$ vecinos más cercanos y luego descarta aquellos que no cumplen con los filtros de metadatos. Esta estrategia es rápida porque no altera la mecánica de búsqueda del grafo, pero sufre de una caída dramática en el "recall" cuando los filtros son restrictivos. Si el filtro solo permite el 5% de los datos y se solicitan 20 resultados, es probable que la búsqueda inicial no contenga suficientes candidatos válidos, devolviendo una lista vacía o incompleta al usuario.

El pre-filtrado, en cambio, asegura que el sistema devuelva exactamente $k$ resultados válidos aplicando el filtro antes de la búsqueda vectorial. E

## Vista visual

![Vista de la diapositiva 23](../assets/slide-023.png)
