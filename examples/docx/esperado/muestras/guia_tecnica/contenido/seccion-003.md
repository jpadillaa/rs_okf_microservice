---
type: "Source Section"
unit_id: "6ee2ee8af17e8863f6c9fbed37072c6d5cd5a5674ca27eea0ddd0530e3167bef/contenido/seccion-003.md"
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
block_spans: "/contenido/seccion-003.spans.json"
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
