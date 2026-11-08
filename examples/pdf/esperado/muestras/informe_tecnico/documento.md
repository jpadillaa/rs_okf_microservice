---
type: "Source Document"
title: "Informe técnico de migración"
description: "Documento fuente informe_tecnico.pdf convertido a conceptos OKF por secciones."
sources:
  - id: "original"
    resource: "/references/original.pdf"
generated:
  by: "process:document-conversion"
  at: "<generado>"
converter_version: "0.1.0"
converter_commit: "<commit>"
profile_version: "1.3.0"
conversion_settings:
  max_image_pixels: 33554432
  ocr:
    enabled: false
settings_sha256: "ad1d7709796a5172b6ad18d538b1bef5780b44279dd787d15ecc82ede959ef16"
source_file:
  path: "references/original.pdf"
  sha256: "b741d4551fe678d52919b803e637eb796420e175113f447ec683dca107d098e3"
source_spans:
  - id: "documento"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 2
  - id: "titulo"
    source_id: "original"
    kind: "pdf_pages"
    from: 1
    to: 1
page_labels:
  "1": "1"
  "2": "2"
---

<!-- okf-source-span: titulo -->
# Informe técnico de migración

Documento de 2 páginas, dividido en 5 conceptos por sus encabezados de nivel 2.

## Estructura

- [Inicio](contenido/seccion-001.md)
- [1\. Contexto](contenido/seccion-002.md) · página 1 (inferido por formato)
- [2\. Arquitectura](contenido/seccion-003.md) · página 1 (inferido por formato)
- [3\. Plan de migración](contenido/seccion-004.md) · página 1 (inferido por formato)
  - 3\.1 Fase de preparación · página 2 (inferido por formato)
  - 3\.2 Fase de ejecución · página 2 (inferido por formato)
- [4\. Riesgos](contenido/seccion-005.md) · página 2 (inferido por formato)

## Propiedades

- Autor: Equipo de pruebas
- Productor del PDF: create\_samples.py
- Título en las propiedades: Informe técnico de migración

## Números de página impresos

Páginas 1 y 2 con número visible; `page_labels` relaciona cada página física con su número impreso.

## Encabezados y pies de página

Texto e imágenes que se repiten en las páginas; no forman parte del flujo del documento.

- Páginas 1 y 2: Informe técnico — Página N de 2
- Páginas 1 y 2: Documento de prueba sin datos reales
