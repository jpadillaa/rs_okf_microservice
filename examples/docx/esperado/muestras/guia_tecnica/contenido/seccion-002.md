---
type: "Source Section"
unit_id: "0895fb23bba4522137a6e84d2610abe97be01c5859b446aabd7763b691954aa1/contenido/seccion-002.md"
parent: "documento.md"
previous: "contenido/seccion-001.md"
next: "contenido/seccion-003.md"
title: "Fundamentos"
description: "Sección 2 de guia_tecnica.docx: Fundamentos. La similitud entre la consulta y cada fragmento se mide con el coseno de sus vectores:"
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
    from: 4
    to: 11
  - id: "b-5"
    source_id: "original"
    kind: "docx_blocks"
    from: 5
    to: 5
  - id: "b-6"
    source_id: "original"
    kind: "docx_blocks"
    from: 6
    to: 6
  - id: "b-7"
    source_id: "original"
    kind: "docx_blocks"
    from: 7
    to: 7
  - id: "b-8"
    source_id: "original"
    kind: "docx_blocks"
    from: 8
    to: 8
  - id: "b-9"
    source_id: "original"
    kind: "docx_blocks"
    from: 9
    to: 9
  - id: "b-10"
    source_id: "original"
    kind: "docx_blocks"
    from: 10
    to: 10
---

# Fundamentos

<!-- okf-source-span: b-5 -->
La similitud entre la consulta y cada fragmento se mide con el coseno de sus vectores:

<!-- okf-source-span: b-6 -->
`cos(q,d)=(∑_(i=1)^n q_i d_i)/(√(∑_(i=1)^n (q_i)^2)√(∑_(i=1)^n (d_i)^2))`

<!-- okf-source-span: b-7 -->
La calidad del recuperador se resume con la medida `F_1=(2PR)/(P+R)`, que combina precisión (P) y exhaustividad (R).

<!-- okf-source-span: b-8 -->
Los vectores se almacenan en un índice aproximado que permite búsquedas rápidas a costa de una pequeña pérdida de exhaustividad. La figura 1 muestra el flujo completo.

> Cuadro de texto:
>
> **Recuerde**
>
> Normalice los vectores antes de indexarlos si usa producto punto.
>
> - Use el mismo modelo para la consulta y los documentos.

<!-- okf-source-span: b-9 -->
![Flujo de indexación: documentos, fragmentación, embeddings e índice vectorial](../assets/imagen-001.png)

<!-- okf-source-span: b-10 -->
*Figura 1. Flujo de indexación.*
