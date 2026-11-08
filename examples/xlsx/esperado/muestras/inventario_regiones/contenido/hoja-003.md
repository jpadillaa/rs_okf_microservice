---
type: "Sheet"
unit_id: "427623ae5ca17b0cbd49f0d3cfc2b9a03dd56ee460a390112404daec49c42997/contenido/hoja-003.md"
title: "Formatos"
description: "Hoja 3 del libro inventario_regiones.xlsx: rango A1:C15 con 1 región."
sources:
  - id: "original"
    resource: "/references/original.xlsx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
sheet_index: 3
source_spans:
  - id: "hoja"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Formatos"
    range: "A1:C15"
  - id: "r-1"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Formatos"
    range: "A1:C15"
---

# Formatos

Hoja 3 de 4 · rango con contenido A1:C15 (15 filas × 3 columnas) · 45 celdas con contenido, 2 fórmulas · 1 región.

Los valores de las fórmulas son los almacenados en el archivo; el conversor no recalcula.

## Región A1:C15

<!-- okf-source-span: r-1 -->
| Fila | Tipo | Valor | Formato aplicado |
| --- | --- | --- | --- |
| 2 | Fecha | 2026-03-15 | dd/mm/aaaa (integrado 14) |
| 3 | Fecha y hora | 2026-03-15 14:30:00 | dd/mm/yyyy hh:mm |
| 4 | Hora | 14:30:00 | h:mm:ss (integrado 21) |
| 5 | Mes y año | 2026-07-01 | mmmm yyyy |
| 6 | Moneda | 1234.5 | #,##0.00 "€" |
| 7 | Porcentaje | 12.5% | 0.00% |
| 8 | Miles | 9876543.21 | #,##0.00 |
| 9 | Científico | 0.000123 | 0.00E+00 |
| 10 | Fracción | 2.75 | # ?/? |
| 11 | Texto con ceros | 00123 | @ |
| 12 | Suma flotante | 0.3 | General; 0,1 + 0,2 |
| 13 | Booleano | TRUE | VERDADERO |
| 14 | Fórmula de texto | Inventario Norte | CONCAT |
| 15 | Celda con \| barra | a \| b | texto con barra vertical |

**Fórmulas**

- B12: `=0.1+0.2`
- B14: `=CONCAT("Inventario ",MID(Almacenes!A1,9,5))`

## Formatos numéricos con significado

El valor mostrado es el número almacenado; el formato añade una unidad o un texto que no aparece en él.

- B6: `#,##0.00\ "€"`

