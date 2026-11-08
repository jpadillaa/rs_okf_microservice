---
type: "Source Document"
title: "libro_con_perdidas"
description: "Libro libro_con_perdidas.xlsx convertido a conceptos OKF por hojas y regiones."
sources:
  - id: "original"
    resource: "/references/original.xlsx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
converter_version: "0.1.0"
converter_commit: "<commit>"
profile_version: "1.3.0"
conversion_settings: {}
settings_sha256: "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
source_file:
  path: "references/original.xlsx"
  sha256: "782106213febbcffae20d3f21c50c284a0b924d9afae2158ffecce866faf62e8"
source_spans:
  - id: "hoja-1"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Precios"
    range: "A1:D8"
---

# libro\_con\_perdidas

Libro de 2 hojas (0 de gráfico; ocultas: 0) con 19 celdas con contenido y 10 fórmulas.

## Hojas

- [Precios](contenido/hoja-001.md) — A1:D8 · 19 celdas · 3 regiones
  - Región A1:D4
  - Región A6:B6
  - Región A8
- [Borrador](contenido/hoja-002.md) — sin celdas con contenido

## Nombres definidos

- Tasa: `0.04`

## Vínculos externos

Libros de los que leen algunas fórmulas; no se incluyen en el bundle.

- [1]: `file:///C:/Compartido/Tarifas/precios_2025.xlsx`

> El libro pide recalcular al abrirse; los valores almacenados de las fórmulas pueden no estar actualizados.
