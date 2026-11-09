---
type: "Source Section"
unit_id: "24198022d6a413fcb668f87da2bbf580c6e623638b82427ac44485d46a2c295e/contenido/seccion-003.md"
parent: "documento.md"
previous: "contenido/seccion-002.md"
next: "contenido/seccion-004.md"
title: "Configuración del índice"
description: "Sección 3 de guia_tecnica.docx: Configuración del índice. Siga estos pasos para crear el índice:"
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
    from: 12
    to: 29
  - id: "b-13"
    source_id: "original"
    kind: "docx_blocks"
    from: 13
    to: 13
  - id: "b-14-18"
    source_id: "original"
    kind: "docx_blocks"
    from: 14
    to: 18
  - id: "b-19"
    source_id: "original"
    kind: "docx_blocks"
    from: 19
    to: 19
  - id: "b-20-21"
    source_id: "original"
    kind: "docx_blocks"
    from: 20
    to: 21
  - id: "b-22"
    source_id: "original"
    kind: "docx_blocks"
    from: 22
    to: 22
  - id: "b-23-24"
    source_id: "original"
    kind: "docx_blocks"
    from: 23
    to: 24
  - id: "b-25"
    source_id: "original"
    kind: "docx_blocks"
    from: 25
    to: 25
  - id: "b-26"
    source_id: "original"
    kind: "docx_blocks"
    from: 26
    to: 26
  - id: "b-27"
    source_id: "original"
    kind: "docx_blocks"
    from: 27
    to: 27
  - id: "b-28"
    source_id: "original"
    kind: "docx_blocks"
    from: 28
    to: 28
  - id: "b-29"
    source_id: "original"
    kind: "docx_blocks"
    from: 29
    to: 29
---

# Configuración del índice

<!-- okf-source-span: b-13 -->
Siga estos pasos para crear el índice:

<!-- okf-source-span: b-14-18 -->
1. Prepare el corpus.
   - a) Elimine duplicados.
   - b) Normalice la codificación a UTF-8.
     - i. Verifique los metadatos obligatorios.
2. Fragmente los documentos.

<!-- okf-source-span: b-19 -->
Antes de continuar, revise que cada fragmento conserve su documento de origen.

<!-- okf-source-span: b-20-21 -->
3. Calcule los embeddings.
4. Cree el índice y verifique su tamaño.

<!-- okf-source-span: b-22 -->
Para la verificación, repita la numeración desde el principio:

<!-- okf-source-span: b-23-24 -->
1. Ejecute las consultas de prueba.
2. Compare los resultados con la referencia.

<!-- okf-source-span: b-25 -->
El tamaño del fragmento influye en la precisión, como muestra el gráfico:

<!-- okf-source-span: b-26 -->
> Datos del gráfico: Precisión por tamaño de fragmento. Precisión: 256 tokens = 0.71; 512 tokens = 0.82; 1024 tokens = 0.76

<!-- okf-source-span: b-27 -->
El código mínimo para crear la colección es:

<!-- okf-source-span: b-28 -->
```
coleccion = cliente.create_collection(
    name="laboratorio3", metric="cosine")
```

<!-- okf-source-span: b-29 -->
> **Importante:** los índices deben reconstruirse cada vez que cambia el modelo de embeddings; los vectores de modelos distintos no son comparables.
