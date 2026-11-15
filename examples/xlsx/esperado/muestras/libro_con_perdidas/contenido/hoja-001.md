---
type: "Sheet"
unit_id: "f541e2d2167117b92154b84c45a7ca70dbe1228afb2e5c120ece6bba862499ea/contenido/hoja-001.md"
parent: "documento.md"
next: "contenido/hoja-002.md"
title: "Precios"
description: "Hoja 1 del libro libro_con_perdidas.xlsx: rango A1:D8 con 3 regiones."
sources:
  - id: "original"
    resource: "/references/original.xlsx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
status: "draft"
sheet_index: 1
source_spans:
  - id: "hoja"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Precios"
    range: "A1:D8"
  - id: "r-1"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Precios"
    range: "A1:D4"
  - id: "r-2"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Precios"
    range: "A6:B6"
  - id: "r-3"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Precios"
    range: "A8"
---

# Precios

<!-- okf-generated -->
Hoja 1 de 2 · rango con contenido A1:D8 (8 filas × 4 columnas) · 19 celdas con contenido, 10 fórmulas · 3 regiones.

Los valores de las fórmulas son los almacenados en el archivo; el conversor no recalcula.

## Región A1:D4

<!-- okf-source-span: r-1 -->
| Fila | Producto | Precio 2025 | Precio 2026 | Incremento |
| --- | --- | --- | --- | --- |
| 2 | Café especial | 17.2 | \[sin valor almacenado\] | \[sin valor almacenado\] |
| 3 | Cacao en grano | 11.9 | \[sin valor almacenado\] | \[sin valor almacenado\] |
| 4 | Panela orgánica | 4 | \[sin valor almacenado\] | \[sin valor almacenado\] |

**Fórmulas**

- B2:B4: `=[1]Tarifas!B2` en B2; las demás celdas repiten la fórmula con sus referencias relativas desplazadas (B3: `=[1]Tarifas!B3`)
- C2:C4: `=B2*(1+Tasa)` en C2; las demás celdas repiten la fórmula con sus referencias relativas desplazadas (C3: `=B3*(1+Tasa)`) — sin valor almacenado
- D2:D4: `=C2/B2-1` en D2; las demás celdas repiten la fórmula con sus referencias relativas desplazadas (D3: `=C3/B3-1`) — sin valor almacenado

## Región A6:B6

<!-- okf-source-span: r-2 -->
| Fila | A | B |
| --- | --- | --- |
| 6 | Suma de cuadrados | 495.05 |

**Fórmulas**

- B6: `=SUM(B2:B4*B2:B4)` — fórmula matricial sobre B6

## Región A8

<!-- okf-source-span: r-3 -->
- A8: Ficha técnica incrustada

## Gráficos

- **Gráfico 1** (anclado en F1): _gráfico sin datos extraíbles en caché; no se representa._

## Imágenes

<!-- okf-generated -->
_Imagen (anclada en F17) ausente del paquete original._

## Objetos incrustados

- _Objeto incrustado (Word.Document.12) sin representación fiel._

## Formatos numéricos con significado

El valor mostrado es el número almacenado; el formato añade una unidad o un texto que no aparece en él.

- B2:B4: `#,##0.00\ "€"`

