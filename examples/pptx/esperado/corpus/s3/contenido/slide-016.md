---
type: "Slide"
unit_id: "<conversión>/contenido/slide-016.md"
parent: "documento.md"
previous: "contenido/slide-015.md"
next: "contenido/slide-017.md"
title: "RAGAs"
description: "Diapositiva 16 de s3.pptx: RAGAs. Evaluación “sin referencias”"
sources:
  - id: "original"
    resource: "/references/original.pptx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "slide-16"
    source_id: "original"
    kind: "pptx_slide"
    slide_index: 16
    slide_id: 271
---

# RAGAs

## Contenido

- Evaluación “sin referencias”
- Solo requiere pregunta, respuesta generada y contextos recuperados.
- Supera el cuello de botella de crear respuestas de referencia y rubricas manualmente.
- Permite evaluación rápida y continua durante el desarrollo y en producción.

## Notas del presentador

Evaluación “sin referencias”: núcleo del enfoque de RAGAs, elimina la necesidad de respuestas preescritas.

Entrada mínima: solo requiere pregunta, respuesta generada y contextos recuperados.

Supera el cuello de botella de crear datos anotados manualmente.

Permite evaluación rápida y continua durante el desarrollo y en producción.

La propuesta de RAGAs se diferencia de la evaluación tradicional principalmente en el uso de referencias humanas y el enfoque del análisis:

Evaluación tradicional (contraparte):

Depende de respuestas de referencia (ground-truth) escritas por humanos.

Utiliza métricas basadas en similitud léxica (como BLEU o ROUGE).

Mide qué tan parecido es el texto generado a una respuesta modelo, sin considerar el contexto ni la veracidad factual.

Requiere conjuntos de datos anotados manualmente, lo que hace el proceso lento y costoso.

Propuesta RAGAs:

Evalúa sin necesidad de referencias humanas (reference-free).

Usa como insumo la tríada pregunta–respuesta–contextos recuperados, centrada en la fidelidad al contenido fuente.

Permite una evaluación continua, automatizada y contextual, alineada con la dinámica iterativa de los sistemas RAG.

Se enfoca en medir relevancia, consistencia factual y adecuación al contexto, no solo similitud textual.

## Vista visual

![Vista de la diapositiva 16](../assets/slide-016.png)
