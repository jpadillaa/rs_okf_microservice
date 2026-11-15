---
type: "Source Section"
unit_id: "f279d058f44cc723e57564c4b8286454993380cfc749859403f9452a5c2dbcb6/contenido/seccion-005.md"
parent: "documento.md"
previous: "contenido/seccion-004.md"
next: "contenido/seccion-006.md"
title: "Semana 5: RAG Avanzado"
description: "Sección 5 de d1.docx: Semana 5: RAG Avanzado. Técnicas de Re-ranking"
sources:
  - id: "original"
    resource: "/references/original.docx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
source_spans:
  - id: "seccion"
    source_id: "original"
    kind: "docx_blocks"
    from: 43
    to: 64
  - id: "b-44"
    source_id: "original"
    kind: "docx_blocks"
    from: 44
    to: 44
  - id: "b-45"
    source_id: "original"
    kind: "docx_blocks"
    from: 45
    to: 45
  - id: "b-46"
    source_id: "original"
    kind: "docx_blocks"
    from: 46
    to: 46
  - id: "b-47-50"
    source_id: "original"
    kind: "docx_blocks"
    from: 47
    to: 50
  - id: "b-51"
    source_id: "original"
    kind: "docx_blocks"
    from: 51
    to: 51
  - id: "b-52-58"
    source_id: "original"
    kind: "docx_blocks"
    from: 52
    to: 58
  - id: "b-59"
    source_id: "original"
    kind: "docx_blocks"
    from: 59
    to: 59
  - id: "b-60-61"
    source_id: "original"
    kind: "docx_blocks"
    from: 60
    to: 61
  - id: "b-62"
    source_id: "original"
    kind: "docx_blocks"
    from: 62
    to: 62
  - id: "b-63-64"
    source_id: "original"
    kind: "docx_blocks"
    from: 63
    to: 64
---

# Semana 5: RAG Avanzado

<!-- okf-source-span: b-44 -->
**Técnicas de Re-ranking**

<!-- okf-source-span: b-45 -->
- Implementación de pasos de reordenamiento (reranking) para filtrar y priorizar los documentos recuperados más relevantes antes de enviarlos al LLM.

<!-- okf-source-span: b-46 -->
**Transformación de Consultas (Query Rewriting)**

<!-- okf-source-span: b-47-50 -->
- Técnicas para reescribir y mejorar el prompt del usuario antes de la búsqueda, asegurando que el sistema entienda mejor la intención y recupere mejor información.
- **Multi-Query Generation**: generar múltiples variaciones de la pregunta
- **Step-back Prompting**: hacer preguntas más generales primero
- **HyDE** (Hypothetical Document Embeddings): generar respuesta hipotética y buscar con ella

<!-- okf-source-span: b-51 -->
**Estrategias avanzadas de recuperación**

<!-- okf-source-span: b-52-58 -->
- Parent-Child Document Retrieval
  - Indexar chunks pequeños, recuperar documentos completos
  - Ventajas: precisión en búsqueda + contexto completo
- Self-Query Retrieval
  - El LLM extrae metadata filters automáticamente del query
  - Ejemplo: "documentos de 2024 sobre IA" → genera filtro {year: 2024, topic: "AI"}
- Busquedas hibridas

<!-- okf-source-span: b-59 -->
**Compresión y Filtrado de Contexto**

<!-- okf-source-span: b-60-61 -->
- Contextual compression (eliminar info irrelevante)
- LLM-based filtering (el modelo decide qué es relevante)

<!-- okf-source-span: b-62 -->
**Refinamiento de Respuestas**

<!-- okf-source-span: b-63-64 -->
- Output Parsers avanzados (Pydantic, JSON mode)
- Citation y Source Attribution (rastrear de dónde viene cada claim)
