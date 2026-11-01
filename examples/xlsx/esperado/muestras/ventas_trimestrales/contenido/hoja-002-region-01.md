---
type: "Sheet Region"
title: "Tabla «TablaDetalle»"
description: "Región A1:G301 de la hoja «Detalle» del libro ventas_trimestrales.xlsx: 300 filas de datos y 7 columnas, con datos completos en CSV."
sources:
  - id: "original"
    resource: "/references/original.xlsx"
generated:
  by: "process:document-conversion"
  at: "<generado>"
sheet_index: 2
source_spans:
  - id: "region"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Detalle"
    range: "A1:G301"
  - id: "muestra"
    source_id: "original"
    kind: "xlsx_range"
    sheet: "Detalle"
    range: "A1:G11"
---

# Tabla «TablaDetalle»

Región A1:G301 de la hoja [«Detalle»](hoja-002.md): 300 filas de datos y 7 columnas, 2107 celdas con contenido (300 fórmulas). Tabla de Excel «TablaDetalle»; encabezado en la fila 1.

## Esquema

| Columna | Encabezado | Tipos | Celdas con contenido | Fórmulas |
| --- | --- | --- | --- | --- |
| A | Fecha | fecha | 300 | 0 |
| B | Región | texto | 300 | 0 |
| C | Producto | texto | 300 | 0 |
| D | Unidades | número | 300 | 0 |
| E | Precio unitario | número | 300 | 0 |
| F | Importe | número | 300 | 300 |
| G | Entregado | booleano | 300 | 0 |

## Datos completos

- [Valores en CSV](../assets/hoja-002-region-01.csv): las 301 filas del rango A1:G301, en orden y con todas sus columnas, incluidas las filas vacías; la primera fila del CSV es la fila 1 de la hoja. Las fechas están en ISO 8601 y los números con la precisión que muestra Excel.
- [Registros por celda](../assets/hoja-002-region-01-celdas.jsonl): una línea JSON por celda con contenido, con su tipo, valor almacenado, valor mostrado, fórmula, formato y rango combinado.

## Fórmulas

Valores almacenados en el archivo; el conversor no recalcula.

- F2:F301: `=D2*E2` en F2; las demás celdas repiten la fórmula con sus referencias relativas desplazadas (F3: `=D3*E3`)

## Muestra

Primeras 10 de 300 filas de datos; **no es la tabla completa**, que está en el CSV.

<!-- okf-source-span: muestra -->
| Fila | Fecha | Región | Producto | Unidades | Precio unitario | Importe | Entregado |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 2 | 2026-01-02 | Andina | Café especial | 5 | 18.5 | 92.5 | TRUE |
| 3 | 2026-10-12 | Orinoquía | Panela orgánica | 42 | 4.2 | 176.4 | TRUE |
| 4 | 2026-10-08 | Pacífico | Flores de corte | 79 | 22 | 1738 | TRUE |
| 5 | 2026-10-04 | Caribe | Cacao en grano | 116 | 12.75 | 1479 | TRUE |
| 6 | 2026-09-30 | Andina | Aguacate Hass | 33 | 6.9 | 227.7 | FALSE |
| 7 | 2026-09-26 | Orinoquía | Café especial | 70 | 18.5 | 1295 | TRUE |
| 8 | 2026-09-22 | Pacífico | Panela orgánica | 107 | 4.2 | 449.4 | TRUE |
| 9 | 2026-09-18 | Caribe | Flores de corte | 24 | 22 | 528 | TRUE |
| 10 | 2026-09-14 | Andina | Cacao en grano | 61 | 12.75 | 777.75 | TRUE |
| 11 | 2026-09-10 | Orinoquía | Aguacate Hass | 98 | 6.9 | 676.2 | TRUE |
