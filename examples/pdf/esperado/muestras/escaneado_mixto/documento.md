---
type: "Source Document"
unit_id: "543c2c1412db7a0c300f81da2c68c48c60f3df614d32a93a71da8c6240ddd388/documento.md"
title: "Expediente digitalizado"
description: "Documento fuente escaneado_mixto.pdf convertido a conceptos OKF por secciones."
sources:
  - id: "original"
    resource: "/references/original.pdf"
generated:
  by: "process:document-conversion"
  at: "<generado>"
converter_version: "0.1.0"
converter_commit: "<commit>"
profile_version: "1.21.0"
conversion_settings:
  max_image_pixels: 33554432
  ocr:
    enabled: false
settings_sha256: "ad1d7709796a5172b6ad18d538b1bef5780b44279dd787d15ecc82ede959ef16"
conversion_id: "543c2c1412db7a0c300f81da2c68c48c60f3df614d32a93a71da8c6240ddd388"
source_file:
  path: "references/original.pdf"
  sha256: "eb53d8bd714184e9e2feb227d834eec03633bf7b54ab36799b43174d23376bcd"
source_spans:
  - id: "documento"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 5
  - id: "titulo"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 1
page_labels:
  "1": "1"
  "3": "3"
  "4": "4"
  "5": "5"
---

<!-- okf-source-span: titulo -->
# Expediente digitalizado

Documento de 5 páginas, representado en un solo concepto.

## Estructura

- [Texto completo](contenido/texto.md)
    - Registro fotográfico · página 5 (inferido por formato)

## Propiedades

- Productor del PDF: create\_samples.py
- Título en las propiedades: Expediente digitalizado

## Números de página impresos

Páginas 1 y 3–5 con número visible; `page_labels` relaciona cada página física con su número impreso.

## Etiquetas internas del PDF

Etiquetas declaradas en `/PageLabels`; no son necesariamente las impresas.

1 → «Exp-1», 2 → «Exp-2», 3 → «Exp-3», 4 → «Exp-4», 5 → «Exp-5»
